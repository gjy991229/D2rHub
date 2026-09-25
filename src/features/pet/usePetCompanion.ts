import { useEffect, useMemo, useRef, useState } from "react";
import { listenEvent } from "../../platform/tauri";
import type { GlobalConfig } from "../../store/types";
import { PET_ITEM_BY_ID } from "./catalog";
import { createChatterPicker, type ChatterHistory } from "./chatter";
import { PET_EVENT_LINES } from "./chatterData";
import { connectWardrobe, settlePetActivity, usePetWardrobe } from "./wardrobeStore";
import type { PetFrame, PetOutfit } from "./types";

export interface PetBubble { id: number; text: string; color: string; phase: "hold" | "float" }
const EMPTY_OUTFIT: PetOutfit = {};
const LEGACY_MAGE: PetOutfit = { head: "circlet" };
export function usePetCompanion(config: GlobalConfig | null) {
  const wardrobe = usePetWardrobe(state => state.snapshot?.wardrobe);
  const equipped = wardrobe?.equipped ?? (config?.bongo_cat_skin === "mage" ? LEGACY_MAGE : EMPTY_OUTFIT);
  const chatterHistory = useRef<ChatterHistory>({ recent: [], nextAt: 0 });
  const picker = useMemo(() => createChatterPicker(equipped, wardrobe?.tone ?? "mixed", chatterHistory.current), [equipped, wardrobe?.tone]);
  const latest = useRef({ config, picker });
  latest.current = { config, picker };
  const [frame, setFrame] = useState<PetFrame>("up");
  const [clickCount, setClickCount] = useState(0);
  const [activeDrops, setActiveDrops] = useState<PetBubble[]>([]);
  const count = useRef(0);
  const bubbleId = useRef(0);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>[]>());
  const mounted = useRef(false);
  const addDrop = (text: string, color: string) => {
    if (!mounted.current) return;
    const id = ++bubbleId.current;
    // Bound both DOM nodes and timers, including bursts of lifecycle events.
    if (timers.current.size >= 3) {
      const oldest = timers.current.keys().next().value;
      if (oldest !== undefined) {
        timers.current.get(oldest)?.forEach(clearTimeout);
        timers.current.delete(oldest);
      }
    }
    const handles: ReturnType<typeof setTimeout>[] = [];
    timers.current.set(id, handles);
    setActiveDrops(previous => [...previous.slice(-2).map(b => ({ ...b, phase: "float" as const })), { id, text, color, phase: "hold" }]);
    const later = (delay: number, fn: () => void) => {
      const timer = setTimeout(() => { if (mounted.current) fn(); }, delay);
      handles.push(timer);
    };
    later(3000, () => setActiveDrops(previous => previous.map(b => b.id === id ? { ...b, phase: "float" } : b)));
    later(12000, () => { timers.current.delete(id); setActiveDrops(previous => previous.filter(b => b.id !== id)); });
  };
  useEffect(() => {
    mounted.current = true;
    const disconnect = connectWardrobe();
    return () => { mounted.current = false; disconnect(); timers.current.forEach(handles => handles.forEach(clearTimeout)); timers.current.clear(); };
  }, []);
  useEffect(() => {
    let cancelled = false;
    let idleTimer: ReturnType<typeof setTimeout> | undefined;
    let progressTimer: ReturnType<typeof setTimeout> | undefined;
    let lastInputAt = 0;
    let lastPaintAt = 0;
    const finishFrame = () => {
      const remaining = 120 - (performance.now() - lastInputAt);
      if (remaining > 0) { idleTimer = setTimeout(finishFrame, remaining); return; }
      idleTimer = undefined;
      setFrame("up"); setClickCount(count.current);
    };
    const settle = async () => {
      try {
        const outcome = await settlePetActivity();
        if (cancelled) return;
        const english = latest.current.config?.app_language === "en-US";
        for (const reward of outcome.rewards) {
          const item = PET_ITEM_BY_ID.get(reward.id);
          const name = item ? english ? item.en : item.name : reward.id;
          // Receipts only come from committed backend rewards, independent of chatter.
          addDrop(reward.duplicate ? `${name} · ${english ? "duplicate, +4 fragments" : "重复装扮，碎片 +4"}` :
            `${reward.achievement ? english ? "Achievement" : "成就解锁" : english ? "New accessory" : "获得装扮"}：${name}`, "#856404");
        }
      } catch { /* Retain progress; the wardrobe shows the storage error. */ }
      finally { progressTimer = undefined; }
    };
    const inputSubscription = listenEvent<string>("global-input-event", event => {
      if (cancelled || !["Keyboard", "MouseLeft", "MouseRight"].includes(event.payload)) return;
      if (!latest.current.config?.enable_bongo_cat) return;
      count.current += 1;
      lastInputAt = performance.now();
      // Coalesce repeat input into at most 20 visual updates/s; no idle RAF loop.
      if (lastInputAt - lastPaintAt >= 50) {
        lastPaintAt = lastInputAt;
        setClickCount(count.current);
        setFrame(Math.random() < 0.5 ? "left" : "right");
      }
      if (!idleTimer) idleTimer = setTimeout(finishFrame, 120);
      if (!progressTimer) progressTimer = setTimeout(() => void settle(), 60_000);
      const { config: current, picker: pick } = latest.current;
      if (current?.bongo_cat_chatterbox !== false) {
        const line = pick(current?.app_language === "en-US");
        if (line) addDrop(line, "#54403b");
      }
    });
    const launchSubscription = listenEvent<{ success: boolean; login_unconfirmed?: boolean }>("launch-ended", event => {
      if (cancelled || latest.current.config?.bongo_cat_chatterbox === false) return;
      if (event.payload.login_unconfirmed) return;
      const english = latest.current.config?.app_language === "en-US";
      const pool = event.payload.success ? PET_EVENT_LINES.launchSuccess : PET_EVENT_LINES.launchFailure;
      const line = pool[Math.floor(Math.random() * pool.length)];
      addDrop(line[english ? 1 : 0], event.payload.success ? "#189f18" : "#b54040");
    });
    const reportListenerError = (error: unknown) => {
      if (!cancelled) usePetWardrobe.setState({ error: String(error) });
    };
    void inputSubscription.catch(reportListenerError);
    void launchSubscription.catch(reportListenerError);
    return () => {
      cancelled = true;
      clearTimeout(idleTimer); clearTimeout(progressTimer);
      void inputSubscription.then(unlisten => unlisten()).catch(() => {});
      void launchSubscription.then(unlisten => unlisten()).catch(() => {});
    };
  }, []);
  return { frame, clickCount, activeDrops, equipped, resetCount: () => { count.current = 0; setClickCount(0); } };
}
