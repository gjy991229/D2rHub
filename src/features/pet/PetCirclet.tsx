import type { PetFrame } from "./types";

// Preserve the original frame-specific band curves; only the gem is translated.
const FRAMES: Record<PetFrame, { band: string; gem: string }> = {
  up: {
    band: "M66 50 Q86 59.5 106 61 M114 60.5 Q130 57 146 45",
    gem: "translate(0 0)",
  },
  left: {
    band: "M60 42 Q80.5 50 101 51.5 M109 51 Q125.5 48 142 38",
    gem: "translate(-5 -9.5)",
  },
  right: {
    band: "M62.5 44 Q82.3 52.5 102.2 54 M110.2 53.5 Q126.1 51.8 142 40",
    gem: "translate(-3.8 -7)",
  },
};
const BAND_LAYERS = [
  { color: "#54403b", width: 7.5 },
  { color: "#ffd166", width: 5.5 },
  { color: "#2b5c6f", width: 3 },
  { color: "#82c0cc", width: 1 },
];

export function PetCirclet({ frame }: { frame: PetFrame }) {
  const pose = FRAMES[frame];
  return <g fill="none" stroke="#54403b" strokeWidth="1" strokeLinecap="butt" strokeLinejoin="miter">
    {BAND_LAYERS.map(layer => <path key={layer.color} d={pose.band} stroke={layer.color} strokeWidth={layer.width} />)}
    <g transform={pose.gem}>
      <polygon points="106,55.4 110.85,58.2 110.85,63.8 106,66.6 101.15,63.8 101.15,58.2"
        fill="#ffec3d" strokeWidth="1.8" strokeLinejoin="round" />
      <path d="M106 61 L106 55.4 M106 61 L110.85 58.2 M106 61 L110.85 63.8 M106 61 L106 66.6 M106 61 L101.15 63.8 M106 61 L101.15 58.2"
        strokeWidth="0.8" />
      <polygon points="106,58.2 108.42,59.6 108.42,62.4 106,63.8 103.58,62.4 103.58,59.6"
        strokeWidth="0.6" strokeLinejoin="round" />
      <g fill="#4a5568" strokeWidth="1.2" strokeLinejoin="round">
        <path d="M100.5 56.5 Q105 56.5 107 59.5 Q104 58.5 100.5 58.5Z" />
        <path d="M99.5 60 Q105 60 108 61 Q105 62 99.5 62Z" />
        <path d="M100.5 65.5 Q105 65.5 107 62.5 Q104 63.5 100.5 63.5Z" />
      </g>
    </g>
  </g>;
}
