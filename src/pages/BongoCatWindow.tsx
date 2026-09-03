import { useState, useEffect, useRef } from "react";
import { invokeCommand, listenEvent } from "../platform/tauri";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useGlobalConfig, initConfigSync } from "../store/globalConfig";
import type { GlobalConfig } from "../store/types";
import { Lock, Check } from "lucide-react";
import { DROPS, WHITE_DROPS_COMMON, WHITE_DROPS_THEMED } from "./catDropsData";
import { useWindowPlacementSave } from "../hooks/useWindowPlacementSave";
import { usePreventDragRegionDoubleClick } from "../hooks/useAppEffects";
import {
  restoreWindowPlacement,
  type LegacyWindowGeometry,
} from "../utils/windowPlacement";

type Quality = "none" | "white" | "blue" | "yellow" | "green" | "gold" | "divine";

interface ActiveDrop {
  text: string;
  color: string;
  id: number;
  phase: "hold" | "float";
}

export function BongoCatWindow() {
  const { config } = useGlobalConfig();
  const isEnglish = config?.app_language === "en-US";
  const [clickCount, setClickCount] = useState(0);
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
  // Animation frames: 'up' (idle), 'left' (left hit), 'right' (right hit)
  const [frame, setFrame] = useState<"up" | "left" | "right">("up");
  const frameTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Multiple drops array to float independently without interrupting each other
  const [activeDrops, setActiveDrops] = useState<ActiveDrop[]>([]);

  // Context Menu
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);



  // Helper to add a manual text drop popup
  const addDrop = (text: string, color: string) => {
    const newId = Date.now();
    const newDrop: ActiveDrop = {
      text,
      color,
      id: newId,
      phase: "hold"
    };

    setActiveDrops((prev) => {
      // Any older drops currently in 'hold' phase are immediately set to 'float'
      const updated = prev.map((d) =>
        d.phase === "hold" ? { ...d, phase: "float" as const } : d
      );
      return [...updated, newDrop];
    });

    // Timeout to change this specific drop to float phase after 3 seconds
    setTimeout(() => {
      setActiveDrops((prev) =>
        prev.map((d) =>
          d.id === newId ? { ...d, phase: "float" as const } : d
        )
      );
    }, 3000);

    // Timeout to remove this specific drop after 12 seconds total (3s hold + 9s float)
    setTimeout(() => {
      setActiveDrops((prev) => prev.filter((d) => d.id !== newId));
    }, 12000);
  };

  // Global events listener
  useEffect(() => {
    const unlisten = listenEvent<string>("global-input-event", (event) => {
      const type = event.payload;
      if (type !== "Keyboard" && type !== "MouseLeft" && type !== "MouseRight") return;

      // 1. Increment counter
      setClickCount((c) => c + 1);

      // 2. Play frame animation
      if (frameTimeoutRef.current) clearTimeout(frameTimeoutRef.current);

      if (type === "MouseLeft") {
        setFrame("left");
      } else if (type === "MouseRight") {
        setFrame("right");
      } else {
        // Keyboard: random between left and right
        setFrame(Math.random() > 0.5 ? "left" : "right");
      }

      frameTimeoutRef.current = setTimeout(() => {
        setFrame("up");
      }, 120);

      // 3. Roll Drop Rate
      rollDrop();
    });

    return () => {
      unlisten.then((u) => u());
      if (frameTimeoutRef.current) clearTimeout(frameTimeoutRef.current);
    };
  }, []);

  // Listen for launch-ended events from store and trigger success/failure drops
  useEffect(() => {
    const unlistenPromise = listenEvent<{ success: boolean }>("launch-ended", (event) => {
      const { success } = event.payload;
      if (success) {
        addDrop(isEnglish ? "Launch complete!" : "启动成功！", "#189f18"); // Trigger green success pop-up
      } else {
        addDrop(isEnglish ? "Launch failed — check status" : "启动失败...检查一下？", "#ff3333"); // Trigger red/yellow failure pop-up
      }
    });

    return () => {
      unlistenPromise.then((u) => u());
    };
  }, [isEnglish]);



  const rollDrop = () => {
    const rand = Math.random();

    let rolledQuality: Quality = "none";
    let text = "";
    let color = "";

    if (rand < 0.0001) { // 0.01% Divine
      rolledQuality = "divine";
      const list = DROPS.divine;
      text = list[Math.floor(Math.random() * list.length)];
      color = "#ff3399";

      // Special: unlock Mage skin
      if (!config?.bongo_cat_unlocked_skins?.includes("mage")) {
        void patchPetSettings({ bongo_cat_unlocked_skins: [...(config?.bongo_cat_unlocked_skins || []), "mage"] });
      }
    } else if (rand < 0.0004) { // 0.03% Unique/Gold (暗金色)
      rolledQuality = "gold";
      const list = DROPS.gold;
      text = list[Math.floor(Math.random() * list.length)];
      color = "#856404"; // Dark gold (Unique D2 color)
    } else if (rand < 0.0010) { // 0.06% Set/Green (绿色)
      rolledQuality = "green";
      const list = DROPS.green;
      text = list[Math.floor(Math.random() * list.length)];
      color = "#189f18"; // Bright green (Set D2 color)
    } else if (rand < 0.0030) { // 0.2% Rare/Yellow (亮金色)
      rolledQuality = "yellow";
      const list = DROPS.yellow;
      text = list[Math.floor(Math.random() * list.length)];
      color = "#c29900"; // Bright gold-yellow (Rare D2 color)
    } else if (rand < 0.0100) { // 0.7% Blue/Magic (蓝色)
      rolledQuality = "blue";
      const list = DROPS.blue;
      text = list[Math.floor(Math.random() * list.length)];
      color = "#0066cc"; // Magic blue (legible on white)
    } else if (rand < 0.0200) { // 1.0% White/Tucao (白色)
      rolledQuality = "white";
      const skinType = (config?.bongo_cat_skin as "original" | "mage") || "original";
      const themedList = WHITE_DROPS_THEMED[skinType] || [];
      const fullList = [...WHITE_DROPS_COMMON, ...themedList];
      text = fullList[Math.floor(Math.random() * fullList.length)];
      color = "#54403b"; // Cohesive brown color (outline)
    }

    if (rolledQuality !== "none") {
      if (config?.bongo_cat_chatterbox === false && rolledQuality !== "divine") {
        return;
      }
      addDrop(text, color);
    }
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setMenuPos({ x: e.clientX, y: e.clientY });
  };

  const closeMenu = () => {
    setMenuPos(null);
  };

  const patchPetSettings = async (patch: Partial<Pick<GlobalConfig,
    "enable_bongo_cat" | "bongo_cat_skin" | "bongo_cat_unlocked_skins"
  >>) => {
    const saved = await invokeCommand<GlobalConfig>("patch_desktop_pet_settings", { patch });
    useGlobalConfig.setState({ config: saved });
  };

  const hideWindow = async () => {
    try {
      await patchPetSettings({ enable_bongo_cat: false });
    } catch {}
  };

  // Determine correct image URL
  const circletSuffix = config?.bongo_cat_skin === "mage" ? "-circlet" : "";
  const imageUrl = `/bongo-cat-${frame}${circletSuffix}.svg`;

  return (
    <div
      className="select-none relative overflow-visible"
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
            bottom: 120px;
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

            {/* Remove pointer-events-none so it accepts dragging and clicks */}
            <img
              src={imageUrl}
              alt="Bongo Cat"
              className="w-[195px] h-auto drop-shadow-md relative z-20 cursor-move"
              data-tauri-drag-region
            />
            {/* Counter badge directly overlaying the bottom boundary of the cat. z-10 so paws on z-20 are on top. To shift the counter higher, change bottom-[10px] to bottom-[12px], bottom-[14px], etc. */}
            <div className="absolute bottom-[12px] z-10 pointer-events-none flex items-center justify-center">
              <span
                className="w-[148px] h-[22px] flex items-center justify-center bg-[#ffffff] border-2 border-[#54403b] text-[#54403b] rounded-[4px] select-none shadow-sm font-extrabold text-md"
                data-tauri-drag-region
                style={{
                  fontFamily: "'Comic Sans MS', 'Arial', sans-serif"
                }}
              >
                {clickCount}
              </span>
            </div>
          </div>
        </div>
      </div>


      {/* Context Menu Dropdown */}
      {menuPos && (
        <div
          className="absolute z-50 bg-neutral-900/95 border border-neutral-800 rounded-lg p-1 shadow-2xl flex flex-col min-w-[110px]"
          style={{ top: menuPos.y, left: menuPos.x }}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="px-2.5 py-1 text-2xs font-bold text-neutral-500 uppercase tracking-wider select-none">
            {isEnglish ? "Choose skin" : "选择皮肤"}
          </div>
          <button              onClick={() => {
                void patchPetSettings({ bongo_cat_skin: "original" });
                setMenuPos(null);
              }}
              className="w-full flex items-center justify-between text-left px-2.5 py-1.5 rounded text-xs hover:bg-neutral-800 text-neutral-200"
            >
              <span>{isEnglish ? "Original cat" : "原版猫咪"}</span>
              {config?.bongo_cat_skin === "original" && <Check size={10} className="text-accent" />}
            </button>

            <button
              onClick={() => {
                if (config?.bongo_cat_unlocked_skins?.includes("mage")) {
                  void patchPetSettings({ bongo_cat_skin: "mage" });
                }
                setMenuPos(null);
              }}
              disabled={!config?.bongo_cat_unlocked_skins?.includes("mage")}
              className="w-full flex items-center justify-between text-left px-2.5 py-1.5 rounded text-xs hover:bg-neutral-800 text-neutral-200 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <span>{isEnglish ? "Mage cat" : "法师猫咪"}</span>
              {config?.bongo_cat_unlocked_skins?.includes("mage") ? (
                config?.bongo_cat_skin === "mage" && <Check size={10} className="text-accent" />
              ) : (
              <Lock size={9} className="text-neutral-600" />
            )}
          </button>

          <hr className="border-neutral-800 my-1" />

          <button
            onClick={() => { setClickCount(0); closeMenu(); }}
            className="w-full text-left px-2.5 py-1.5 rounded text-xs hover:bg-neutral-800 text-neutral-200"
          >
            {isEnglish ? "Reset count" : "重置计数"}
          </button>

          <button
            onClick={() => { hideWindow(); closeMenu(); }}
            className="w-full text-left px-2.5 py-1.5 rounded text-xs hover:bg-neutral-800 text-red-400 hover:text-red-300"
          >
            {isEnglish ? "Close companion" : "关闭悬浮窗"}
          </button>
        </div>
      )}
    </div>
  );
}
