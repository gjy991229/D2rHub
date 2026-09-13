import { PetAvatar } from "./PetAvatar";
import type { PetFrame, PetOutfit } from "./types";

/** Keep the counter below the artwork, including accessory hems and paw marks. */
export function PetStage({ frame, equipped, count, draggable = false, width = 195 }: {
  frame: PetFrame; equipped: PetOutfit; count: number; draggable?: boolean; width?: number;
}) {
  const scale = width / 195;
  return <div style={{ position: "relative", display: "flex", flexDirection: "column", alignItems: "center", width }}>
    <PetAvatar frame={frame} equipped={equipped} draggable={draggable} width={width} />
    <div style={{
      position: "relative", zIndex: 10, pointerEvents: "none", boxSizing: "border-box",
      width: 148 * scale, height: 22 * scale, marginTop: -6 * scale,
      display: "flex", alignItems: "center", justifyContent: "center",
      background: "#ffffff", border: `${2 * scale}px solid #54403b`, borderRadius: 4 * scale,
      color: "#54403b", fontFamily: "'Comic Sans MS', Arial, sans-serif", fontWeight: 800,
      fontSize: 16 * scale, lineHeight: 1, whiteSpace: "nowrap",
    }}>{count}</div>
  </div>;
}
