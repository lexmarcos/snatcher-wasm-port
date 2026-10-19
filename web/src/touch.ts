// Touch input for phones and tablets: the glue between the page (layout, Touch tab, wake lock, hints) and the two controllers.
//   mode "pad"   the on-screen pad (touchpad.ts)
//   mode "game"  touch the picture itself (touchgame.ts)
//   mode "off"   nothing: the browser's own mouse emulation of a touch still works as before
// Default mode: the pad on a device whose main pointer is a finger, "off" on a computer. The choice, the size and the opacity of the pad, haptics and the
// wake lock are kept in localStorage (`snatcher.touch.*`). Documentation: docs/re/web.md section 11, docs/design-system.md "Touch controls".
import type { Input } from "./input";
import { TouchPad } from "./touchpad";
import { TouchGame, type GameApi } from "./touchgame";
import {
  clampNum, defaultTouchMode, PAD_OPACITY_RANGE, PAD_SIZE_RANGE, parseTouchMode, planLayout, type Insets, type LayoutOut, type TouchMode,
} from "./touchmath";
import { T, type UiKey } from "./i18n";
import { canVibrate, canWakeLock, chromeAdvice, haptic, isCoarsePrimary, toggleFullscreen, WakeLock } from "./platform";

export interface TouchDeps {
  input: Input;
  canvas: HTMLCanvasElement;
  ls: { get(k: string): string | null; set(k: string, v: string): void };
  openPanel(): void;
  /** the display filter wants the picture laid out for it (pixel aspect, whole device pixels), or undefined when it is off */
  crt?(): { par: number } | undefined;
  /** the picture box was (re)laid out: the display filter sizes its canvas from it (also when the Touch tab changes the pad size) */
  onLayout?(L: LayoutOut): void;
}

const $ = <E extends HTMLElement>(id: string) => document.getElementById(id) as E;
const K = { mode: "snatcher.touch.mode", size: "snatcher.touch.size", opacity: "snatcher.touch.opacity", haptics: "snatcher.touch.haptics", awake: "snatcher.touch.awake", rotate: "snatcher.touch.rotate" };

export class Touch {
  mode: TouchMode;
  size: number;
  opacity: number;
  haptics: boolean;
  awake: boolean;
  readonly pad: TouchPad;
  readonly game: TouchGame;
  readonly wake = new WakeLock();
  layout: LayoutOut | null = null;
  /** the game has started (the controls are shown from then on) and is running (not paused) */
  started = false;
  private running = false;
  private rotateTimer = 0;
  private root = $("touch");
  private html = document.documentElement;

  constructor(private d: TouchDeps) {
    const coarse = isCoarsePrimary();
    this.mode = parseTouchMode(d.ls.get(K.mode)) ?? defaultTouchMode(coarse, navigator.maxTouchPoints ?? 0);
    this.size = clampNum(d.ls.get(K.size), PAD_SIZE_RANGE[0], PAD_SIZE_RANGE[1], 100);
    this.opacity = clampNum(d.ls.get(K.opacity), PAD_OPACITY_RANGE[0], PAD_OPACITY_RANGE[1], 85);
    this.haptics = d.ls.get(K.haptics) === "1" && canVibrate();
    const aw = d.ls.get(K.awake);
    this.awake = aw === "1" ? true : aw === "0" ? false : coarse; // a computer keeps its screen settings unless the player asks
    this.pad = new TouchPad(this.root);
    this.game = new TouchGame($("wrap"), d.canvas, { back: $("gBack"), ok: $("gOk"), holster: $("gHolster") });
    for (const c of [this.pad, this.game]) c.onTouch = () => d.input.touchDevice();
    $("tMenu").addEventListener("click", () => d.openPanel());
    // pad mode: a finger on the picture is not a mouse (the browser's made-up mouse events would switch the light gun on)
    $("wrap").addEventListener("pointerdown", (e) => { if (this.mode === "pad" && e.pointerType !== "mouse") { d.input.lastTouchAt = performance.now(); e.preventDefault(); } });
    // iOS Safari: a pinch on the game surface would zoom the page (touch-action is not honoured for pinch there)
    document.addEventListener("gesturestart", (e) => { if (this.mode !== "off" && this.started && !(e.target as HTMLElement).closest?.(".modal")) e.preventDefault(); } );
    this.wake.onChange = () => this.renderState();
    this.bindTab();
    $("rotateOk").addEventListener("click", () => this.dismissRotate());
    this.apply();
  }

  /** The engine instance of the running game (null = none). */
  attach(game: GameApi | null) { this.game.game = game; this.started = !!game; this.apply(); }
  /** Called when the game starts or stops running (start overlay, menu panel, hidden tab): the wake lock and the controls follow. */
  setRunning(on: boolean) {
    this.running = on;
    this.wake.want(on && this.awake && this.started);
    if (!on) { this.pad.releaseAll(); this.game.releaseAll(); }
  }
  /** per animation frame */
  refresh() { if (this.mode === "game" && this.started) this.game.refresh(); }

  // ---- mode and prefs
  setMode(m: TouchMode) {
    this.mode = m; this.d.ls.set(K.mode, m);
    this.pad.releaseAll(); this.game.releaseAll();
    this.apply();
    this.fit();
  }

  private apply() {
    const on = this.mode !== "off";
    this.d.input.touch = on ? (this.mode === "pad" ? this.pad : this.game) : null;
    this.pad.active = this.mode === "pad"; this.game.active = this.mode === "game";
    this.pad.haptics = this.haptics; this.game.haptics = this.haptics;
    if (on) this.html.dataset.touch = this.mode; else { delete this.html.dataset.touch; delete this.html.dataset.arr; this.html.removeAttribute("data-full"); }
    this.root.dataset.mode = this.mode === "game" ? "game" : "pad";
    this.root.hidden = !on || !this.started;
    this.html.style.setProperty("--pad-op", String(this.opacity / 100));
    this.wake.want(this.running && this.awake && this.started);
    this.renderTab();
    this.updateRotate();
  }

  // ---- layout
  private safe(): Insets {
    const p = $("sai"), cs = getComputedStyle(p);
    const px = (v: string) => parseFloat(v) || 0;
    return { t: px(cs.paddingTop), r: px(cs.paddingRight), b: px(cs.paddingBottom), l: px(cs.paddingLeft) };
  }

  /**
   * Size the picture and place the controls for the viewport. Returns false when touch controls are off, so the caller keeps its own (desktop) layout.
   */
  fit(): boolean {
    if (this.mode === "off") { this.layout = null; this.updateRotate(); return false; }
    const safe = this.safe();
    const L = planLayout({ vw: innerWidth, vh: innerHeight, dpr: window.devicePixelRatio || 1, safe, mode: this.mode === "pad" ? "pad" : "game", size: this.size, crt: this.d.crt?.() });
    this.layout = L;
    this.d.canvas.style.width = `${L.canvasW}px`; this.d.canvas.style.height = `${L.canvasH}px`;
    this.d.onLayout?.(L);
    const st = this.html.style;
    st.setProperty("--u", `${L.u}px`);
    st.setProperty("--reserve-b", `${L.reserveBottom}px`);
    this.html.dataset.arr = L.arr; this.html.toggleAttribute("data-full", L.full);
    this.updateRotate();
    return true;
  }

  // ---- the rotate hint: phones in portrait whose picture would be larger sideways (never a block, dismissed for good with OK)
  private updateRotate() {
    const hint = $("rotateHint");
    const L = this.layout;
    let show = false;
    if (L && this.started && !L.landscape && Math.min(innerWidth, innerHeight) < 600 && this.d.ls.get(K.rotate) !== "1") {
      const alt = planLayout({ vw: innerHeight, vh: innerWidth, dpr: window.devicePixelRatio || 1, safe: { t: 0, r: 0, b: 0, l: 0 }, mode: this.mode === "pad" ? "pad" : "game", size: this.size, crt: this.d.crt?.() });
      show = alt.scale > L.scale;
    }
    if (show && hint.hidden) { clearTimeout(this.rotateTimer); this.rotateTimer = window.setTimeout(() => { hint.hidden = true; }, 12000); }
    hint.hidden = !show;
  }
  private dismissRotate() { this.d.ls.set(K.rotate, "1"); $("rotateHint").hidden = true; clearTimeout(this.rotateTimer); }

  // ---- the Touch tab
  private bindTab() {
    const sel = $<HTMLSelectElement>("touchMode"), size = $<HTMLInputElement>("touchSize"), op = $<HTMLInputElement>("touchOpacity");
    const hap = $<HTMLInputElement>("touchHaptics"), awake = $<HTMLInputElement>("touchAwake");
    sel.value = this.mode;
    sel.onchange = () => this.setMode(parseTouchMode(sel.value) ?? "off");
    const paint = (el: HTMLInputElement, out: string) => { $(out).textContent = `${el.value}%`; el.style.setProperty("--v", `${((Number(el.value) - Number(el.min)) / (Number(el.max) - Number(el.min))) * 100}%`); };
    size.value = String(this.size); paint(size, "touchSizeO");
    size.oninput = () => { this.size = Number(size.value); this.d.ls.set(K.size, size.value); paint(size, "touchSizeO"); this.fit(); };
    op.value = String(this.opacity); paint(op, "touchOpacityO");
    op.oninput = () => { this.opacity = Number(op.value); this.d.ls.set(K.opacity, op.value); paint(op, "touchOpacityO"); this.html.style.setProperty("--pad-op", String(this.opacity / 100)); };
    hap.checked = this.haptics;
    hap.onchange = () => { this.haptics = hap.checked; this.d.ls.set(K.haptics, hap.checked ? "1" : "0"); this.pad.haptics = this.game.haptics = this.haptics; if (this.haptics) haptic(30); };
    $("touchHapticsRow").hidden = !canVibrate();
    awake.checked = this.awake;
    awake.onchange = () => { this.awake = awake.checked; this.d.ls.set(K.awake, awake.checked ? "1" : "0"); this.wake.want(this.running && this.awake && this.started); };
    $("touchAwakeRow").hidden = !canWakeLock();
    $("touchFull").onclick = () => toggleFullscreen();
  }

  /** Texts that depend on the language and on the state; called after a language change and whenever the mode changes. */
  renderTab() {
    $<HTMLSelectElement>("touchMode").value = this.mode;
    const help: Record<TouchMode, UiKey> = { pad: "t.help.pad", game: "t.help.game", off: "t.help.off" };
    $("touchHelp").textContent = T(help[this.mode]);
    $("touchSizeRow").hidden = this.mode !== "pad";
    $("touchOpacityRow").hidden = this.mode !== "pad";
    const adv = chromeAdvice();
    $("touchFull").hidden = adv !== "fullscreen";
    $("touchAdvice").textContent = adv === "homescreen" ? T("t.advice.home") : "";
    $("gOk").dataset.start = T("t.start"); $("gOk").dataset.ok = T("t.ok");
    this.renderState();
  }
  private renderState() {
    const parts: string[] = [];
    if (this.awake && canWakeLock() && this.started && this.wake.failed) parts.push(T("t.awake.failed"));
    $("touchState").textContent = parts.join(" ");
  }
}
