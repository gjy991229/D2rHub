import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/window";
import { useLaunch } from "../store/launch";
import { useAccounts } from "../store/accounts";
import { useGlobalConfig } from "../store/globalConfig";
import { showToast } from "../components/ui/Toast";
import type { AudioModRuntimeWarning, GlobalConfig, LaunchProgress } from "../store/types";
import { validateTrackingTarget } from "../utils/trackingTarget";

async function retryWindowAction(
  action: () => Promise<boolean>,
  isCancelled: () => boolean,
  initialDelayMs: number,
) {
  const wait = (delayMs: number) =>
    new Promise<void>((resolve) => window.setTimeout(resolve, delayMs));

  await wait(initialDelayMs);
  const deadline = Date.now() + 4_000;
  while (!isCancelled() && Date.now() < deadline) {
    if (await action()) return;
    await wait(200);
  }
}

export function useBongoCatWindow(loading: boolean, config: GlobalConfig | null) {
  const prevEnabledRef = useRef(config?.enable_bongo_cat);
  const prevScaleRef = useRef(config?.bongo_cat_scale);

  useEffect(() => {
    if (loading || !config) return;

    let cancelled = false;

    const showCat = async () => {
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        if (cancelled) return true;
        const catWin = await WebviewWindow.getByLabel("bongo-cat");
        if (catWin) {
          if (cancelled) return true;
          await catWin.show();
          await invoke("set_bongo_cat_input_visible", { visible: true }).catch(() => {});
          return true;
        }
      } catch {}
      return false;
    };

    const hideCat = async () => {
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        if (cancelled) return;
        const catWin = await WebviewWindow.getByLabel("bongo-cat");
        if (catWin) {
          if (cancelled) return;
          await catWin.hide();
          await invoke("set_bongo_cat_input_visible", { visible: false }).catch(() => {});
        }
      } catch {}
    };

    if (config.enable_bongo_cat) {
      void retryWindowAction(showCat, () => cancelled, 300);
    } else if (prevEnabledRef.current) {
      // 配置变更：从开启变为关闭 → 隐藏猫咪窗口
      void hideCat();
    }

    prevEnabledRef.current = config.enable_bongo_cat;

    return () => {
      cancelled = true;
    };
  }, [loading, config?.enable_bongo_cat]);

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

export function useOverlayWindow(loading: boolean, config: GlobalConfig | null) {
  useEffect(() => {
    if (loading || !config) return;

    let cancelled = false;

    const manageOverlay = async () => {
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        if (cancelled) return true;
        const windows = [
          { label: "overlay", enabled: config.enable_tz_overlay },
          {
            label: "stats-overlay",
            enabled: config.enable_stats_overlay,
          },
        ];
        let allSettled = true;
        for (const entry of windows) {
          const overlayWin = await WebviewWindow.getByLabel(entry.label);
          if (!overlayWin || cancelled) continue;
          if (entry.enabled) await overlayWin.show();
          else await overlayWin.hide();
          const visible = await overlayWin.isVisible();
          allSettled = allSettled && visible === entry.enabled;
        }
        return allSettled;
      } catch {}
      return false;
    };

    void retryWindowAction(manageOverlay, () => cancelled, 600);

    return () => {
      cancelled = true;
    };
  }, [loading, config?.enable_tz_overlay, config?.enable_stats_overlay]);
}

export function useLaunchEvents(config: GlobalConfig | null) {
  const { launching, results, reset: resetLaunch } = useLaunch();
  const { accounts } = useAccounts();
  const prevLaunchingRef = useRef(launching);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const stopListening = await listen<LaunchProgress>("launch-progress", (event) => {
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
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<AudioModRuntimeWarning>("audio-mod-compatibility-warning", (event) => {
      showToast("warning", event.payload.message);
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!launching && results.length > 0) {
      const t = setTimeout(() => resetLaunch(), 5000);
      return () => clearTimeout(t);
    }
  }, [launching, results.length, resetLaunch]);

  useEffect(() => {
    const wasLaunching = prevLaunchingRef.current;
    prevLaunchingRef.current = launching;
    if (!wasLaunching || launching || !config) return;
    if (!config.rune_audio_enabled) return;

    const target = validateTrackingTarget(config.rune_audio_target_account, accounts);
    if (!target.valid) return;

    const targetResult = results.find(r => r.account_id === target.account.id);
    if (!targetResult || !targetResult.success) return;

    const timer = setTimeout(async () => {
      try {
        await invoke("start_rune_audio_monitor");
        showToast("success", "符文声纹监控已自动启动");
      } catch (e) {
        showToast("error", `符文声纹监控启动失败: ${e}`);
      }
    }, 3000);
    return () => clearTimeout(timer);
  }, [launching, results, config, accounts]);
}

export function useAutoUpdate(
  loading: boolean,
  config: GlobalConfig | null,
  onUpdateAvailable: (version: string, url: string) => void
) {
  useEffect(() => {
    if (loading || !config || !config.first_run_complete || !config.enable_auto_update) return;

    const runAutoCheck = async () => {
      const today = new Date().toISOString().split("T")[0];
      const lastCheck = localStorage.getItem("d2rhub-last-update-check-date");

      if (lastCheck === today) return;
      localStorage.setItem("d2rhub-last-update-check-date", today);

      try {
        interface CloudVersionInfo {
          version: string;
          download_url: string;
        }
        const info = await invoke<CloudVersionInfo>("check_cloud_version");
        const cloudVersion = info.version;
        const downloadUrl = info.download_url;

        const currentVer = await invoke<string>("get_app_version");
        const cleanLocal = currentVer.replace(/^v/, "").trim();
        const cleanCloud = cloudVersion.replace(/^v/, "").trim();

        if (cleanLocal !== cleanCloud) {
          localStorage.setItem("d2rhub-update-available-version", cleanCloud);
          onUpdateAvailable(cloudVersion, downloadUrl);
        } else {
          localStorage.removeItem("d2rhub-update-available-version");
        }
      } catch (err) {
        console.error("启动自动检查更新失败:", err);
      }
    };

    const timer = setTimeout(runAutoCheck, 3000);
    return () => clearTimeout(timer);
  }, [loading, config, onUpdateAvailable]);
}

export function useFirstLaunch(
  loading: boolean,
  config: GlobalConfig | null,
  saveConfig: (cfg: GlobalConfig) => Promise<void>
) {
  const firstLaunchOpenedRef = useRef(false);

  useEffect(() => {
    if (loading || !config) return;
    if (!config.first_launch) return;
    if (firstLaunchOpenedRef.current) return;

    firstLaunchOpenedRef.current = true;

    const timer = setTimeout(async () => {
      try {
        await invoke("open_user_guide");
      } catch {}
      try {
        // 从 store 取最新 config，避免闭包过期覆盖其他字段
        const latestCfg = useGlobalConfig.getState().config;
        if (latestCfg) {
          await saveConfig({ ...latestCfg, first_launch: false });
        }
      } catch {}
    }, 800);

    return () => clearTimeout(timer);
  }, [loading, config?.first_launch, saveConfig]);
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
