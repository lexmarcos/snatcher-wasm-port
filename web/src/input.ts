// Keyboard + Gamepad API → Genesis 3-button pad bits (must match engine::pad).
// The mouse acts as the light gun (Konami Justifier) for the shooting sections.
export const PAD = { UP: 1, DOWN: 2, LEFT: 4, RIGHT: 8, B: 16, C: 32, A: 64, START: 128 } as const;

const KEYMAP: Record<string, number> = {
  ArrowUp: PAD.UP, ArrowDown: PAD.DOWN, ArrowLeft: PAD.LEFT, ArrowRight: PAD.RIGHT,
  KeyZ: PAD.A, KeyX: PAD.B, KeyC: PAD.C, Enter: PAD.START, Space: PAD.B,
  KeyA: PAD.A, KeyS: PAD.B, KeyD: PAD.C,
};

/** Minimal view of the wasm `Game` used here. */
interface GunSink { set_gun(present: boolean, x: number, y: number, trigger: boolean, aux: boolean): void }

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
  set enabled(v: boolean) { this._enabled = v; if (!v) { this.keys = 0; this.mouseButtons = 0; } }
  /** Scripted inputs for headless tests: window.snatcher.input.script = [[frames, bits], ...] */
  script: [number, number][] = [];

  constructor(private canvas: HTMLCanvasElement) {
    addEventListener("keydown", (e) => {
      const b = KEYMAP[e.code];
      // Enter/Space on a focused page control (toolbar button) must activate it, not drive the game
      const t = e.target as HTMLElement | null;
      if ((e.code === "Enter" || e.code === "Space") && t?.matches?.("button, input, select, textarea, a, summary")) return;
      if (b && this._enabled && !e.ctrlKey && !e.metaKey && !e.altKey) { this.keys |= b; this.gunMode = false; e.preventDefault(); }
    });
    addEventListener("keyup", (e) => { const b = KEYMAP[e.code]; if (b) { this.keys &= ~b; if (this._enabled) e.preventDefault(); } });
    addEventListener("blur", () => { this.keys = 0; this.mouseButtons = 0; });
    canvas.addEventListener("mousemove", (e) => this.track(e));
    canvas.addEventListener("mousedown", (e) => {
      if (!this._enabled) return;
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
    const w = this.canvas.width || 256, h = this.canvas.height || 224;
    const clamp = (v: number, hi: number) => Math.max(0, Math.min(hi - 1, v));
    this.mouseX = clamp(Math.floor(((e.clientX - r.left) / r.width) * w), w);
    this.mouseY = clamp(Math.floor(((e.clientY - r.top) / r.height) * h), h);
  }

  /** Pad bits for this frame; also sends the light-gun state to the engine. */
  poll(): number {
    if (!this._enabled) return 0;
    let bits = this.pad();
    if (bits) this.gunMode = false;
    const g = this.game ?? (window as any).snatcher?.game;
    g?.set_gun(this.gunMode, this.mouseX, this.mouseY, (this.mouseButtons & 1) !== 0, (this.mouseButtons & 2) !== 0);
    return bits;
  }

  private pad(): number {
    if (this.script.length) {
      const s = this.script[0];
      const bits = s[1];
      if (--s[0] <= 0) this.script.shift();
      return bits;
    }
    let bits = this.keys;
    for (const gp of navigator.getGamepads?.() ?? []) {
      if (!gp) continue;
      const btn = (i: number) => !!gp.buttons[i]?.pressed;
      if (btn(12) || gp.axes[1] < -0.5) bits |= PAD.UP;
      if (btn(13) || gp.axes[1] > 0.5) bits |= PAD.DOWN;
      if (btn(14) || gp.axes[0] < -0.5) bits |= PAD.LEFT;
      if (btn(15) || gp.axes[0] > 0.5) bits |= PAD.RIGHT;
      if (btn(2)) bits |= PAD.A;
      if (btn(0)) bits |= PAD.B;
      if (btn(1)) bits |= PAD.C;
      if (btn(9)) bits |= PAD.START;
    }
    return bits;
  }
}
