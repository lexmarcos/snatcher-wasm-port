// The display filter in the page: the settings (saved in localStorage `snatcher.crt`), the Display tab controls, the GL canvas over the game canvas, and the
// geometry the pointer code needs. The engine and the 2D canvas are untouched: the 2D canvas keeps receiving every frame (tests read it, it is the fallback),
// the GL canvas (`#crt`) is laid exactly over it while the filter is on. Documentation: docs/re/web.md section 12, docs/design-system.md "Picture style".
import { CrtFilter } from "./crt";
import {
  CRT_MIN_SCALE, defaultPreset, fitPicture, parOf, parsePreset, parseSaved, PRESETS, presetOf, sanitizeParams, serializeSaved, setView,
  type CrtParams, type CrtPreset, type CrtSaved, type MaskKind, type Shape,
} from "./crtmath";
import { T } from "./i18n";

const KEY = "snatcher.crt";
const $ = <E extends HTMLElement>(id: string) => document.getElementById(id) as E;

export interface DisplayDeps {
  canvas: HTMLCanvasElement;      // the 2D game canvas (#screen)
  glCanvas: HTMLCanvasElement;    // the filter's canvas (#crt), a sibling laid over it
  ls: { get(k: string): string | null; set(k: string, v: string): void };
  params: URLSearchParams;
  coarse: boolean;                // the main pointer is a finger (phone / tablet)
  /** the page layout has to be redone (the picture's size or shape changed) */
  refit(): void;
  /** the current engine frame (256x224 RGBA view), or null before the game runs */
  frame(): Uint8Array | null;
}

/** What `fit()` / the touch layout decided for the picture. */
export interface Box { crt: boolean; cssW: number; cssH: number; dpr: number }

const SLIDERS: { id: string; key: "scan" | "mask" | "glow" | "sharp" | "curve" | "composite" }[] = [
  { id: "crtScan", key: "scan" }, { id: "crtMask", key: "mask" }, { id: "crtGlow", key: "glow" }, { id: "crtSharp", key: "sharp" },
  { id: "crtCurve", key: "curve" }, { id: "crtComp", key: "composite" },
];

export class Display {
  filter: CrtFilter | null = null;
  /** WebGL2 or the shaders are not available (or the context is lost): the plain picture is used */
  unavailable = false;
  saved: CrtSaved;
  private forced: CrtPreset | null;
  private tried = false;
  private boxCrt = false;
  private devW = 0; private devH = 0;
  private have = false;
  private noteSmall = false;

  constructor(private d: DisplayDeps) {
    const dflt = defaultPreset(d.coarse);
    this.saved = parseSaved(d.ls.get(KEY), { preset: dflt, params: { ...PRESETS[dflt as Exclude<CrtPreset, "custom">] } });
    const f = parsePreset(d.params.get("crt"));
    this.forced = f && f !== "custom" ? f : null;
  }

  get preset(): CrtPreset { return this.forced ?? this.saved.preset; }
  get params(): CrtParams { return this.forced ? PRESETS[this.forced] : this.saved.params; }
  /**
   * The filter is wanted and as far as we know can run: the layout should be made for it. The GL side is only built when the first frame is about to be drawn
   * (`present`), so a page that never starts a game never touches WebGL; if it turns out not to work, `unavailable` is set and the layout is redone for the plain picture
   * before anything was painted.
   */
  wants(): boolean { return this.preset !== "off" && !this.unavailable && !this.filter?.lost; }
  par(): number { return parOf(this.params.shape); }
  /** the filtered picture is on screen right now */
  get active(): boolean { return this.boxCrt && this.have && !!this.filter && !this.filter.lost; }

  /** Create the GL side on first need. */
  private ensure(): CrtFilter | null {
    if (this.filter && !this.filter.lost) return this.filter;
    if (this.tried || this.unavailable) return null;
    this.tried = true;
    const f = CrtFilter.create(this.d.glCanvas, this.forced !== null || this.d.params.has("crtsoft")); // ?crt=tv asks for it by name: software GL is allowed (tests)
    if (!f) { this.unavailable = true; this.renderTab(); return null; }
    f.onLost = () => { this.have = false; this.showGl(false); this.setBoxState(false); this.d.refit(); this.renderTab(); };
    f.onRestored = () => { this.tried = false; this.have = false; this.d.refit(); const fr = this.d.frame(); if (fr) this.present(fr); this.renderTab(); };
    this.filter = f;
    return f;
  }

  /** Show or hide the GL canvas; while it is shown the 2D canvas underneath is not painted (opacity 0: it still takes the pointer and still holds the frame), so the browser does not draw the picture twice. */
  private showGl(on: boolean) {
    const gl = this.d.glCanvas;
    if (gl.hidden === !on) return;
    gl.hidden = !on;
    gl.parentElement?.classList.toggle("crt-on", on);
  }

  private setBoxState(crt: boolean) {
    this.boxCrt = crt;
    if (crt) setView(this.par(), this.params.curve); else setView(1, 0);
  }

  /** The layout's answer: the picture box (CSS px) and whether it was made for the filter. Sizes the GL canvas and redraws. */
  setBox(b: Box) {
    const ok = b.crt && this.wants();
    this.noteSmall = this.preset !== "off" && !ok && !this.unavailable;
    this.setBoxState(ok);
    if (ok) {
      this.devW = Math.max(1, Math.round(b.cssW * b.dpr)); this.devH = Math.max(1, Math.round(b.cssH * b.dpr));
      if (this.filter) {
        this.filter.setSize(this.devW, this.devH);
        this.filter.setParams(this.params);
        if (this.have) { this.filter.draw(this.devH / 224); this.showGl(true); }
      } else {
        // a style was switched on while the game is running (paused in the menu): build the GL side now and draw the frame on screen
        const fr = this.d.frame();
        if (fr) this.present(fr);
      }
    } else { this.showGl(false); }
    this.renderTab();
  }

  /** A new engine frame (called after the 2D canvas got it). */
  present(frame: Uint8Array) {
    if (!this.boxCrt) return;
    let f = this.filter;
    if (!f) {
      f = this.ensure();
      if (!f) { this.d.refit(); return; }      // no WebGL2 after all: the plain layout, before the first picture is painted
      f.setSize(this.devW, this.devH); f.setParams(this.params);
    }
    if (f.lost) return;
    f.upload(frame);
    this.have = true;
    f.draw(this.devH / 224);
    this.showGl(true);
  }
  /** Redraw the frame the GPU already has (settings changed while paused). */
  redraw() { if (this.active && this.filter) this.filter.draw(this.devH / 224); }

  // ---------------------------------------------------------------- settings

  private persist() { if (!this.forced) this.d.ls.set(KEY, serializeSaved(this.saved)); }
  private leaveForced() { if (this.forced) { const f = this.forced; this.forced = null; this.saved = { preset: f, params: { ...PRESETS[f as Exclude<CrtPreset, "custom">] } }; } }

  setPreset(p: Exclude<CrtPreset, "custom">) {
    this.leaveForced();
    this.saved = { preset: p, params: { ...PRESETS[p] } };
    this.changed(true);
  }
  setParams(patch: Partial<CrtParams>) {
    this.leaveForced();
    const params = sanitizeParams({ ...this.saved.params, ...patch }, this.saved.params);
    this.saved = { preset: presetOf(params), params };
    this.changed(patch.shape !== undefined || patch.curve !== undefined);
  }
  reset() { this.setPreset(defaultPreset(this.d.coarse) as Exclude<CrtPreset, "custom">); }

  private changed(relayout: boolean) {
    this.persist();
    if (relayout) this.d.refit();
    else if (this.filter) { this.filter.setParams(this.params); this.redraw(); }
    this.renderTab();
  }

  // ---------------------------------------------------------------- the Display tab

  bindUi() {
    const sel = $<HTMLSelectElement>("crtPreset"), shape = $<HTMLSelectElement>("crtShape"), kind = $<HTMLSelectElement>("crtMaskKind");
    sel.onchange = () => { const p = parsePreset(sel.value); if (p && p !== "custom") this.setPreset(p); };
    shape.onchange = () => this.setParams({ shape: shape.value === "square" ? "square" : "tv" as Shape });
    kind.onchange = () => this.setParams({ maskKind: kind.value as MaskKind });
    for (const s of SLIDERS) $<HTMLInputElement>(s.id).oninput = (e) => this.setParams({ [s.key]: Number((e.target as HTMLInputElement).value) / 100 });
    $("crtReset").onclick = () => this.reset();
    this.renderTab();
  }

  /** Values and texts of the controls; called after every change, a language change and a layout. */
  renderTab() {
    if (!document.getElementById("crtPreset")) return;
    const p = this.params, pre = this.preset;
    const sel = $<HTMLSelectElement>("crtPreset");
    sel.value = pre;
    ($<HTMLOptionElement>("crtOptCustom")).hidden = pre !== "custom";
    $("crtHelp").textContent = T(`crt.help.${pre}` as any);
    $<HTMLSelectElement>("crtShape").value = p.shape;
    $<HTMLSelectElement>("crtMaskKind").value = p.maskKind;
    for (const s of SLIDERS) {
      const el = $<HTMLInputElement>(s.id), v = Math.round(p[s.key] * 100);
      el.value = String(v);
      el.style.setProperty("--v", `${v}%`);
      $(s.id + "O").textContent = `${v}%`;
    }
    const off = pre === "off";
    $("crtAdjust").hidden = off; $("crtShapeRow").hidden = off;
    $("smoothRow").hidden = !off && this.boxCrt;
    const note = $("crtNote");
    const text = this.unavailable && !off ? T("crt.note.nogl") : this.noteSmall ? T("crt.note.small") : "";
    note.textContent = text; note.hidden = !text;
  }
}

export { fitPicture, CRT_MIN_SCALE };
