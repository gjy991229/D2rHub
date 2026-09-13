import { CHATTER_LINES, type ChatterLine } from "./chatterData";
import { PET_ITEM_BY_ID } from "./catalog";
import type { PetOutfit, PetTone } from "./types";
export interface ChatterHistory { recent: string[]; nextAt: number }

/** Build only when outfit/tone changes, not once per keystroke. */
export function createChatterPicker(outfit: PetOutfit, tone: PetTone, history: ChatterHistory = { recent: [], nextAt: 0 }) {
  const tags = new Set(Object.values(outfit).flatMap(id => PET_ITEM_BY_ID.get(id!)?.tags ?? []));
  const allowed = (line: ChatterLine) => tone === "mixed" || line.tone === tone;
  const common = CHATTER_LINES.filter(line => ["common", "cat", "camp"].includes(line.topic) && allowed(line));
  const themed = CHATTER_LINES.filter(line => tags.has(line.topic) && !["cat", "camp"].includes(line.topic) && allowed(line));
  return (english: boolean): string | null => {
    const now = performance.now();
    if (now < history.nextAt) return null;
    history.nextAt = now + 25_000 + Math.random() * 20_000;
    const pool = themed.length && Math.random() < 0.55 ? themed : common;
    const choices = pool.filter(line => !history.recent.includes(line.id));
    const available = choices.length ? choices : common.filter(line => !history.recent.includes(line.id));
    const line = available[Math.floor(Math.random() * available.length)];
    if (!line) return null;
    history.recent.push(line.id);
    if (history.recent.length > 8) history.recent.shift();
    return english ? line.en : line.zh;
  };
}
