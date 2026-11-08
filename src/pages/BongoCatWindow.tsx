import { useState, useEffect, useRef } from "react";
import { invokeCommand } from "../platform/tauri";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useGlobalConfig, initConfigSync } from "../store/globalConfig";
import { syncThemeFromConfig } from "../store/theme";
import "../features/pet/petUi.css";
import type { GlobalConfig } from "../store/types";
import { shouldApplyConfigCommandResponse } from "../store/configCommitOrdering";
import { PetStage } from "../features/pet/PetStage";
import { usePetCompanion } from "../features/pet/usePetCompanion";
import { changeWardrobe, settlePetActivity } from "../features/pet/wardrobeStore";
import { useWindowPlacementSave } from "../hooks/useWindowPlacementSave";
import { usePreventDragRegionDoubleClick } from "../hooks/useAppEffects";
import {
  restoreWindowPlacement,
  type LegacyWindowGeometry,
} from "../utils/windowPlacement";

export function BongoCatWindow() {
  const { config } = useGlobalConfig();
  const isEnglish = config?.app_language === "en-US";
  const { frame, clickCount, activeDrops, equipped, resetCount } = usePetCompanion(config);
  const placementRestoredRef = useRef(false);
  const markPlacementInteraction = useWindowPlacementSave({
    label: "bongo-cat",
    legacyStorageKey: "d2rhub-cat-position",
  });

  // Load config on mount
  useEffect(() => {
    // Apply font scale on startup
    try {
      const saved = localStorage.getItem("d2rhub-font-scale");
      if (saved && ["small","default","large"].includes(saved)) {
        document.documentElement.dataset.fontScale = saved;
      } else {
        document.documentElement.dataset.fontScale = "default";
      }
    } catch {
      document.documentElement.dataset.fontScale = "default";
    }
    // Subscribe before loading so no main-window commit is missed.
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    initConfigSync().then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Sync font scale from config changes
  useEffect(() => {
    if (!config?.font_scale) return;
    if (["small","default","large"].includes(config.font_scale)) {
      document.documentElement.dataset.fontScale = config.font_scale;
      try { localStorage.setItem("d2rhub-font-scale", config.font_scale); } catch {}
    }
  }, [config?.font_scale]);

  usePreventDragRegionDoubleClick();

  // Apply the final size first, then let the native placement service restore
  // physical coordinates. This avoids restoring against the wrong DPI/size.
  useEffect(() => {
    if (!config) return;
    (async () => {
      try {
        const win = getCurrentWindow();
        const currentScale = config.bongo_cat_scale || 1.0;
        await win.setSize(new LogicalSize(Math.round(240 * currentScale), Math.round(400 * currentScale)));
        if (!placementRestoredRef.current) {
          let legacyGeometry: LegacyWindowGeometry | null = null;
          try {
            const parsed = JSON.parse(localStorage.getItem("d2rhub-cat-position") || "null");
            if (
              parsed
              && Number.isFinite(parsed.x)
              && Number.isFinite(parsed.y)
              && Number.isFinite(parsed.width)
              && Number.isFinite(parsed.height)
            ) {
              legacyGeometry = parsed;
            }
          } catch {}
          await restoreWindowPlacement("bongo-cat", legacyGeometry);
          placementRestoredRef.current = true;
        } else {
          await restoreWindowPlacement("bongo-cat");
        }
      } catch (err) {
        console.error("Failed to size and restore cat window:", err);
      }
    })();
  }, [config?.bongo_cat_scale]);
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const [menuError, setMenuError] = useState<string | null>(null);
  const menuOpen = menuPos !== null;
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => { syncThemeFromConfig(config?.theme); }, [config?.theme]);
  useEffect(() => {
    if (menuOpen) menuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, [menuOpen]);
  useEffect(() => {
    void invokeCommand("pet_set_menu_open", { open: menuOpen }).catch(error => setMenuError(String(error)));
  }, [menuOpen]);
  useEffect(() => {
    const close = () => setMenuPos(null);
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    window.addEventListener("blur", close);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", escape);
      void invokeCommand("pet_set_menu_open", { open: false }).catch(() => {});
    };
  }, []);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setMenuError(null);
    setMenuPos({ x: Math.max(0, Math.min(e.clientX, window.innerWidth - 220)), y: Math.max(0, Math.min(e.clientY, window.innerHeight - 260)) });
  };

  const closeMenu = () => {
    setMenuPos(null);
  };
  const runMenuAction = async (action: () => Promise<unknown>) => {
    try { await action(); closeMenu(); }
    catch (error) { setMenuError(String(error)); }
  };

  const patchPetSettings = async (patch: Partial<Pick<GlobalConfig,
    "enable_bongo_cat"
  >>) => {
    const before = useGlobalConfig.getState().config;
    const saved = await invokeCommand<GlobalConfig>("patch_desktop_pet_settings", { patch });
    if (shouldApplyConfigCommandResponse(before, useGlobalConfig.getState().config)) {
      useGlobalConfig.setState({ config: saved });
    }
  };

  const hideWindow = async () => {
    await settlePetActivity().catch(() => {});
    await patchPetSettings({ enable_bongo_cat: false });
  };

  return (
    <div
      className="select-none relative overflow-visible"
      tabIndex={0}
      aria-label={isEnglish ? "Desktop companion. Press Shift+F10 for actions." : "桌宠，按 Shift+F10 打开操作菜单"}
      onKeyDown={event => {
        if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
          event.preventDefault();
          setMenuPos({ x: 10, y: Math.max(0, window.innerHeight - 280) });
        }
      }}
      onContextMenu={handleContextMenu}
      onClick={closeMenu}
      onPointerDownCapture={(event) => {
        if (
          event.button === 0
          && event.target instanceof Element
          && event.target.closest("[data-tauri-drag-region]")
        ) {
          markPlacementInteraction();
        }
      }}
    >
      <div
        className="relative pointer-events-auto"
        style={{
          width: "240px",
          height: "400px",
          transform: `scale(${(config?.bongo_cat_scale || 1.0)})`,
          transformOrigin: "top left",
          background: "transparent"
        }}
      >
        {/* CSS Animations Stylesheet */}
        <style>{`
          html, body, #root {
            background: transparent !important;
            margin: 0;
            padding: 0;
            overflow: hidden;
          }

          .drop-bubble {
            position: absolute;
            bottom: 145px;
            max-width: 210px;
            background-color: #ffffff;
            border: 2px solid #54403b;
            text-align: center;
            font-weight: 800;
            padding: 8px 12px;
            border-radius: 16px;
            box-shadow: 0 10px 25px -5px rgba(0,0,0,0.1), 0 8px 10px -6px rgba(0,0,0,0.1);
            pointer-events: none;
            z-index: 30;

            /* Transition for transform and opacity */
            transition: transform 9s linear, opacity 9s ease-in-out;
            transform: translateY(0px) scale(1);
            opacity: 1;
          }

          .drop-bubble.float {
            transform: translateY(-220px) scale(0.9);
            opacity: 0;
          }
        `}</style>


        {/* Draggable Bongo Cat Image with Absolute Overlapping Counter */}
        <div
          className="w-full h-full flex flex-col justify-end items-center cursor-move relative"
        >
          <div className="relative flex flex-col items-center justify-center">
            {/* Floating drop text area inside the relative container of the cat */}
            {activeDrops.map((drop) => (
              <div
                key={drop.id}
                className={`drop-bubble ${drop.phase === "float" ? "float" : ""}`}
                style={{
                  color: drop.color,
                }}
              >
                {drop.text}
              </div>
            ))}

            <PetStage frame={frame} equipped={equipped} count={clickCount} draggable />
          </div>
        </div>
      </div>


      {/* Context Menu Dropdown */}
      {menuPos && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={isEnglish ? "Companion actions" : "桌宠操作"}
          className="pet-context-menu absolute z-50"
          style={{ top: menuPos.y, left: menuPos.x, width: Math.min(220, window.innerWidth), maxHeight: Math.max(1, window.innerHeight - menuPos.y), overflowY: "auto" }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={event => {
            const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
            const index = items.indexOf(document.activeElement as HTMLButtonElement);
            const next = event.key === "ArrowDown" ? (index + 1) % items.length
              : event.key === "ArrowUp" ? (index - 1 + items.length) % items.length
                : event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : -1;
            if (next >= 0) { event.preventDefault(); items[next]?.focus(); }
          }}
        >
          <button role="menuitem"
            onClick={() => void runMenuAction(() => invokeCommand("pet_open_wardrobe"))}>
            {isEnglish ? "Open wardrobe" : "打开装扮衣柜"}
          </button>
          {[0, 1, 2].map(index => <button key={index} role="menuitem"
            onClick={() => void runMenuAction(() => changeWardrobe({ kind: "load_preset", index }))}
            >
            {isEnglish ? "Wear outfit" : "穿上搭配"} {index + 1}
          </button>)}
          <button role="menuitem" onClick={() => void runMenuAction(() => changeWardrobe({ kind: "clear" }))}>
            {isEnglish ? "Remove accessories" : "卸下所有装扮"}
          </button>
          <hr />

          <button
            role="menuitem"
            onClick={() => { resetCount(); closeMenu(); }}
          >
            {isEnglish ? "Reset count" : "重置计数"}
          </button>

          <button
            role="menuitem"
            onClick={() => void runMenuAction(hideWindow)}
          >
            {isEnglish ? "Hide companion" : "隐藏桌宠"}
          </button>
          {menuError && <p role="alert" className="px-2 py-1 text-xs text-error break-words">{menuError}</p>}
        </div>
      )}
    </div>
  );
}


