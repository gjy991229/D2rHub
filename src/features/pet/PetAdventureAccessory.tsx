import type { PetFrame } from "./types";
import { PET_FRAME_GEOMETRY } from "./petGeometry";

export function PetAdventureAccessory({ id, frame }: { id: string; frame: PetFrame }) {
  const head = frame === "left" ? "translate(-4 -9)" : frame === "right" ? "translate(-4 -5)" : undefined;
  const pulse = frame === "up" ? "translate(0 -18)" : "translate(0 -20)";
  const [lx, ly, rx, ry] = PET_FRAME_GEOMETRY[frame].eyes;
  switch (id) {
    case "captain-hat": return <g transform={head}><path d="M76 44 L77 28 Q107 14 139 30 L140 45Z" fill="#ebe0c6" /><path d="M75 43 Q109 52 143 43 L141 52 Q108 62 73 51Z" fill="#596f89" /><path d="M99 34 L111 34 M105 29 L105 43 M97 39 Q105 49 113 39" fill="none" stroke="#b48b49" strokeWidth="2.5" /></g>;
    case "sun-crown": return <g transform={head}><path d="M75 39 L87 46 L94 29 L109 44 L124 28 L132 44 L145 37 L139 56 Q109 65 80 55Z" fill="#d9b463" /><path d="M82 51 Q109 60 138 51" fill="none" stroke="#fff1ba" strokeWidth="3" /><path d="M109 45 L114 50 L109 55 L104 50Z" fill="#d38362" /></g>;
    case "pirate-patch": return <g><path d={`M${lx - 15} ${ly - 17} L${rx + 19} ${ry + 5}`} stroke="#65535b" strokeWidth="3" /><path d={`M${rx - 10} ${ry - 8} q10 -4 20 0 v10 q-10 14 -20 0Z`} fill="#66596d" /><path d={`M${rx - 4} ${ry - 1} l8 5 m-8 0 l8 -5`} stroke="#dfcda5" strokeWidth="1.5" /></g>;
    case "festival-mask": return <g><path d={`M${lx - 15} ${ly - 10} Q${lx + 10} ${ly - 17} ${(lx + rx) / 2} ${ly - 2} Q${rx - 8} ${ry - 14} ${rx + 15} ${ry - 10} L${rx + 11} ${ry + 10} Q${rx - 3} ${ry + 16} ${(lx + rx) / 2} ${ly + 6} Q${lx + 3} ${ly + 17} ${lx - 11} ${ly + 10}Z`} fill="#a27b99" fillOpacity="0.45" stroke="#af8a52" /><path d={`M${lx - 8} ${ly - 5} q8 -6 16 0 M${rx - 8} ${ry - 5} q8 -6 16 0`} fill="none" stroke="#f3ddad" strokeWidth="2.5" /></g>;
    case "sailor-knot": return <g transform={head}><path d="M73 89 L95 99 L124 91 L112 105 L97 102 L84 105Z" fill="#6c94b0" /><path d="M97 100 L89 117 L101 112 L110 117 L104 100Z" fill="#8bafc3" /><path d="M78 93 L87 101 M117 96 L111 101" stroke="#eee4cb" /><circle cx="100" cy="102" r="4" fill="#eee4cb" /></g>;
    case "heart-locket": return <g transform={head}><path d="M76 91 Q101 107 123 94" fill="none" stroke="#bd9961" /><path d="M101 100 C91 90 86 106 101 116 C116 106 111 90 101 100Z" fill="#d28c8b" /><path d="M94 101 Q92 105 97 108" fill="none" stroke="#f2d5bd" /></g>;
    case "snow-cape": case "festival-cape": return <g transform={head}><path d="M60 64 Q108 44 151 62 L197 112 Q167 119 148 99 L114 90 L76 95 Q53 115 22 109 L46 76Z" fill={id === "snow-cape" ? "#8aadc0" : "#a4657c"} /><path d="M24 108 Q45 115 64 95 M154 99 Q173 116 195 110" fill="none" stroke={id === "snow-cape" ? "#f2ebd9" : "#e6c17b"} strokeWidth={id === "snow-cape" ? 6 : 3} />{id === "snow-cape" ? <path d="M177 92 v13 M171 95 l12 7 M171 102 l12 -7" stroke="#f2ebd9" /> : <><path d="M177 91 L184 101 L177 111 L170 101Z" fill="#e6c17b" /><circle cx="40" cy="103" r="3" fill="#e6c17b" /></>}</g>;
    case "paper-boat": return <g transform={pulse}><path d="M175 81 L187 64 L190 81 L200 70 L205 82 L198 92 L181 92Z" fill="#eee4c9" /><path d="M175 81 L190 85 L205 82 M190 85 L198 92 M187 64 L187 82" fill="none" stroke="#a79b83" /><path d="M178 96 Q184 93 191 96 Q198 99 205 95" fill="none" stroke="#86aebb" /></g>;
    case "music-sprite": return <g transform={pulse}><path d="M187 78 L187 61 L201 58 L201 76 M187 66 L201 63" fill="none" stroke="#b29ac7" strokeWidth="4" /><ellipse cx="182" cy="81" rx="6" ry="4" fill="#b29ac7" /><ellipse cx="196" cy="79" rx="6" ry="4" fill="#b29ac7" /><path d="M177 61 v7 M174 65 h6 M201 89 v7 M198 93 h6" stroke="#e6c17b" /></g>;
    default: return null;
  }
}
