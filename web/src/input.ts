// Keyboard + Gamepad API → Genesis 3-button pad bits (must match engine::pad).
// The mouse acts as the light gun (Konami Justifier) for the shooting sections.
// Gamepads: family detection, layouts and per-family button maps live in gamepads.ts (docs/re/web.md "Controllers").
import { clientToGame } from "./touchmath";
import {
  assign, cloneMaps, Control, DEFAULT_MAPS, detectFamily, Device, Family, GenesisButton, padBits, PadLike, PadMaps, PAD, readPad, sanitizeMaps, CONTROLS,
} from "./gamepads";
export { PAD };

const KEYMAP: Record<string, number> = {
  ArrowUp: PAD.UP, ArrowDown: PAD.DOWN, ArrowLeft: PAD.LEFT, ArrowRight: PAD.RIGHT,
  KeyZ: PAD.A, KeyX: PAD.B, KeyC: PAD.C, Enter: PAD.START, Space: PAD.B,
  KeyA: PAD.A, KeyS: PAD.B, KeyD: PAD.C,
};

/** Minimal view of the wasm `Game` used here. */
interface GunSink { set_gun(present: boolean, x: number, y: number, trigger: boolean, aux: boolean): void }

/**
 * A touch controller (on-screen pad or "touch the game", web/src/touchpad.ts, touchgame.ts). It feeds the same two paths as the keyboard and the mouse:
 * `bits()` is ORed into the pad bits of the tick, `gun()` replaces the mouse as the light gun while a finger aims.
 */
export interface TouchSource {
  /** Pad bits for this game tick (called once per tick: timed presses advance here). */
  bits(): number;
  /** The light gun for this tick, or null when nothing aims. */
  gun(): { x: number; y: number; trigger: boolean; aux: boolean } | null;
  /** Let go of everything (a menu opened, the page was hidden, a touch was cancelled). */
  releaseAll(): void;
}

export class Input {
  keys = 0;
  /** Mouse position in game screen pixels (0..W-1, 0..H-1; the canvas backing size is the game's). */
  mouseX = 0; mouseY = 0;
  /** bit0 = left button (gun trigger), bit1 = right button (gun second button: draw / holster). */
  mouseButtons = 0;
  /**
   * Light-gun mode: switched on by the first mouse click on the canvas and off again when a pad
   * input (keyboard or gamepad) is used, so the pad keeps working for aiming. While on, the engine
   * runs the shooting sections with the gun rules: the engine converts the pixel position to the raw
   * values the main CPU would report (X = 2 x H counter, Y = V counter; M:0xFF148A / M:0xFF14E0,
   * `gun_raw` in engine/src/menus.rs), subtracts the calibration ($7988/$7989, title screen; default
   * = perfect) and S:0x12516 maps the result to the 3x3 cells (x < 0x60 / < 0xA0, y < 0x30 / < 0x58).
   * Trigger = bit5, second button = bit6 of $7985.
   */
  gunMode = false;
  /** Set by main (`input.game = game`); falls back to `window.snatcher.game`. */
  game: GunSink | null = null;
  /** false while a menu/overlay is open: keys and mouse go to the page, not the game. */
  private _enabled = true;
  get enabled() { return this._enabled; }
  set enabled(v: boolean) { this._enabled = v; if (!v) { this.keys = 0; this.mouseButtons = 0; this.touch?.releaseAll(); } }
  /** The active touch controller (null = touch input is off). */
  touch: TouchSource | null = null;
  /** performance.now() of the last touch handled by a touch controller: the mouse events a browser makes up from a touch are ignored for a moment. */
  lastTouchAt = -1e9;
  /** Scripted inputs for headless tests: window.snatcher.input.script = [[frames, bits], ...] */
  script: [number, number][] = [];
  /** The device the player used last (keyboard, or a specific pad): decides which button icons the game shows. The mouse never changes it. */
  device: Device = { kind: "keyboard" };
  /** Called when `device` changes (also when the pad of the current device changes family). */
  onDevice: ((d: Device) => void) | null = null;
  /** Called when the set of connected pads changes (connect / disconnect). */
  onPads: (() => void) | null = null;
  /** The pad's View / Share / - button: opens the web menu. */
  onSelect: (() => void) | null = null;
  /** Per-family Genesis-button -> physical-control maps (saved by main.ts). */
  maps: PadMaps = cloneMaps(DEFAULT_MAPS);
  /** Genesis bits from the gamepads, refreshed by `scan()` once per animation frame. */
  private padState = 0;
  private selectHeld = new Set<number>();
  private prevDown = new Map<number, Set<string>>();
  /** While set, the next newly pressed assignable control on any pad is passed here instead of being used as game input. */
  private captureCb: ((c: Control, family: Family) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    addEventListener("keydown", (e) => {
      const b = KEYMAP[e.code];
      // Enter/Space on a focused page control (toolbar button) must activate it, not drive the game
      const t = e.target as HTMLElement | null;
      if ((e.code === "Enter" || e.code === "Space") && t?.matches?.("button, input, select, textarea, a, summary")) return;
      if (b && this._enabled && !e.ctrlKey && !e.metaKey && !e.altKey) { this.keys |= b; this.gunMode = false; this.setDevice({ kind: "keyboard" }); e.preventDefault(); }
    });
    addEventListener("keyup", (e) => { const b = KEYMAP[e.code]; if (b) { this.keys &= ~b; if (this._enabled) e.preventDefault(); } });
    addEventListener("blur", () => { this.keys = 0; this.mouseButtons = 0; this.touch?.releaseAll(); });
    addEventListener("gamepadconnected", () => this.onPads?.());
    addEventListener("gamepaddisconnected", (e) => {
      const gp = (e as GamepadEvent).gamepad;
      this.selectHeld.delete(gp.index); this.prevDown.delete(gp.index);
      if (this.device.kind === "pad" && this.device.index === gp.index) this.setDevice({ kind: "keyboard" });
      this.onPads?.();
    });
    canvas.addEventListener("mousemove", (e) => this.track(e));
    canvas.addEventListener("mousedown", (e) => {
      if (!this._enabled || performance.now() - this.lastTouchAt < 1000) return; // a mouse event the browser made from a touch: the touch controller owns it
      this.track(e);
      this.gunMode = true;
      this.mouseButtons |= e.button === 2 ? 2 : 1;
      e.preventDefault();
    });
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    addEventListener("mouseup", (e) => { this.mouseButtons &= ~(e.button === 2 ? 2 : 1); });
  }

  private track(e: MouseEvent) {
    const r = this.canvas.getBoundingClientRect();
    const p = clientToGame(e.clientX, e.clientY, { x: r.left, y: r.top, w: r.width, h: r.height }, this.canvas.width || 256, this.canvas.height || 224);
    this.mouseX = p.x; this.mouseY = p.y;
  }

  /** Pad bits for this frame; also sends the light-gun state to the engine. */
  poll(): number {
    if (!this._enabled) return 0;
    let bits = this.pad();
    const t = this.touch;
    if (t) bits |= t.bits();
    if (bits) this.gunMode = false;
    const g = this.game ?? (window as any).snatcher?.game;
    const tg = t?.gun() ?? null;
    if (tg) g?.set_gun(true, tg.x, tg.y, tg.trigger, tg.aux);
    else g?.set_gun(this.gunMode, this.mouseX, this.mouseY, (this.mouseButtons & 1) !== 0, (this.mouseButtons & 2) !== 0);
    return bits;
  }

  private pad(): number {
    if (this.script.length) {
      const s = this.script[0];
      const bits = s[1];
      if (--s[0] <= 0) this.script.shift();
      return bits;
    }
    return this.keys | this.padState;
  }

  /** Called by the touch controllers: the finger is now the last used device (the game text shows the A B C letters of the on-screen pad). */
  touchDevice() { this.lastTouchAt = performance.now(); this.setDevice({ kind: "touch" }); }

  private setDevice(d: Device) {
    const a = this.device;
    if (a.kind === d.kind && (a.kind !== "pad" || (d.kind === "pad" && a.index === d.index && a.family === d.family && a.id === d.id))) return;
    this.device = d;
    this.onDevice?.(d);
  }

  /** Connected pads with their detected family (for the menu). */
  pads(): { index: number; id: string; family: Family; mapping: string }[] {
    const out = [];
    for (const gp of (navigator.getGamepads?.() ?? []) as (Gamepad | null)[]) if (gp?.connected) out.push({ index: gp.index, id: gp.id, family: detectFamily(gp.id, gp.mapping), mapping: gp.mapping });
    return out;
  }

  /** Ask for the next pressed assignable control (remap screen); `null` cancels. */
  capture(cb: ((c: Control, family: Family) => void) | null) {
    this.captureCb = cb;
    // controls that are already held do not count: wait for a fresh press
  }
  get capturing() { return this.captureCb !== null; }

  /**
   * Read all gamepads once (call every animation frame, also while a menu is open): updates the pad bits, the last-used device, the
   * View/Share/- button and the remap capture. Radial dead zone, D-pad, hat axes: see gamepads.ts.
   */
  scan() {
    let bits = 0;
    const list = (navigator.getGamepads?.() ?? []) as (Gamepad | null)[];
    for (const gp of list) {
      if (!gp || !gp.connected) continue;
      const family = detectFamily(gp.id, gp.mapping);
      const st = readPad(gp as PadLike, family);
      const prev = this.prevDown.get(gp.index) ?? new Set<string>();
      // View / Share / - → web menu (edge)
      if (st.down.has("select") && !this.selectHeld.has(gp.index)) { this.selectHeld.add(gp.index); this.onSelect?.(); }
      else if (!st.down.has("select")) this.selectHeld.delete(gp.index);
      if (this.captureCb) {
        const fresh = CONTROLS.find((c) => st.down.has(c) && !prev.has(c));
        if (fresh) { const cb = this.captureCb; this.captureCb = null; cb(fresh, family); }
      } else {
        bits |= padBits(st, this.maps[family]);
      }
      if (st.active) this.setDevice({ kind: "pad", index: gp.index, family, id: gp.id });
      this.prevDown.set(gp.index, new Set(st.down));
    }
    this.padState = bits;
  }

  /** Set one mapping entry (swapping with the Genesis button that had the control). */
  assign(family: Family, g: GenesisButton, c: Control) {
    this.maps[family] = assign(this.maps[family], g, c);
    this.onDevice?.(this.device);
  }
  setMaps(m: unknown) { this.maps = sanitizeMaps(m); }
  resetMap(family: Family) { this.maps[family] = { ...DEFAULT_MAPS[family] }; this.onDevice?.(this.device); }
}
