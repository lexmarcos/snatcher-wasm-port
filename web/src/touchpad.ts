// Mode 1, "on-screen pad": a virtual Sega CD pad (D-pad, A, B, C, Start) drawn on the page. It feeds the same input path as the keyboard and the
// gamepads (`Input.touch.bits()` is ORed into the pad bits of every game tick), so the game cannot tell it from a real pad.
//   * Multi-touch: every finger is tracked on its own; the bits are the union of all fingers.
//   * A finger that starts on the D-pad keeps steering it wherever it goes (like a thumb stick); any other finger is hit-tested on every move,
//     so sliding from one button to another presses the second and lets go of the first, and lifting anywhere lets go.
//   * Nothing can stay pressed: pointerup, pointercancel, lost capture, touchcancel, the page being hidden, a lost focus and a modal opening all release.
//   * A press shorter than one game tick is still seen for one tick (latch).
import type { TouchSource } from "./input";
import { dpadBits } from "./touchmath";
import { haptic } from "./platform";

interface Ptr { id: number; ownsDpad: boolean; bits: number }

export class TouchPad implements TouchSource {
  /** bits held by fingers right now */
  held = 0;
  private latch = 0;
  private ptrs = new Map<number, Ptr>();
  private btns: HTMLElement[];
  private arms: HTMLElement[];
  private dpad: HTMLElement;
  /** haptic tick on a new press (Touch tab) */
  haptics = false;
  /** a finger went down on the pad: the touch is now the last used device */
  onTouch: (() => void) | null = null;
  /** set by the owner while the pad is not shown or the game is not listening */
  active = true;

  constructor(private root: HTMLElement) {
    this.dpad = root.querySelector<HTMLElement>("#tDpad")!;
    this.arms = Array.from(root.querySelectorAll<HTMLElement>(".arm"));
    this.btns = Array.from(root.querySelectorAll<HTMLElement>(".pbtn"));
    const on = (type: string, fn: (e: PointerEvent) => void) => root.addEventListener(type, fn as EventListener);
    on("pointerdown", (e) => this.down(e));
    on("pointermove", (e) => { if (this.ptrs.has(e.pointerId)) { e.preventDefault(); this.move(e); } });
    on("pointerup", (e) => this.up(e));
    on("pointercancel", (e) => this.up(e));
    on("lostpointercapture", (e) => this.up(e));
    root.addEventListener("contextmenu", (e) => e.preventDefault());
    // belts and braces: nothing may stay pressed when the page loses the touch
    addEventListener("touchcancel", () => this.releaseAll(), { passive: true });
    addEventListener("pagehide", () => this.releaseAll());
    document.addEventListener("visibilitychange", () => { if (document.hidden) this.releaseAll(); });
  }

  // ---- TouchSource
  bits(): number { const b = this.held | this.latch; this.latch = this.held; return this.active ? b : 0; }
  gun() { return null; } // the pad aims the way a pad does: the game's own 3 x 3 cell aiming (A draws the blaster, C fires)
  releaseAll() { this.ptrs.clear(); this.latch = 0; this.recompute(); }

  // ---- geometry
  private dpadCentre() { const r = this.dpad.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, r }; }
  /** Dead zone: the hub of the cross (a third of a button), so a thumb resting in the middle presses nothing. */
  private deadZone(): number { return this.dpad.getBoundingClientRect().width / 9; }
  private inDpad(x: number, y: number, slop = 14): boolean { const r = this.dpad.getBoundingClientRect(); return x >= r.left - slop && x <= r.right + slop && y >= r.top - slop && y <= r.bottom + slop; }
  /** The button under (x, y) with a little slop; among overlapping slops the one whose centre is nearest. */
  private buttonAt(x: number, y: number, slop = 6): HTMLElement | null {
    let best: HTMLElement | null = null, bd = Infinity;
    for (const b of this.btns) {
      const r = b.getBoundingClientRect();
      if (x < r.left - slop || x > r.right + slop || y < r.top - slop || y > r.bottom + slop) continue;
      const d = Math.hypot(x - (r.left + r.width / 2), y - (r.top + r.height / 2));
      if (d < bd) { bd = d; best = b; }
    }
    return best;
  }

  private bitsAt(p: Ptr, x: number, y: number): number {
    if (p.ownsDpad) { const c = this.dpadCentre(); return dpadBits(x - c.x, y - c.y, this.deadZone()); }
    const b = this.buttonAt(x, y);
    if (b) return Number(b.dataset.btn) | 0;
    if (this.inDpad(x, y, 0)) { const c = this.dpadCentre(); return dpadBits(x - c.x, y - c.y, this.deadZone()); }
    return 0;
  }

  // ---- events
  private down(e: PointerEvent) {
    if (!this.active || (e.pointerType === "mouse" && e.button !== 0)) return;
    if ((e.target as HTMLElement).closest("button")) return; // Menu: a normal click
    e.preventDefault();
    this.onTouch?.();
    try { this.root.setPointerCapture(e.pointerId); } catch { /* ignore */ }
    const p: Ptr = { id: e.pointerId, ownsDpad: this.inDpad(e.clientX, e.clientY) && !this.buttonAt(e.clientX, e.clientY, 0), bits: 0 };
    this.ptrs.set(e.pointerId, p);
    this.move(e);
  }
  private move(e: PointerEvent) {
    const p = this.ptrs.get(e.pointerId);
    if (!p) return;
    p.bits = this.bitsAt(p, e.clientX, e.clientY);
    this.recompute();
  }
  private up(e: PointerEvent) {
    if (this.ptrs.delete(e.pointerId)) this.recompute();
  }

  private recompute() {
    let b = 0;
    for (const p of this.ptrs.values()) b |= p.bits;
    const rising = b & ~this.held;
    this.held = b;
    this.latch |= rising;
    if (rising && this.haptics) haptic();
    for (const el of this.btns) el.classList.toggle("on", (b & (Number(el.dataset.btn) | 0)) !== 0);
    for (const el of this.arms) el.classList.toggle("on", (b & (Number(el.dataset.bit) | 0)) !== 0);
    this.root.dataset.held = String(b);
  }
}

