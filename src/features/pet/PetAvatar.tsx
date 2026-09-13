import { memo } from "react";
import { PET_ITEM_BY_ID, PET_SLOTS } from "./catalog";
import { PetAccessory } from "./PetAccessory";
import { PET_CANVAS, PET_FRAME_GEOMETRY } from "./petGeometry";
import type { PetFrame, PetItem, PetOutfit } from "./types";

export const PetAvatar = memo(function PetAvatar({ frame = "up", equipped, draggable = false, width = 195 }: {
  frame?: PetFrame; equipped: PetOutfit; draggable?: boolean; width?: number;
}) {
  const items = PET_SLOTS.map(slot => PET_ITEM_BY_ID.get(equipped[slot.id] ?? ""))
    .filter((item): item is PetItem => item !== undefined);
  const source = PET_FRAME_GEOMETRY[frame];
  const height = width * PET_CANVAS.height / PET_CANVAS.width;
  return <span className="relative z-20 shrink-0 inline-block" style={{ width, height }}
    data-tauri-drag-region={draggable ? true : undefined}>
    <svg viewBox={`0 0 ${PET_CANVAS.width} ${PET_CANVAS.height}`} width={width} height={height} role="img" aria-label="Bongo Cat"
      style={{ pointerEvents: "none", overflow: "visible" }}>
    <g pointerEvents="none" stroke="#54403b" strokeWidth="2" strokeLinejoin="round" strokeLinecap="round">
      {items.filter(item => item.back).map(item => <PetAccessory key={item.id} id={item.id} frame={frame} />)}
      <image href={`/bongo-cat-${frame}.svg`} x="0" y="0" width={source.width} height={source.height} />
      {items.filter(item => !item.back).map(item => <PetAccessory key={item.id} id={item.id} frame={frame} />)}
    </g>
    </svg>
  </span>;
});
