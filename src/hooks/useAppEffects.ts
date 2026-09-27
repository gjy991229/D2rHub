import { useEffect, useRef } from "react";
import { invokeCommand, listenEvent } from "../platform/tauri";
import { LogicalSize } from "@tauri-apps/api/window";
import { useLaunch } from "../store/launch";
import { useAccounts } from "../store/accounts";
import { showToast } from "../components/ui/Toast";
import type { AudioModRuntimeWarning, GlobalConfig, LaunchProgress } from "../store/types";
import type { GlobalConfigPatch } from "../utils/globalConfigPatch";
import { validateTrackingTarget } from "../utils/trackingTarget";

export function useBongoCatWindow(loading: boolean, config: GlobalConfig | null) {
  const prevScaleRef = useRef(config?.bongo_cat_scale);

  // 缩放变更即时生效（无需重启）
  useEffect(() => {
    if (loading || !config?.enable_bongo_cat) return;
    const scale = config.bongo_cat_scale;
    if (scale === prevScaleRef.current) return;
    prevScaleRef.current = scale;
    let cancelled = false;

    (async () => {
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        if (cancelled) return;
        const catWin = await WebviewWindow.getByLabel("bongo-cat");
        if (catWin && !cancelled) {
          // 原始尺寸 240×400，等比缩放
          await catWin.setSize(new LogicalSize(240 * scale, 400 * scale));
        }
      } catch {}
    })();
    return () => {
      cancelled = true;
    };
  }, [loading, config?.enable_bongo_cat, config?.bongo_cat_scale]);
}

export function useLaunchEvents(config: GlobalConfig | null, optionalFeaturesAvailable = true, retainLaunchFailures = false) {
  const { launching, results, reset: resetLaunch } = useLaunch();
  const { accounts } = useAccounts();
  const prevLaunchingRef = useRef(launching);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const stopListening = await listenEvent<LaunchProgress>("launch-progress", (event) => {
          useLaunch.getState().addProgressAndLog(event.payload);
        });
        if (cancelled) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      } catch (err) {
        console.error("Failed to setup launch-progress listener:", err);
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    if (!optionalFeaturesAvailable) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<AudioModRuntimeWarning>("audio-mod-compatibility-warning", (event) => {
      showToast("warning", event.payload.message);
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [optionalFeaturesAvailable]);

  useEffect(() => {
    if (!launching && results.length > 0) {
      if (retainLaunchFailures && results.some(result => !result.success)) return;
      const t = setTimeout(() => resetLaunch(), 5000);
      return () => clearTimeout(t);
    }
  }, [launching, results, resetLaunch, retainLaunchFailures]);

  useEffect(() => {
    const wasLaunching = prevLaunchingRef.current;
    prevLaunchingRef.current = launching;
    if (!optionalFeaturesAvailable || !wasLaunching || launching || !config) return;
    if (!config.rune_audio_enabled) return;

    const target = validateTrackingTarget(config.rune_audio_target_account, accounts);
    if (!target.valid) return;

    const targetResult = results.find(r => r.account_id === target.account.id);
    if (!targetResult || !targetResult.success) return;

    const timer = setTimeout(async () => {
      try {
        await invokeCommand("start_rune_audio_monitor");
        showToast("success", "符文声纹监控已自动启动");
      } catch (e) {
        showToast("error", `符文声纹监控启动失败: ${e}`);
      }
    }, 3000);
    return () => clearTimeout(timer);
  }, [launching, results, config, accounts, optionalFeaturesAvailable]);
}

export function useAutoUpdate(
  loading: boolean,
  config: GlobalConfig | null,
  onUpdateAvailable: (version: string, url: string) => void
) {
  const checking = useRef(false);
  const attempted = useRef(false);
  const onAvailable = useRef(onUpdateAvailable);
  onAvailable.current = onUpdateAvailable;
  useEffect(() => {
    if (loading || !config?.first_run_complete || !config.enable_auto_update || checking.current || attempted.current) return;
    const run = async () => {
      checking.current = true; attempted.current = true;
      const today = new Date().toLocaleDateString("en-CA");
      const check = async (kind: "software" | "resources") => {
        const key = `d2rhub-v2-${kind}-check-date`;
        if (localStorage.getItem(key) === today) return;
        if (kind === "software") {
          const info = await invokeCommand<{ version: string; available: boolean }>("check_software_update");
          if (info.available) {
            localStorage.setItem("d2rhub-update-available-version", info.version);
            onAvailable.current(info.version, "");
          } else localStorage.removeItem("d2rhub-update-available-version");
        } else {
          const notices = await invokeCommand<string[]>("check_mod_resource_updates");
          if (notices.length) showToast("info", `可更新：${notices.join("、")}。请在 Mod 管理 → 下载 Mod 与加工器中更新。`);
        }
        // A network failure must not suppress the next startup's check.
        localStorage.setItem(key, today);
      };
      await Promise.allSettled([check("software"), check("resources")]);
      checking.current = false;
    };
    const timer = setTimeout(() => void run(), 3000);
    return () => clearTimeout(timer);
  }, [loading, config?.first_run_complete, config?.enable_auto_update]);
}

export function useFirstLaunch(
  loading: boolean,
  config: GlobalConfig | null,
  patchConfig: (patch: GlobalConfigPatch) => Promise<GlobalConfig>
) {
  const firstLaunchOpenedRef = useRef(false);

  useEffect(() => {
    if (loading || !config || !config.first_run_complete) return;
    if (!config.first_launch) return;
    if (firstLaunchOpenedRef.current) return;

    firstLaunchOpenedRef.current = true;

    const timer = setTimeout(async () => {
      try {
        await invokeCommand("open_user_guide");
      } catch {}
      try {
        await patchConfig({ first_launch: false });
      } catch {}
    }, 800);

    return () => clearTimeout(timer);
  }, [loading, config?.first_launch, config?.first_run_complete, patchConfig]);
}

export function usePreventDragRegionDoubleClick() {
  useEffect(() => {
    const preventDoubleClick = (e: MouseEvent) => {
      const target = e.target;
      if (!(target instanceof Element)) return;
      const dragRegion = target.closest('[data-tauri-drag-region]') as HTMLElement | null;
      // Overlay owns this double-click gesture (mini/expanded toggle). Its handler
      // still filters buttons and account pills before changing window mode.
      if (target.closest('[data-allow-drag-region-double-click="true"]')) {
        return;
      }
      if (!dragRegion) return;

      // 如果双击目标与拖拽区域之间存在显式声明 no-drag 的元素，放行双击（如悬浮窗账号胶囊）
      let el: Element | null = target;
      while (el && el !== dragRegion) {
        if (
          el instanceof HTMLElement
          && (el.style as CSSStyleDeclaration & { WebkitAppRegion?: string }).WebkitAppRegion === 'no-drag'
        ) {
          return; // 允许双击事件正常触发
        }
        el = el.parentElement;
      }

      e.stopPropagation();
      e.preventDefault();
    };
    window.addEventListener('dblclick', preventDoubleClick, true);
    return () => window.removeEventListener('dblclick', preventDoubleClick, true);
  }, []);
}
