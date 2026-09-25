import { PET_ITEMS } from "./catalog";
import type { PetItem, PetWardrobe } from "./types";

export function ownedCount(w: PetWardrobe) {
  return PET_ITEMS.filter(item => w.owned.includes(item.id)).length;
}
export function achievementProgress(item: PetItem, w: PetWardrobe) {
  switch (item.metric) {
    case "inputs": return w.inputs ?? 0;
    case "owned": return ownedCount(w);
    case "days": return w.days;
    case "seconds": return w.seconds;
    default: return 0;
  }
}
export function achievementLabel(item: PetItem, w: PetWardrobe, english: boolean) {
  const current = Math.min(achievementProgress(item, w), item.target ?? 0);
  const target = item.target ?? 0;
  const number = (value: number) => value.toLocaleString(english ? "en-US" : "zh-CN", { maximumFractionDigits: 1 });
  switch (item.metric) {
    case "seconds": return english ? `${number(current / 3600)} / ${number(target / 3600)} active hours` : `有效陪伴 ${number(current / 3600)} / ${number(target / 3600)} 小时`;
    case "days": return english ? `${number(current)} / ${number(target)} days (non-consecutive)` : `陪伴 ${number(current)} / ${number(target)} 天（无需连续）`;
    case "inputs": return english ? `${number(current)} / ${number(target)} keyboard & mouse inputs` : `键鼠敲击 ${number(current)} / ${number(target)} 次`;
    case "owned": return english ? `${number(current)} / ${number(target)} unique accessories` : `收藏 ${number(current)} / ${number(target)} 件不同饰品`;
    default: return "";
  }
}
