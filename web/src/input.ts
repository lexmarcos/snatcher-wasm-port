// Keyboard + Gamepad API → Genesis 3-button pad bits (must match engine::pad).
// Mouse position/buttons are kept for the light-gun (Justifier) shooting sections.
export const PAD = { UP: 1, DOWN: 2, LEFT: 4, RIGHT: 8, B: 16, C: 32, A: 64, START: 128 } as const;

const KEYMAP: Record<string, number> = {
  ArrowUp: PAD.UP, ArrowDown: PAD.DOWN, ArrowLeft: PAD.LEFT, ArrowRight: PAD.RIGHT,
  KeyZ: PAD.A, KeyX: PAD.B, KeyC: PAD.C, Enter: PAD.START, Space: PAD.B,
  KeyA: PAD.A, KeyS: PAD.B, KeyD: PAD.C,
};

export class Input {
  keys = 0;
  mouseX = 0; mouseY = 0; mouseDown = false;
  /** Scripted inputs for headless tests: window.snatcher.input.script = [[frames, bits], ...] */
  script: [number, number][] = [];

  constructor(canvas: HTMLCanvasElement) {
    addEventListener("keydown", (e) => { const b = KEYMAP[e.code]; if (b) { this.keys |= b; e.preventDefault(); } });
    addEventListener("keyup", (e) => { const b = KEYMAP[e.code]; if (b) { this.keys &= ~b; e.preventDefault(); } });
    addEventListener("blur", () => { this.keys = 0; });
    canvas.addEventListener("mousemove", (e) => {
      const r = canvas.getBoundingClientRect();
      this.mouseX = Math.floor(((e.clientX - r.left) / r.width) * 320);
      this.mouseY = Math.floor(((e.clientY - r.top) / r.height) * 224);
    });
    canvas.addEventListener("mousedown", () => { this.mouseDown = true; });
    addEventListener("mouseup", () => { this.mouseDown = false; });
  }

  poll(): number {
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
