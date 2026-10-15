// Inline SVG button icons for the controls help and the remap screen. Own simple shapes (generic, no logos or icon packs).
// Strings are built from constants only (never from user or game data).
import type { Control } from "./gamepads";

const S = 20;
const wrap = (inner: string, w = S, label = "") =>
  `<svg class="ico" viewBox="0 0 ${w} ${S}" width="${w}" height="${S}" role="img" aria-label="${label}" xmlns="http://www.w3.org/2000/svg">${inner}</svg>`;
const text = (t: string, x: number, y: number, size = 12, fill = "#0b0d11") =>
  `<text x="${x}" y="${y}" text-anchor="middle" font-family="system-ui,Arial,sans-serif" font-weight="700" font-size="${size}" fill="${fill}">${t}</text>`;
const disc = (fill: string, t: string) => wrap(`<circle cx="10" cy="10" r="9" fill="${fill}"/>${text(t, 10, 14.3)}`, S, t);
const ring = (t: string) => wrap(`<circle cx="10" cy="10" r="8.4" fill="#cfd5dc" stroke="#8a93a0" stroke-width="1"/>${text(t, 10, 14.3, 12, "#1b1f27")}`, S, t);
const pill = (t: string, w = 26) => wrap(`<rect x="1" y="3" width="${w - 2}" height="14" rx="6" fill="#4a5362" stroke="#8a93a0"/>${text(t, w / 2, 14, t.length > 2 ? 9 : 10.5, "#eef2f6")}`, w, t);

const XBOX: Record<string, string> = {
  south: disc("#7bc65a", "A"), east: disc("#e0675f", "B"), west: disc("#5f9be0", "X"), north: disc("#e6c24f", "Y"),
};
const SWITCH: Record<string, string> = { south: ring("B"), east: ring("A"), west: ring("Y"), north: ring("X") };
const stroke = (c: string) => `fill="none" stroke="${c}" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"`;
const PS: Record<string, string> = {
  south: wrap(`<path d="M5 5 L15 15 M15 5 L5 15" ${stroke("#6fa8ff")}/>`, S, "Cross"),
  east: wrap(`<circle cx="10" cy="10" r="6" ${stroke("#ff7a7a")}/>`, S, "Circle"),
  west: wrap(`<rect x="4.5" y="4.5" width="11" height="11" ${stroke("#e89ad0")}/>`, S, "Square"),
  north: wrap(`<path d="M10 4 L16 15 L4 15 Z" ${stroke("#6fd6a0")}/>`, S, "Triangle"),
};
const MENU_LINES = `<path d="M5.5 7 H14.5 M5.5 10 H14.5 M5.5 13 H14.5" stroke="#0b0d11" stroke-width="1.6" stroke-linecap="round"/>`;

export type IconSet = "xbox" | "playstation" | "switch" | "keyboard";

export function controlIcon(set: IconSet, c: Control | "Z" | "X" | "C" | "Enter" | "select"): string {
  if (set === "keyboard") {
    const k = String(c);
    const w = k === "Enter" ? 40 : 20;
    return wrap(`<rect x="1" y="2" width="${w - 2}" height="16" rx="3" fill="#cfd5dc" stroke="#8a93a0"/>${text(k, w / 2, 14, k === "Enter" ? 9.5 : 12, "#1b1f27")}`, w, k);
  }
  const face = set === "xbox" ? XBOX : set === "switch" ? SWITCH : PS;
  if (c in face) return face[c as string];
  switch (c) {
    case "l1": return pill(set === "xbox" ? "LB" : set === "playstation" ? "L1" : "L");
    case "r1": return pill(set === "xbox" ? "RB" : set === "playstation" ? "R1" : "R");
    case "l2": return pill(set === "xbox" ? "LT" : set === "playstation" ? "L2" : "ZL");
    case "r2": return pill(set === "xbox" ? "RT" : set === "playstation" ? "R2" : "ZR");
    case "start":
      if (set === "xbox") return wrap(`<circle cx="10" cy="10" r="9" fill="#cfd5dc"/>${MENU_LINES}`, S, "Menu");
      if (set === "playstation") return wrap(`<path d="M3 3 H11 A7 7 0 0 1 11 17 H3 Z" fill="#cfd5dc"/>${MENU_LINES}`, S, "Options");
      return wrap(`<circle cx="10" cy="10" r="9" fill="#cfd5dc"/><path d="M10 5 V15 M5 10 H15" stroke="#0b0d11" stroke-width="2.4" stroke-linecap="round"/>`, S, "Plus");
    case "select":
      if (set === "xbox") return wrap(`<circle cx="10" cy="10" r="9" fill="#cfd5dc"/><rect x="5" y="6.5" width="8" height="7" fill="none" stroke="#0b0d11" stroke-width="1.5"/>`, S, "View");
      if (set === "playstation") return wrap(`<rect x="2" y="4" width="16" height="12" rx="5" fill="#cfd5dc"/><path d="M6 10 H14" stroke="#0b0d11" stroke-width="1.8" stroke-linecap="round"/>`, S, "Share");
      return wrap(`<circle cx="10" cy="10" r="9" fill="#cfd5dc"/><path d="M5.5 10 H14.5" stroke="#0b0d11" stroke-width="2.4" stroke-linecap="round"/>`, S, "Minus");
  }
  return wrap(text("?", 10, 14), S, "?");
}

export const dpadIcon = () => wrap(`<path d="M7.5 2 H12.5 V7.5 H18 V12.5 H12.5 V18 H7.5 V12.5 H2 V7.5 H7.5 Z" fill="#cfd5dc" stroke="#8a93a0"/>`, S, "D-pad");
export const stickIcon = () => wrap(`<circle cx="10" cy="10" r="8" fill="none" stroke="#8a93a0" stroke-width="1.5"/><circle cx="10" cy="10" r="4" fill="#cfd5dc"/>`, S, "Left stick");
export const mouseIcon = () => wrap(`<rect x="4.5" y="1.5" width="11" height="17" rx="5.5" fill="#cfd5dc" stroke="#8a93a0"/><path d="M10 2 V9 M4.5 9 H15.5" stroke="#8a93a0"/>`, S, "Mouse");
