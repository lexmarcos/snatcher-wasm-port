// Gamepad families, layouts, button maps and prompt resolution. Pure functions (no DOM): unit-tested in web/test/gamepads.test.ts.
// Documentation: docs/re/web.md section "Controllers".

export type Family = "xbox" | "playstation" | "switch" | "generic";
export const FAMILIES: Family[] = ["xbox", "playstation", "switch", "generic"];
export const FAMILY_LABEL: Record<Family, string> = { xbox: "Xbox", playstation: "PlayStation", switch: "Nintendo Switch", generic: "Generic gamepad" };

/** Physical controls that can produce a Genesis button. The index is the id shared with the engine (`sncore::prompt::CTL_*`). */
export const CONTROLS = ["south", "east", "west", "north", "l1", "r1", "l2", "r2", "start"] as const;
export type Control = (typeof CONTROLS)[number];
/** Controls that are read but never assigned to the game: select = opens the web menu, home is left to the OS/browser, sticks are not buttons. */
export type Extra = "select" | "home" | "l3" | "r3";
export const ctlId = (c: Control): number => CONTROLS.indexOf(c);

/** Genesis pad bits; must match `engine::pad` and the Rust `btn` constants. */
export const PAD = { UP: 1, DOWN: 2, LEFT: 4, RIGHT: 8, B: 16, C: 32, A: 64, START: 128 } as const;
export type GenesisButton = "A" | "B" | "C" | "START";
export const GENESIS_BITS: Record<GenesisButton, number> = { A: PAD.A, B: PAD.B, C: PAD.C, START: PAD.START };
export const GENESIS_ORDER: GenesisButton[] = ["A", "B", "C", "START"];

// ---------------------------------------------------------------- family detection

export interface ParsedId { vendor: string | null; product: string | null; name: string }

/**
 * Chrome/Edge: "Xbox 360 Controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)" or "Name (Vendor: 054c Product: 09cc)".
 * Firefox: "045e-028e-Microsoft X-Box 360 pad" (Linux, ids zero-padded) or "45e-28e-Controller" (macOS/Windows, not padded).
 * Safari: "Name (Vendor: 054c Product: 09cc)" like Chrome. Anything else: no ids, name only.
 */
export function parseGamepadId(id: string): ParsedId {
  const s = String(id ?? "");
  const pad4 = (h: string) => h.toLowerCase().padStart(4, "0");
  let m = /Vendor:\s*([0-9a-f]{1,4})\s+Product:\s*([0-9a-f]{1,4})/i.exec(s);
  if (m) return { vendor: pad4(m[1]), product: pad4(m[2]), name: s.replace(/\s*\(.*?Vendor:.*?\)\s*$/i, "").trim() };
  m = /^([0-9a-f]{1,4})-([0-9a-f]{1,4})-(.*)$/i.exec(s);
  if (m) return { vendor: pad4(m[1]), product: pad4(m[2]), name: m[3].trim() };
  return { vendor: null, product: null, name: s.trim() };
}

const PS_PRODUCTS = new Set(["05c4", "09cc", "0ce6", "0df2", "0ba0", "0268", "0cda"]); // DS4 v1/v2, DualSense, Edge, DS4 USB adapter, DS3, Portal
const SWITCH_PRODUCTS = new Set(["2009", "2006", "2007", "200e", "2017", "2019"]);     // Pro Controller, Joy-Con L/R, charging grip, SNES/N64 online pads

/** Family from vendor id first (exact), then from name words (some browsers/OSes hide the ids). */
export function detectFamily(id: string, _mapping?: string): Family {
  const { vendor, product, name } = parseGamepadId(id);
  if (vendor === "045e") return "xbox";
  if (vendor === "054c") return "playstation";
  if (vendor === "057e") return "switch";
  void product;
  const n = name.toLowerCase();
  if (/xbox|x-box|xinput|microsoft/.test(n)) return "xbox";
  if (/dualshock|dualsense|playstation|sony|wireless controller|ps[345]\b|ps\(/.test(n)) return "playstation";
  if (/nintendo|switch|joy-?con|pro controller/.test(n)) return "switch";
  return "generic";
}

/** Whether this is a recognised member of its family (debug / docs only). */
export function knownProduct(id: string): boolean {
  const { vendor, product } = parseGamepadId(id);
  return (vendor === "054c" && PS_PRODUCTS.has(product!)) || (vendor === "057e" && SWITCH_PRODUCTS.has(product!)) || vendor === "045e";
}

// ---------------------------------------------------------------- layouts

/** Where each physical control is read from. Buttons by index; triggers may also come from an axis; the D-pad from buttons or a hat axis pair. */
export interface Layout {
  buttons: Partial<Record<Control | Extra, number>>;
  dpadButtons?: { up: number; down: number; left: number; right: number };
  /** D-pad as two axes (x, y; -1/+1): common in Firefox on Linux. */
  hat?: [number, number];
  /** Analog triggers as axes (rest at -1 or 0), used when the layout has no l2/r2 buttons. */
  triggerAxes?: { l2: number; r2: number };
  stick: [number, number];
}

/** W3C "standard" mapping (Chrome/Edge/Firefox when `gamepad.mapping === "standard"`): positional for every family. */
export const STANDARD: Layout = {
  buttons: { south: 0, east: 1, west: 2, north: 3, l1: 4, r1: 5, l2: 6, r2: 7, select: 8, start: 9, l3: 10, r3: 11, home: 16 },
  dpadButtons: { up: 12, down: 13, left: 14, right: 15 },
  stick: [0, 1],
};

/**
 * Raw (non-"standard") layouts per family. These are the Linux joystick-API orders that Firefox exposes when it has no
 * standard mapping for a device (buttons sorted by evdev code, hat as two axes). HYPOTHESIS: from the kernel drivers and the
 * SDL game-controller database; none of these was verified with a real device (docs/re/web.md "Controllers").
 */
export const RAW: Record<Family, Layout> = {
  // xpad: A B X Y LB RB Back Start Guide L3 R3; axes LX LY LT RX RY RT HAT0X HAT0Y
  xbox: { buttons: { south: 0, east: 1, west: 2, north: 3, l1: 4, r1: 5, select: 6, start: 7, home: 8, l3: 9, r3: 10 }, hat: [6, 7], triggerAxes: { l2: 2, r2: 5 }, stick: [0, 1] },
  // hid-playstation / hid-sony: cross circle triangle square L1 R1 L2 R2 Share Options PS L3 R3 (SDL DS4 line: a:b0 b:b1 x:b3 y:b2)
  playstation: { buttons: { south: 0, east: 1, north: 2, west: 3, l1: 4, r1: 5, l2: 6, r2: 7, select: 8, start: 9, home: 10, l3: 11, r3: 12 }, hat: [6, 7], stick: [0, 1] },
  // hid-nintendo: B A X Y Capture L R ZL ZR - + Home LS RS (BTN_SOUTH=B, BTN_EAST=A; capture = BTN_Z sits between Y and L)
  switch: { buttons: { south: 0, east: 1, north: 2, west: 3, l1: 5, r1: 6, l2: 7, r2: 8, select: 9, start: 10, home: 11, l3: 12, r3: 13 }, hat: [6, 7], stick: [0, 1] },
  // unknown pad without a standard mapping: assume the usual SDL/XInput order
  generic: { buttons: { south: 0, east: 1, west: 2, north: 3, l1: 4, r1: 5, l2: 6, r2: 7, select: 8, start: 9, l3: 10, r3: 11 }, hat: [6, 7], stick: [0, 1] },
};

export function layoutFor(family: Family, mapping: string | undefined): Layout {
  return mapping === "standard" ? STANDARD : RAW[family];
}

export interface PadLike {
  id: string; index: number; connected: boolean; mapping: string;
  buttons: ArrayLike<{ pressed: boolean; value?: number }>;
  axes: ArrayLike<number>;
}

export const STICK_DEADZONE = 0.35;
const TRIGGER_AXIS_ON = 0.5;
const HAT_ON = 0.5;
const SIN22 = 0.3827;

/** Radial dead zone, 8 directions (22.5 degree sectors). Returns the four direction flags. */
export function stickDirs(x: number, y: number, dz = STICK_DEADZONE): { up: boolean; down: boolean; left: boolean; right: boolean } {
  const m = Math.hypot(x, y);
  if (!(m >= dz)) return { up: false, down: false, left: false, right: false };
  const nx = x / m, ny = y / m;
  return { up: ny < -SIN22, down: ny > SIN22, left: nx < -SIN22, right: nx > SIN22 };
}

export interface PadState {
  family: Family;
  raw: boolean;
  down: Set<Control | Extra>;
  dir: { up: boolean; down: boolean; left: boolean; right: boolean };
  /** Anything pressed or pushed (buttons, D-pad, stick outside the dead zone). */
  active: boolean;
}

/** Read one gamepad into named controls. */
export function readPad(gp: PadLike, family: Family = detectFamily(gp.id, gp.mapping)): PadState {
  const lay = layoutFor(family, gp.mapping);
  const down = new Set<Control | Extra>();
  const btn = (i: number | undefined) => i !== undefined && !!gp.buttons[i]?.pressed;
  for (const [name, idx] of Object.entries(lay.buttons) as [Control | Extra, number][]) if (btn(idx)) down.add(name);
  const ax = (i: number) => gp.axes[i] ?? 0;
  if (lay.triggerAxes && !lay.buttons.l2) {
    if (ax(lay.triggerAxes.l2) > TRIGGER_AXIS_ON) down.add("l2");
    if (ax(lay.triggerAxes.r2) > TRIGGER_AXIS_ON) down.add("r2");
  }
  const d = lay.dpadButtons;
  const s = stickDirs(ax(lay.stick[0]), ax(lay.stick[1]));
  const dir = {
    up: s.up || (!!d && btn(d.up)), down: s.down || (!!d && btn(d.down)),
    left: s.left || (!!d && btn(d.left)), right: s.right || (!!d && btn(d.right)),
  };
  if (lay.hat && gp.axes.length > Math.max(lay.hat[0], lay.hat[1])) {
    const hx = ax(lay.hat[0]), hy = ax(lay.hat[1]);
    if (hx < -HAT_ON) dir.left = true; else if (hx > HAT_ON) dir.right = true;
    if (hy < -HAT_ON) dir.up = true; else if (hy > HAT_ON) dir.down = true;
  }
  const active = down.size > 0 || dir.up || dir.down || dir.left || dir.right;
  return { family, raw: gp.mapping !== "standard", down, dir, active };
}

// ---------------------------------------------------------------- button maps (layer 2: Genesis button -> physical control)

export type PadMap = Record<GenesisButton, Control>;
export type PadMaps = Record<Family, PadMap>;

/**
 * Defaults. Genesis C is "confirm / fire" under the game's default pad configuration ($78BA = 0), B is "cancel / light" and A is "draw
 * blaster". Xbox A / PlayStation cross (bottom) confirm and B / circle (right) go back (Western convention); the Switch convention is
 * the mirror image: A (right) confirms, B (bottom) goes back. The remaining Genesis button A goes to the left face button
 * (Xbox X, PlayStation square, Switch Y). START is Menu / Options / +.
 */
export const DEFAULT_MAPS: PadMaps = {
  xbox: { C: "south", B: "east", A: "west", START: "start" },
  playstation: { C: "south", B: "east", A: "west", START: "start" },
  switch: { C: "east", B: "south", A: "west", START: "start" },
  generic: { C: "south", B: "east", A: "west", START: "start" },
};
export const cloneMaps = (m: PadMaps): PadMaps => JSON.parse(JSON.stringify(m));

/** Validate a stored map (anything unknown falls back to the default for that family/button). */
export function sanitizeMaps(v: unknown): PadMaps {
  const out = cloneMaps(DEFAULT_MAPS);
  if (!v || typeof v !== "object") return out;
  for (const f of FAMILIES) {
    const src = (v as any)[f];
    if (!src || typeof src !== "object") continue;
    const m: Partial<PadMap> = {};
    for (const g of GENESIS_ORDER) if (CONTROLS.includes(src[g])) m[g] = src[g];
    // one control may drive only one Genesis button: a stored map that breaks this is discarded
    const vals = GENESIS_ORDER.map((g) => m[g] ?? out[f][g]);
    if (new Set(vals).size === vals.length) GENESIS_ORDER.forEach((g, i) => (out[f][g] = vals[i]));
  }
  return out;
}

/** Assign `ctl` to `g`; the Genesis button that had `ctl` gets the old control (swap), so the map stays one-to-one. */
export function assign(map: PadMap, g: GenesisButton, ctl: Control): PadMap {
  const n = { ...map };
  const other = GENESIS_ORDER.find((k) => k !== g && n[k] === ctl);
  if (other) n[other] = n[g];
  n[g] = ctl;
  return n;
}

/** Genesis bits produced by a pad state under a map (D-pad bits included). */
export function padBits(st: PadState, map: PadMap): number {
  let b = 0;
  if (st.dir.up) b |= PAD.UP;
  if (st.dir.down) b |= PAD.DOWN;
  if (st.dir.left) b |= PAD.LEFT;
  if (st.dir.right) b |= PAD.RIGHT;
  for (const g of GENESIS_ORDER) if (st.down.has(map[g])) b |= GENESIS_BITS[g];
  return b;
}

// ---------------------------------------------------------------- prompts (what the engine shows in text)

/** Style numbers shared with the engine (`sncore::prompt::Style`). */
export const STYLE = { genesis: 0, xbox: 1, playstation: 2, switch: 3, keyboard: 4 } as const;
export type Device = { kind: "keyboard" } | { kind: "touch" } | { kind: "pad"; index: number; family: Family; id: string };

/** Keyboard: Genesis A/B/C = Z/X/C (A/S/D and Space also work), START = Enter. Ids: sncore::prompt::KEY_*. */
export const KEYBOARD_IDS: [number, number, number, number] = [0, 1, 2, 3];

export interface Prompt { style: number; ids: [number, number, number, number] }

/** Style and the control ids behind Genesis A, B, C, START for the active device (engine `set_prompt`). */
export function promptFor(dev: Device, maps: PadMaps): Prompt {
  if (dev.kind === "keyboard") return { style: STYLE.keyboard, ids: KEYBOARD_IDS };
  // touch: the on-screen pad is drawn with the Genesis letters (A B C, Start), so the game text keeps its original letters
  if (dev.kind === "touch") return { style: STYLE.genesis, ids: [0, 0, 0, 0] };
  const m = maps[dev.family];
  const style = dev.family === "playstation" ? STYLE.playstation : dev.family === "switch" ? STYLE.switch : STYLE.xbox; // generic pads show Xbox-style icons
  return { style, ids: [ctlId(m.A), ctlId(m.B), ctlId(m.C), ctlId(m.START)] };
}

/** Layer 1 as stored in the game: pad configuration $78BA -> Genesis letter printed for EC ("draw blaster"), EE ("light"), F0 ("fire"). */
export const CONFIG_LETTERS: [GenesisButton, GenesisButton, GenesisButton][] = [
  ["A", "B", "C"], ["A", "C", "B"], ["B", "A", "C"], ["C", "A", "B"], ["C", "B", "A"], ["B", "C", "A"],
];
export type PromptFn = "EC" | "EE" | "F0";
const FN_INDEX: Record<PromptFn, number> = { EC: 0, EE: 1, F0: 2 };

/** Both layers together: which icon (family + physical control) the game shows for a function under `config` on this device. */
export function resolveIcon(config: number, fn: PromptFn, dev: Device, maps: PadMaps): { letter: GenesisButton; set: "xbox" | "playstation" | "switch" | "keyboard" | "touch"; control: Control | "Z" | "X" | "C" | "Enter" | "A" | "B" | "Start" } {
  const letter = CONFIG_LETTERS[Math.max(0, Math.min(5, config))][FN_INDEX[fn]];
  if (dev.kind === "touch") return { letter, set: "touch", control: letter === "START" ? "Start" : letter };
  if (dev.kind === "keyboard") return { letter, set: "keyboard", control: ({ A: "Z", B: "X", C: "C", START: "Enter" } as const)[letter] };
  const set = dev.family === "playstation" ? "playstation" : dev.family === "switch" ? "switch" : "xbox";
  return { letter, set, control: maps[dev.family][letter] };
}

/** Human names of the controls per icon set (for the help panel and the remap screen). */
export const CONTROL_NAMES: Record<"xbox" | "playstation" | "switch", Record<Control, string>> = {
  xbox: { south: "A", east: "B", west: "X", north: "Y", l1: "LB", r1: "RB", l2: "LT", r2: "RT", start: "Menu" },
  playstation: { south: "Cross", east: "Circle", west: "Square", north: "Triangle", l1: "L1", r1: "R1", l2: "L2", r2: "R2", start: "Options" },
  switch: { south: "B", east: "A", west: "Y", north: "X", l1: "L", r1: "R", l2: "ZL", r2: "ZR", start: "+" },
};
export const SELECT_NAMES = { xbox: "View", playstation: "Share / Create", switch: "-" } as const;
export const setOf = (f: Family): "xbox" | "playstation" | "switch" => (f === "playstation" ? "playstation" : f === "switch" ? "switch" : "xbox");
