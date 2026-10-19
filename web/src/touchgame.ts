// Mode 2, "touch the game": no pad is drawn; the picture itself is the controller. The engine says what the game waits for and where the tappable
// things are (`Game.touch_ui()`, engine/src/touch.rs); this file turns taps and drags into the pad presses (and the light gun) the game already
// understands, through the same `Input` path as the keyboard. State by state (docs/re/web.md section 11):
//   message / none  press = the confirm button held while the finger is down (a held confirm also prints text at once); a tap needs no aim
//   menu            tap a line = the cursor walks there (Up / Down presses, one at a time, waiting for the game) and confirm; arrow lines flip the page;
//                   the dark room's free view: touch and drag = B held plus the direction of the drag (a stick from the touch point), lift = release
//   entry           tap a key = the cursor walks there along the keyboard's own grid (engine `touch_step`) and the key is typed
//   shoot           tap = aim there and fire (the mouse light-gun path); the first tap also draws the blaster; a Holster button puts it away
//   title / slots / options   tap an entry (NEW GAME, CONTINUE, a slot, a settings row; a value row cycles its value on a second tap)
//   skip            a Start button appears (intro, "press Start" steps); no tap does anything, so a stray touch never skips a scene
//   Back            a Back button (shown only when it does something) or a two-finger tap = the cancel button / backspace
import type { TouchSource } from "./input";
import { PAD } from "./gamepads";
import { clientToGame, dpadBits, hitRect, type Rect } from "./touchmath";
import { haptic } from "./platform";

export interface TRect extends Rect { k: number }
export interface Ui {
  kind: string; ready: boolean; back: boolean; start: boolean; label: string; cur: number; page: number;
  confirm: number; cancel: number; extra: number; pointer: boolean; drawn: boolean; playing: boolean; rects: TRect[];
}
export interface GameApi { touch_ui(): string; touch_step(target: number): number }

const NONE: Ui = { kind: "none", ready: false, back: false, start: false, label: "start", cur: 0, page: 0, confirm: PAD.C, cancel: PAD.B, extra: PAD.A, pointer: false, drawn: false, playing: false, rects: [] };

/** One timed step of the press queue: `bits` held for `ticks` game ticks (a press is 2 ticks down, 2 ticks up: the game loses a press made while it redraws). */
interface Step { bits: number; ticks: number }
const PRESS_TICKS = 2, GAP_TICKS = 2;

type Plan =
  | { kind: string; type: "line"; id: number; steps: number; wait: number }
  | { kind: string; type: "arrow"; dir: number; page: number; steps: number; wait: number }
  | { kind: string; type: "cell"; id: number; steps: number; wait: number }
  | { kind: string; type: "pick"; id: number; steps: number; wait: number };

interface Finger { id: number; x0: number; y0: number; x: number; y: number; t0: number; moved: number; drag: boolean }

export class TouchGame implements TouchSource {
  game: GameApi | null = null;
  ui: Ui = NONE;
  haptics = false;
  active = true;
  onTouch: (() => void) | null = null;
  /** raw game-pixel position of the last aim (for tests and the crosshair of the title calibration) */
  aim = { x: 128, y: 112 };
  private fingers = new Map<number, Finger>();
  private maxFingers = 0;
  private gestureStart = 0;
  private gestureMoved = 0;
  /** buttons held while a finger is down (confirm for messages) */
  private holdBits = 0;
  private dragBits = 0;
  private queue: Step[] = [];
  private cur: Step | null = null;
  private plan: Plan | null = null;
  private gunSteps: { aux: boolean; trig: boolean; ticks: number }[] = [];
  private gunCur: { aux: boolean; trig: boolean; ticks: number } | null = null;
  private gunIdle = 0;

  constructor(private stage: HTMLElement, private canvas: HTMLCanvasElement, private ctl: { back: HTMLElement; ok: HTMLElement; holster: HTMLElement }) {
    const on = (type: string, fn: (e: PointerEvent) => void) => stage.addEventListener(type, fn as EventListener);
    on("pointerdown", (e) => this.down(e));
    on("pointermove", (e) => this.move(e));
    on("pointerup", (e) => this.up(e));
    on("pointercancel", (e) => this.cancel(e));
    on("lostpointercapture", (e) => this.cancel(e));
    stage.addEventListener("contextmenu", (e) => e.preventDefault());
    addEventListener("touchcancel", () => this.releaseAll(), { passive: true });
    addEventListener("pagehide", () => this.releaseAll());
    document.addEventListener("visibilitychange", () => { if (document.hidden) this.releaseAll(); });
    ctl.back.addEventListener("click", () => this.pressBack());
    ctl.ok.addEventListener("click", () => this.press(PAD.START));
    ctl.holster.addEventListener("click", () => this.gunPress(false, true));
  }

  // ---- engine state
  readUi(): Ui {
    if (!this.game) return (this.ui = NONE);
    try {
      const u = JSON.parse(this.game.touch_ui()) as Ui;
      if (!u.confirm) u.confirm = PAD.C;
      if (!u.cancel) u.cancel = PAD.B;
      return (this.ui = u);
    } catch { return (this.ui = NONE); }
  }

  /** Per animation frame: show the Back / Start / Holster buttons that do something now. */
  refresh() {
    const u = this.readUi();
    const set = (el: HTMLElement, show: boolean) => { el.style.visibility = show ? "visible" : "hidden"; (el as HTMLButtonElement).disabled = !show; };
    set(this.ctl.back, u.back);
    set(this.ctl.ok, u.start);
    this.ctl.ok.textContent = this.ctl.ok.dataset[u.label === "ok" ? "ok" : "start"] ?? this.ctl.ok.textContent;
    set(this.ctl.holster, u.kind === "shoot" && u.playing && u.drawn);
  }

  // ---- TouchSource
  bits(): number {
    if (!this.active) return 0;
    const u = this.readUi();
    let b = this.holdBits | this.dragBits;
    if (!this.cur && this.queue.length) this.cur = { ...this.queue.shift()! };
    if (this.cur) { b |= this.cur.bits; if (--this.cur.ticks <= 0) this.cur = null; }
    else this.stepPlan(u);
    return b;
  }

  gun(): { x: number; y: number; trigger: boolean; aux: boolean } | null {
    if (!this.active) return null;
    const k = this.ui.kind;
    if (k !== "shoot" && k !== "calibrate") { this.gunSteps = []; this.gunCur = null; this.gunIdle = 0; return null; }
    if (!this.gunCur && this.gunSteps.length) this.gunCur = { ...this.gunSteps.shift()! };
    const s = this.gunCur;
    if (s && --s.ticks <= 0) this.gunCur = null;
    return { x: this.aim.x, y: this.aim.y, trigger: !!s?.trig, aux: !!s?.aux };
  }

  releaseAll() {
    this.fingers.clear(); this.maxFingers = 0;
    this.holdBits = 0; this.dragBits = 0;
    this.queue = []; this.cur = null; this.plan = null;
    this.gunSteps = []; this.gunCur = null;
  }

  // ---- presses
  private press(bits: number) {
    if (!bits) return;
    this.queue.push({ bits, ticks: PRESS_TICKS }, { bits: 0, ticks: GAP_TICKS });
    if (this.haptics) haptic();
    this.onTouch?.();
  }
  private pressBack() { const u = this.readUi(); if (u.back) this.press(u.cancel); }
  /** Gun button sequence: optional draw (aux), then optional trigger, each 2 ticks with a 2 tick gap. */
  private gunPress(trig: boolean, aux: boolean) {
    if (this.gunSteps.length > 6) return;
    if (aux) this.gunSteps.push({ aux: true, trig: false, ticks: PRESS_TICKS }, { aux: false, trig: false, ticks: GAP_TICKS });
    if (trig) this.gunSteps.push({ aux: false, trig: true, ticks: PRESS_TICKS }, { aux: false, trig: false, ticks: GAP_TICKS });
    if (this.haptics) haptic();
  }

  // ---- the walk of a tap (one press at a time, the game's answer awaited)
  private stepPlan(u: Ui) {
    const p = this.plan;
    if (!p) return;
    if (u.kind !== p.kind) { this.plan = null; return; }
    if (!u.ready) { if (++p.wait > 300) this.plan = null; return; }
    p.wait = 0;
    if (++p.steps > 40) { this.plan = null; return; }
    switch (p.type) {
      case "line": {
        if (u.cur === p.id) { this.plan = null; this.press(u.confirm); return; }
        this.press(p.id < u.cur ? PAD.UP : PAD.DOWN);
        return;
      }
      case "arrow": {
        if (u.page !== p.page) { this.plan = null; return; }
        this.press(p.dir);
        return;
      }
      case "cell": {
        if (u.cur === p.id) { this.plan = null; this.press(u.confirm); return; }
        const d = this.game?.touch_step(p.id) ?? 0;
        if (!d) { this.plan = null; return; }
        this.press(d);
        return;
      }
      case "pick": { // title menu / slot screen / settings rows: step the cursor to the entry, then act on it
        if (u.cur !== p.id) { this.press(p.id > u.cur ? PAD.DOWN : PAD.UP); return; }
        this.plan = null;
        // a settings row with a value (device, controls, sound mode) cycles it; GUN ADJUST opens the calibration; the QUIT button starts the game
        if (u.kind === "options") this.press(p.id <= 2 ? PAD.RIGHT : p.id === 3 ? u.confirm : PAD.START);
        else this.press(PAD.START);
        return;
      }
    }
  }

  // ---- fingers
  private point(e: PointerEvent) {
    const r = this.canvas.getBoundingClientRect();
    return clientToGame(e.clientX, e.clientY, { x: r.left, y: r.top, w: r.width, h: r.height });
  }
  private onControl(e: PointerEvent): boolean { return !!(e.target as HTMLElement).closest("button, #bar, .tside"); }

  private down(e: PointerEvent) {
    if (!this.active || this.onControl(e) || (e.pointerType === "mouse" && e.button !== 0)) return;
    e.preventDefault();
    try { this.stage.setPointerCapture(e.pointerId); } catch { /* ignore */ }
    this.onTouch?.();
    const now = performance.now();
    if (!this.fingers.size) { this.maxFingers = 0; this.gestureStart = now; this.gestureMoved = 0; }
    const f: Finger = { id: e.pointerId, x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, t0: now, moved: 0, drag: false };
    this.fingers.set(e.pointerId, f);
    this.maxFingers = Math.max(this.maxFingers, this.fingers.size);
    const u = this.readUi();
    const p = this.point(e);
    switch (u.kind) {
      case "message": case "none":
        if (this.fingers.size === 1) { this.holdBits = u.confirm; if (this.haptics) haptic(); }
        break;
      case "shoot": case "calibrate": {
        // aim and fire at once: this is the light gun, a mouse click would do the same. Outside the picture (a little slack) nothing happens.
        const r = this.canvas.getBoundingClientRect();
        const near = e.clientX > r.left - 12 && e.clientX < r.right + 12 && e.clientY > r.top - 12 && e.clientY < r.bottom + 12;
        if (!near) break;
        this.aim = { x: p.x, y: p.y };
        this.gunPress(true, u.kind === "shoot" && !u.drawn && u.playing); // the first tap draws the blaster, then fires
      }
    }
  }

  private move(e: PointerEvent) {
    const f = this.fingers.get(e.pointerId);
    if (!f) return;
    e.preventDefault();
    f.x = e.clientX; f.y = e.clientY;
    f.moved = Math.max(f.moved, Math.hypot(f.x - f.x0, f.y - f.y0));
    this.gestureMoved = Math.max(this.gestureMoved, f.moved);
    const u = this.ui;
    if ((u.kind === "shoot" || u.kind === "calibrate") && this.fingers.size === 1) { const p = this.point(e); this.aim = { x: p.x, y: p.y }; }
    if (u.kind === "menu" && u.pointer && this.fingers.size === 1 && f.moved > 14) {
      f.drag = true; this.plan = null;
      // the dark room's free view: B held + the direction of the drag from the touch point
      this.dragBits = u.cancel | dpadBits(f.x - f.x0, f.y - f.y0, 14);
    }
  }

  private up(e: PointerEvent) {
    const f = this.fingers.get(e.pointerId);
    if (!f) return;
    this.fingers.delete(e.pointerId);
    if (f.drag) this.dragBits = 0;
    if (this.fingers.size) return;
    // the gesture is over
    this.holdBits = 0;
    const now = performance.now();
    const two = this.maxFingers >= 2;
    if (two) {
      // two fingers down together, quickly and without moving: Back
      if (now - this.gestureStart < 450 && this.gestureMoved < 24) this.pressBack();
      this.maxFingers = 0;
      return;
    }
    if (f.drag || f.moved > 14) return;
    const u = this.readUi();
    const p = this.point(e);
    this.tap(u, p.x, p.y, p.inside);
  }

  private cancel(e: PointerEvent) {
    const f = this.fingers.get(e.pointerId);
    if (!f) return;
    this.fingers.delete(e.pointerId);
    if (f.drag) this.dragBits = 0;
    if (!this.fingers.size) { this.holdBits = 0; this.maxFingers = 0; }
  }

  /** A tap that has ended (menus, keys and the title screens act on release, so a drag or a second finger can still turn it into something else). */
  private tap(u: Ui, gx: number, gy: number, _inside: boolean) {
    const mk = (p: Plan) => { this.plan = p; if (this.haptics) haptic(); };
    switch (u.kind) {
      case "menu": {
        const r = hitRect(u.rects, gx, gy, 12, 5);
        if (!r) return;
        if (r.k === 1 || r.k === 2) mk({ kind: "menu", type: "arrow", dir: r.k === 1 ? PAD.UP : PAD.DOWN, page: u.page, steps: 0, wait: 0 });
        else mk({ kind: "menu", type: "line", id: r.id, steps: 0, wait: 0 });
        return;
      }
      case "entry": {
        const r = hitRect(u.rects, gx, gy, 5, 5);
        if (r) mk({ kind: "entry", type: "cell", id: r.id, steps: 0, wait: 0 });
        return;
      }
      case "title": case "slots": case "options": {
        const r = hitRect(u.rects, gx, gy, 4, 3);
        if (r) mk({ kind: u.kind, type: "pick", id: r.id, steps: 0, wait: 0 });
        return;
      }
    }
  }
}
