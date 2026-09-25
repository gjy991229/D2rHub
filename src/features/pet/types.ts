export type PetFrame = "up" | "left" | "right";
export type PetSlot = "head" | "face" | "neck" | "body" | "hands" | "companion";
export type PetTone = "mixed" | "gentle" | "snarky";
export interface PetItem {
  id: string; name: string; en: string; slot: PetSlot; tags: string[];
  source: "starter" | "random" | "achievement"; weight: number; cost: number;
  metric?: "seconds" | "days" | "inputs" | "owned"; target?: number; bonus_fragments?: number; back?: boolean; motion?: "paws" | "pulse";
}
export type PetOutfit = Partial<Record<PetSlot, string>>;
export interface PetWardrobe {
  owned: string[]; equipped: PetOutfit; presets: PetOutfit[];
  seconds: number; inputs: number; days: number; last_day: string; daily_rolls: number;
  roll_seconds: number; misses: number; fragments: number; tone: PetTone;
}
export interface PetSnapshot { generation: number; revision: number; wardrobe: PetWardrobe }
export interface PetReward { id: string; duplicate: boolean; achievement: boolean; bonus_fragments?: number }
export interface PetOutcome { snapshot: PetSnapshot; rewards: PetReward[] }
export type PetAction =
  | { kind: "equip"; id: string }
  | { kind: "unequip"; slot: PetSlot }
  | { kind: "clear" }
  | { kind: "redeem"; id: string }
  | { kind: "tone"; tone: PetTone }
  | { kind: "save_preset" | "load_preset"; index: number };
