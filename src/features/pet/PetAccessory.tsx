import type { PetFrame } from "./types";
import { PetCirclet } from "./PetCirclet";
import { PET_FRAME_GEOMETRY } from "./petGeometry";

/** Accessories share native SVG units on the normalized 208 × 135 canvas. */
export function PetAccessory({ id, frame }: { id: string; frame: PetFrame }) {
  const head = frame === "left" ? "translate(-4 -9)" : frame === "right" ? "translate(-4 -5)" : undefined;
  const pulse = frame === "up" ? "translate(0 -18)" : "translate(0 -20)";
  const [eyeLeftX, eyeLeftY, eyeRightX, eyeRightY] = PET_FRAME_GEOMETRY[frame].eyes;
  if (id === "circlet") return <PetCirclet frame={frame} />;
  switch (id) {
    case "cloth-cap": return <g transform={head}><path d="M76 45 Q84 16 114 28 Q137 32 143 48 L115 55Z" fill="#b9bba2" /><path d="M74 45 Q113 59 145 48 L149 54 Q112 64 73 51Z" fill="#727e65" /></g>;
    case "amazon-band": return <g transform={head}><path d="M74 48 Q109 61 145 48 L145 54 Q110 68 73 54Z" fill="#b48c4e" /><path d="M140 49 Q130 17 153 9 Q162 34 140 49Z" fill="#e5d9be" /><path d="M141 47 L150 17" fill="none" /><circle cx="106" cy="58" r="4" fill="#77bcae" /></g>;
    case "barbarian-horns": return <g transform={head}><path d="M76 43 Q104 18 138 40 L143 52 Q108 46 75 49Z" fill="#8b9aa1" /><path d="M82 39 Q60 41 60 17 Q70 30 87 30Z M130 37 Q151 42 157 23 Q142 31 129 29Z" fill="#efe1bd" /><path d="M104 28 L104 48" stroke="#dfcaa1" strokeWidth="4" /></g>;
    case "druid-antlers": return <g transform={head}><path d="M77 50 Q108 60 141 49" fill="none" stroke="#6b936b" strokeWidth="7" /><path d="M85 48 L80 30 L66 20 M80 30 L84 16 M73 25 L65 32 M130 49 L139 30 L150 20 M139 30 L134 16 M143 26 L155 31" fill="none" stroke="#8d6746" strokeWidth="5" /><path d="M96 48 Q91 32 80 39 Q80 50 96 48Z" fill="#91b370" /></g>;
    case "traveler-hood": return <g transform={head}><path d="M66 60 Q68 29 102 13 Q113 27 124 32 Q140 37 148 57 L134 62 Q126 44 106 43 Q85 39 79 65Z" fill="#687c77" /><path d="M71 60 Q81 34 106 38 Q131 39 141 59" fill="none" stroke="#b9c3a5" strokeWidth="3" /><path d="M102 16 Q99 27 106 38" fill="none" stroke="#4d625b" /></g>;
    case "glasses": return <g fill="none" stroke="#6b5950"><circle cx={eyeLeftX} cy={eyeLeftY} r="10" /><circle cx={eyeRightX} cy={eyeRightY} r="10" /><path d={`M${eyeLeftX + 10} ${eyeLeftY} Q${(eyeLeftX + eyeRightX) / 2} ${eyeLeftY - 9} ${eyeRightX - 10} ${eyeRightY} M${eyeLeftX - 10} ${eyeLeftY - 2} l-4 -4 M${eyeRightX + 10} ${eyeRightY - 1} l5 -2`} /></g>;
    case "red-scarf": return <g transform={head}><path d="M72 89 Q96 99 125 91 L129 98 Q100 109 70 96Z" fill="#c96862" /><path d="M111 97 L117 118 L128 115 L122 96Z" fill="#de8274" /><path d="M118 108 L125 106" fill="none" /></g>;
    case "bell": return <g transform={head}><path d="M77 91 Q99 102 122 94" fill="none" stroke="#8cb29c" strokeWidth="4" /><circle cx="101" cy="101" r="7" fill="#e4b761" /><path d="M97 102 L105 102 M101 102 L101 107" fill="none" /></g>;
    case "necromancer-skull": return <g transform={head}><path d="M76 91 Q98 107 122 94" fill="none" stroke="#9c9286" /><path d="M91 102 Q89 92 100 93 Q111 93 109 103 L106 106 L106 111 L94 111 L94 106Z" fill="#ece4d0" /><circle cx="96" cy="101" r="2" fill="#54403b" /><circle cx="104" cy="101" r="2" fill="#54403b" /><path d="M99 107 L99 111 M102 107 L102 111" /></g>;
    case "bronze-charm": return <g transform={head}><path d="M76 91 Q100 107 124 94" fill="none" stroke="#aa8860" /><path d="M101 95 L111 104 L102 114 L92 104Z" fill="#c39761" /><path d="M98 103 L101 107 L106 101" fill="none" /></g>;
    case "paladin-cape": return <g transform={head}><path d="M60 64 Q108 44 151 62 L197 112 Q167 119 148 99 L114 90 L76 95 Q53 115 22 109 L46 76Z" fill="#7f99b0" /><path d="M151 69 L191 108 L179 113 L143 72Z" fill="#d4b875" /><path d="M183 98 L183 109 M178 103 L188 103" fill="none" stroke="#fff0c4" strokeWidth="3" /><path d="M51 82 L33 105 M153 84 L165 106" fill="none" stroke="#58758f" /></g>;
    case "star-cape": return <g transform={head}><path d="M60 64 Q108 44 151 62 L197 112 Q167 119 148 99 L114 90 L76 95 Q53 115 22 109 L46 76Z" fill="#665b8b" /><path d="M183 96 L185 102 L191 104 L185 106 L183 112 L181 106 L175 104 L181 102Z" fill="#e9dca7" /><circle cx="40" cy="105" r="2" fill="#e9dca7" stroke="none" /><path d="M51 82 L33 105 M153 84 L161 101" fill="none" stroke="#9185b1" /></g>;
    case "assassin-wraps": return <g>{(["left", "right"] as const).map(side => {
      const x = side === "left" ? 43 : 143;
      const y = frame === side ? 94 : frame === "up" ? 85 : 78;
      return <g key={side} transform={`translate(${x} ${y}) rotate(${frame === side ? -20 : 25})`}><rect x="-9" y="-4" width="18" height="8" rx="3" fill="#817490" /><path d="M-4 -3 L0 3 M2 -3 L6 3" stroke="#d9c8dd" fill="none" /></g>;
    })}</g>;
    case "warlock-tome": return <g transform={pulse}><path d="M179 61 L201 57 L204 88 L181 93Z" fill="#70566b" /><path d="M183 88 L202 84 L204 88 L181 93Z" fill="#e2ccb0" /><path d="M188 66 L196 77 L186 80 L193 64" fill="none" stroke="#a6c493" /></g>;
    case "little-ghost": return <g transform={pulse}><path d="M180 92 L180 74 Q180 58 192 58 Q204 58 204 74 L204 92 L198 88 L192 94 L187 89Z" fill="#d9e3de" /><circle cx="187" cy="73" r="2" fill="#54403b" /><circle cx="197" cy="73" r="2" fill="#54403b" /><path d="M189 80 Q192 83 195 80" fill="none" /></g>;
    case "rune-lo": case "rune-ber": case "rune-jah": return <g transform={pulse}><path d="M180 62 L196 58 L204 67 L201 90 L184 94 L176 84Z" fill={id === "rune-jah" ? "#b6bdc4" : id === "rune-ber" ? "#b8bea7" : "#c2ad96"} /><path d="M183 65 L194 62 M182 86 L187 89" fill="none" stroke="#e8ddc5" /><path d={id === "rune-jah" ? "M185 69 L194 69 L190 83 M182 78 L198 75" : id === "rune-ber" ? "M185 68 L185 85 L196 79 L185 74 L196 68" : "M185 68 L185 83 L195 83 M192 69 L196 74 L191 78"} fill="none" stroke="#72502e" strokeWidth="2.5" /></g>;
    default: return null;
  }
}


