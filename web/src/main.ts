// Snatcher web port – browser front end.
// The engine (Rust → wasm) owns all game logic and renders a 256x224 RGBA frame (H32 mode).
// This file handles: asset sources, the start overlay and menu, the fixed 60 Hz loop, canvas scaling,
// input, audio and saves. See docs/re/web.md.
import { AudioOut } from "./audio";
import { Input } from "./input";
import {
  CONTROL_NAMES, Control, Family, FAMILIES, FAMILY_LABEL, GENESIS_ORDER, GenesisButton, promptFor, resolveIcon,
  SELECT_NAMES, parseGamepadId, setOf, STYLE,
} from "./gamepads";
import { controlIcon, dpadIcon, mouseIcon, stickIcon } from "./icons";
import { Pacer } from "./pacer";
import { SaveStore, SLOTS } from "./saves";
import { pickedSource, probeServed, type Source } from "./sources";
import { T, applyStatic, getLang, parseLang, setUiDict, type Lang, type UiKey } from "./i18n";

const W = 256, H = 224;
const params = new URLSearchParams(location.search);
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const msg = $("msg"), canvas = $<HTMLCanvasElement>("screen"), ctx = canvas.getContext("2d")!, debugEl = $("debug");
const bar = $("bar"), overlay = $("overlay"), panel = $("panel");
const smoothBox = $<HTMLInputElement>("smooth"), dbgBox = $<HTMLInputElement>("dbg");

// ---------- persistent per-viewer settings (localStorage may be blocked: everything works without it) ----------
const ls = {
  get(k: string): string | null { try { return localStorage.getItem(k); } catch { return null; } },
  set(k: string, v: string) { try { localStorage.setItem(k, v); } catch { /* ignore */ } },
};

// ---------- notes (visible degradation messages) ----------
type Level = "warn" | "err" | "ok";
const notes = new Map<string, { level: Level; text: string }>();
function note(key: string, level: Level, text: string | null) {
  if (text === null) notes.delete(key); else notes.set(key, { level, text });
  renderNotes();
}
function renderNotes() {
  for (const id of ["notes", "pnNotes"]) {
    const ul = $(id); ul.replaceChildren();
    for (const n of notes.values()) { const li = document.createElement("li"); li.className = n.level; li.textContent = n.text; ul.append(li); }
    if (!notes.size && id === "pnNotes") { const li = document.createElement("li"); li.className = "ok"; li.textContent = T("note.ok"); ul.append(li); }
  }
}

// ---------- canvas scaling (integer scale in device pixels; fractional only when the window is smaller than 1x) ----------
function fit() {
  const dpr = window.devicePixelRatio || 1;
  const availW = innerWidth - 32;
  const availH = innerHeight - (document.fullscreenElement ? 0 : bar.offsetHeight + 24);
  const raw = Math.min((availW * dpr) / W, (availH * dpr) / H);
  const s = raw >= 1 ? Math.floor(raw) : Math.max(0.25, raw);
  canvas.style.width = `${(W * s) / dpr}px`;
  canvas.style.height = `${(H * s) / dpr}px`;
}
addEventListener("resize", fit);
document.addEventListener("fullscreenchange", fit);

smoothBox.checked = ls.get("snatcher.smooth") === "1";
const applySmooth = () => { canvas.classList.toggle("smooth", smoothBox.checked); ls.set("snatcher.smooth", smoothBox.checked ? "1" : "0"); };
smoothBox.onchange = applySmooth; applySmooth();
dbgBox.checked = params.has("debug");
const applyDbg = () => { debugEl.style.display = dbgBox.checked ? "block" : "none"; };
dbgBox.onchange = applyDbg; applyDbg();

// ---------- modal handling (overlay + menu): focus trap, Esc, pause ----------
let overlayOpen = true, panelOpen = false;
const FOCUSABLE = "button:not(:disabled), input:not([hidden]):not(:disabled), select, summary, [tabindex]:not([tabindex='-1'])";
function trap(modal: HTMLElement, e: KeyboardEvent) {
  if (e.key !== "Tab") return;
  const items = Array.from(modal.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((x) => x.offsetParent !== null);
  if (!items.length) return;
  const first = items[0], last = items[items.length - 1];
  if (e.shiftKey && document.activeElement === first) { last.focus(); e.preventDefault(); }
  else if (!e.shiftKey && document.activeElement === last) { first.focus(); e.preventDefault(); }
}
for (const m of [overlay, panel]) m.addEventListener("keydown", (e) => trap(m, e));

const isPaused = () => overlayOpen || panelOpen || document.hidden;

// ---------- input device, button maps, prompts ----------
const input = new Input(canvas);
input.enabled = false; // the start overlay is open
try { input.setMaps(JSON.parse(ls.get("snatcher.padmaps") ?? "null")); } catch { input.setMaps(null); }
const saveMaps = () => ls.set("snatcher.padmaps", JSON.stringify(input.maps));
type PromptMode = "auto" | "letters";
let promptMode: PromptMode = ls.get("snatcher.prompts") === "letters" ? "letters" : "auto";
let lastPrompt = { style: 0, ids: [0, 0, 0, 0] as number[] };

/** Tell the engine which icons to print (the last used device + its Genesis-button map), then refresh the help text. */
function applyPrompt() {
  const p = promptMode === "letters" ? { style: STYLE.genesis, ids: promptFor(input.device, input.maps).ids as number[] } : promptFor(input.device, input.maps);
  lastPrompt = { style: p.style, ids: [...p.ids] };
  game?.set_prompt(p.style, p.ids[0], p.ids[1], p.ids[2], p.ids[3]);
  renderControls(); renderPadMap();
}

// ---------- controls help (shows the controls of the active device) ----------
const GEN_ROWS: [GenesisButton, UiKey][] = [["C", "ctl.confirm"], ["B", "ctl.cancel"], ["A", "ctl.draw"], ["START", "ctl.start"]];
function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e;
}
function iconCell(html: string, label: string, td: HTMLElement) {
  const s = el("span", "icon"); s.innerHTML = html; td.append(s, el("span", "iconlabel", label));
}
function renderControls() {
  const dev = input.device;
  const rows: [string, (td: HTMLElement) => void][] = [];
  const set = dev.kind === "keyboard" ? "keyboard" : setOf(dev.family);
  rows.push([dev.kind === "keyboard" ? T("ctl.keyboard") : T("ctl.gamepad"), (td) => {
    td.textContent = dev.kind === "keyboard" ? T("ctl.lastused") : T("ctl.lastdev", { name: FAMILY_LABEL[dev.family], extra: dev.family === "generic" ? T("ctl.generic") : "" });
  }]);
  rows.push([T("ctl.move"), (td) => {
    if (dev.kind === "keyboard") td.textContent = T("ctl.arrows");
    else { iconCell(dpadIcon(), T("ctl.dpad"), td); td.append(" / "); iconCell(stickIcon(), T("ctl.stick"), td); }
  }]);
  for (const [g, whatKey] of GEN_ROWS) {
    const what = T(whatKey);
    rows.push([g === "START" ? what : `${what} (${g})`, (td) => {
      if (dev.kind === "keyboard") {
        const k = ({ A: "Z", B: "X", C: "C", START: "Enter" } as const)[g];
        iconCell(controlIcon("keyboard", k), k, td);
        td.append(el("span", "dim", g === "START" ? "" : T("ctl.also", { k: ({ A: "A", B: T("ctl.sorspace"), C: "D" } as const)[g] })));
      } else {
        const c = input.maps[dev.family][g];
        iconCell(controlIcon(setOf(dev.family), c), CONTROL_NAMES[setOf(dev.family)][c], td);
      }
    }]);
  }
  rows.push([T("ctl.web"), (td) => {
    if (dev.kind === "pad") { iconCell(controlIcon(setOf(dev.family), "select"), SELECT_NAMES[setOf(dev.family)], td); td.append(T("ctl.orEsc")); } else td.textContent = T("ctl.esc");
  }]);
  rows.push([T("ctl.gun"), (td) => { iconCell(mouseIcon(), T("ctl.mouse"), td); td.append(T("ctl.gunhelp")); }]);
  rows.push([T("ctl.misc"), (td) => { td.textContent = "F / M / F1"; }]);
  void set;
  for (const host of Array.from(document.querySelectorAll(".controls"))) {
    const t = el("table", "keys");
    for (const [k, fill] of rows) { const r = t.insertRow(); r.insertCell().textContent = k; fill(r.insertCell()); }
    host.replaceChildren(t);
  }
}

// ---------- gamepad panel: connected pads, per-family remap, prompt mode ----------
const padFamily = $<HTMLSelectElement>("padFamily"), padMapTable = $<HTMLTableElement>("padMapTable"), padInfo = $("padInfo"), padHint = $("padHint");
const promptSel = $<HTMLSelectElement>("promptMode");
for (const f of FAMILIES) { const o = el("option", undefined, FAMILY_LABEL[f]); o.value = f; padFamily.append(o); }
promptSel.value = promptMode;
promptSel.onchange = () => { promptMode = promptSel.value === "letters" ? "letters" : "auto"; ls.set("snatcher.prompts", promptMode); applyPrompt(); };
let capturing: GenesisButton | null = null;
const shownFamily = (): Family => padFamily.value as Family;
function renderPadMap() {
  const f = shownFamily(), set = setOf(f), m = input.maps[f];
  padMapTable.replaceChildren();
  for (const [g, whatKey] of GEN_ROWS) {
    const what = T(whatKey);
    const r = padMapTable.insertRow();
    r.insertCell().textContent = g === "START" ? what : T("pad.row", { what, g });
    iconCell(controlIcon(set, m[g]), CONTROL_NAMES[set][m[g]], r.insertCell());
    const b = el("button", undefined, capturing === g ? T("pad.press") : T("pad.assign")); b.type = "button";
    b.onclick = () => startCapture(g);
    r.insertCell().append(b);
  }
  const pads = input.pads();
  padInfo.textContent = pads.length ? T("pad.connected", { list: pads.map((p) => `${FAMILY_LABEL[p.family]}${p.mapping === "standard" ? "" : T("pad.nonstd")} "${parseGamepadId(p.id).name.slice(0, 50)}"`).join("; ") }) : T("pad.none");
  padHint.textContent = f === "generic" ? T("pad.generic") : "";
}
function startCapture(g: GenesisButton) {
  if (!input.pads().length) { padHint.textContent = T("pad.connect"); return; }
  capturing = g; renderPadMap();
  input.capture((c: Control) => {
    capturing = null;
    input.assign(shownFamily(), g, c);
    saveMaps(); renderPadMap();
  });
}
function cancelCapture() { if (capturing) { capturing = null; input.capture(null); renderPadMap(); } }
padFamily.onchange = () => { cancelCapture(); renderPadMap(); };
$("padReset").onclick = () => { cancelCapture(); input.resetMap(shownFamily()); saveMaps(); renderPadMap(); };

input.onDevice = () => applyPrompt();
input.onPads = () => {
  const p = input.pads();
  setMsg(p.length ? "msg.pad" : "msg.nopad", p.length ? { name: FAMILY_LABEL[p[p.length - 1].family] } : undefined);
  const cur = input.device;
  if (cur.kind === "pad") padFamily.value = cur.family;
  renderPadMap();
};
renderControls();

// ---------- state shared with the running game ----------
const store = new SaveStore();
let game: any = null, audio: AudioOut | null = null;
let crashed = false;

// ---------- language (page strings: i18n.ts + i18n/pt-BR/ui.json; game text: the engine, translation pack) ----------
// ?lang=pt-BR overrides the stored choice for this visit; the selectors in the start overlay and the menu panel persist it.
type MsgState = { key: UiKey; vars?: Record<string, string | number> };
let msgState: MsgState | null = null;
function setMsg(key: UiKey, vars?: Record<string, string | number>) { msgState = { key, vars }; msg.textContent = T(key, vars); }
const langOv = $<HTMLSelectElement>("langOv"), langPn = $<HTMLSelectElement>("langPn");
let wantLang: Lang = parseLang(params.get("lang")) ?? parseLang(ls.get("snatcher.lang")) ?? "en";
let uiCache: Record<string, string> | null = null;
const packLoaded = new WeakSet<object>();
async function fetchUi(): Promise<Record<string, string> | null> {
  if (uiCache) return uiCache;
  try { const r = await fetch("/i18n/pt-BR/ui.json"); if (!r.ok) return null; const j = await r.json(); uiCache = j.strings ?? j; return uiCache; } catch { return null; }
}
/** Install the pt-BR pack in an engine instance (once). */
async function ensurePack(g: any): Promise<boolean> {
  if (packLoaded.has(g)) return true;
  try {
    const r = await fetch("/i18n/pt-BR.pack");
    if (!r.ok) return false;
    const n = g.load_translation(new Uint8Array(await r.arrayBuffer()));
    if (n < 0) return false;
    packLoaded.add(g);
    return true;
  } catch { return false; }
}
// Voice subtitles (docs/voice-subtitles.md): the pt-BR pack carries a subtitle per voice clip, drawn in the empty message box while the clip plays.
// Default: on in pt-BR, off in English (there is no English subtitle data); an explicit choice is kept in localStorage (`snatcher.vsub` 1/0), ?vsub=0|1 overrides it for the visit.
const vsubBox = $<HTMLInputElement>("vsub");
const flag = (v: string | null): boolean | null => (v === "1" ? true : v === "0" ? false : null);
let vsubPref: boolean | null = flag(params.get("vsub")) ?? flag(ls.get("snatcher.vsub"));
function applyVsub() {
  const pt = wantLang === "pt-BR";
  vsubBox.disabled = !pt;
  vsubBox.checked = pt && (vsubPref ?? true);
  if (game) game.set_voice_subtitles(vsubBox.checked);
}
vsubBox.onchange = () => { vsubPref = vsubBox.checked; ls.set("snatcher.vsub", vsubPref ? "1" : "0"); applyVsub(); };
async function syncEngineLanguage(g: any) {
  let pt = wantLang === "pt-BR";
  if (pt) { pt = await ensurePack(g); note("pack", pt ? "ok" : "warn", pt ? null : T("note.nopack")); } else note("pack", "ok", null);
  g.set_language(pt ? "pt-BR" : "en");
  g.set_voice_subtitles(pt && vsubBox.checked);
}
function renderDynamic() {
  applyMute(); refreshSaves(); renderControls(); renderPadMap(); renderNotes(); updateSoundMode();
  if (msgState) msg.textContent = T(msgState.key, msgState.vars);
  if (srcLines) showSource(srcLines(), srcCls);
  if (!busy) btnPlay.textContent = T("ov.play");
}
async function applyLanguage(l: Lang, persist = true) {
  wantLang = l;
  if (persist) ls.set("snatcher.lang", l);
  setUiDict(l, l === "pt-BR" ? await fetchUi() : null);
  langOv.value = l; langPn.value = l;
  applyVsub();
  applyStatic();
  renderDynamic();
  if (game) await syncEngineLanguage(game);
}
langOv.onchange = () => { void applyLanguage(langOv.value as Lang); };
langPn.onchange = () => { void applyLanguage(langPn.value as Lang); };

function savesLabel(): string {
  const s = store.slots().map(([n]) => n);
  const base = s.length ? T("saves.slots", { list: s.join(", ") }) : T("saves.none");
  return base + (store.volatile ? T("saves.volatile") : "");
}
function refreshSaves() { $("ovSaves").textContent = savesLabel(); $("pnSaves").textContent = savesLabel() + (game ? T("pn.savesnote") : ""); }
refreshSaves();

function download(name: string, text: string) {
  const a = document.createElement("a");
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  a.href = url; a.download = name; document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
function exportSaves() {
  if (!store.slots().length) { setMsg("msg.nosaves"); note("saveio", "warn", T("note.nosaves")); return; }
  download(`snatcher-saves-${new Date().toISOString().slice(0, 10)}.json`, store.exportJson());
  note("saveio", "ok", T("note.exported"));
}
const importFile = $<HTMLInputElement>("importFile");
function importSaves() { importFile.value = ""; importFile.click(); }
importFile.onchange = async () => {
  const f = importFile.files?.[0];
  if (!f) return;
  const r = store.importJson(await f.text());
  for (const n of r.imported) { const b = store.bytes(n); if (b && game) game.set_save(n, b); }
  const text = r.imported.length ? T("note.imported", { slots: r.imported.join(", ") }) : T("note.notimported");
  note("saveio", r.errors.length || !r.imported.length ? "warn" : "ok", text + (r.errors.length ? " " + r.errors.join("; ") : ""));
  refreshSaves();
};
$("ovImport").onclick = importSaves; $("pnImport").onclick = importSaves;
$("ovExport").onclick = exportSaves; $("pnExport").onclick = exportSaves;

// ---------- volume / mute ----------
const VOLS: [string, "music" | "voice" | "fx"][] = [["volMusic", "music"], ["volVoice", "voice"], ["volFx", "fx"]];
const volume: Record<string, number> = {};
for (const [id, kind] of VOLS) {
  const el = $<HTMLInputElement>(id), out = $(id + "O");
  const saved = Number(ls.get("snatcher.vol." + kind));
  volume[kind] = ls.get("snatcher.vol." + kind) !== null && isFinite(saved) ? Math.max(0, Math.min(100, saved)) : 100;
  el.value = String(volume[kind]); out.textContent = `${volume[kind]}%`;
  el.oninput = () => { volume[kind] = Number(el.value); out.textContent = `${el.value}%`; ls.set("snatcher.vol." + kind, el.value); audio?.setVolume(kind, volume[kind] / 100); };
}
let muted = ls.get("snatcher.mute") === "1";
const btnMute = $("btnMute");
function applyMute() { btnMute.setAttribute("aria-pressed", String(muted)); btnMute.textContent = muted ? T("bar.unmute") : T("bar.mute"); audio?.setMuted(muted); }
function toggleMute() { muted = !muted; ls.set("snatcher.mute", muted ? "1" : "0"); applyMute(); }
btnMute.onclick = () => { toggleMute(); canvas.focus(); };
applyMute();

// ---------- fullscreen ----------
const canFull = !!document.fullscreenEnabled;
function toggleFullscreen() {
  if (!canFull) return;
  if (document.fullscreenElement) void document.exitFullscreen(); else void document.documentElement.requestFullscreen().catch(() => {});
}
for (const id of ["btnFull", "pnFull"]) { const b = $(id); if (!canFull) b.hidden = true; b.onclick = () => { toggleFullscreen(); if (id === "btnFull") canvas.focus(); }; }

// ---------- menu panel ----------
function openPanel() {
  if (overlayOpen || panelOpen || !game) return;
  panelOpen = true; panel.hidden = false; input.enabled = false;
  refreshSaves(); updateSoundMode(); renderPadMap();
  $("pnResume").focus();
}
function closePanel() {
  if (!panelOpen) return;
  cancelCapture(); panelOpen = false; panel.hidden = true; input.enabled = true;
  canvas.focus();
}
function updateSoundMode() {
  $("soundMode").textContent = audio ? T("pn.sound", { fm: audio.mono.fm ? T("pn.mono") : T("pn.stereo"), pcm: audio.mono.pcm ? T("pn.mono") : T("pn.stereo") }) : "";
}
$("btnMenu").onclick = () => openPanel();
$("pnResume").onclick = closePanel;
panel.addEventListener("mousedown", (e) => { if (e.target === panel) closePanel(); });

addEventListener("keydown", (e) => {
  if (e.key === "F1") { dbgBox.checked = !dbgBox.checked; applyDbg(); e.preventDefault(); return; }
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.key === "Escape") { if (capturing) { cancelCapture(); e.preventDefault(); return; } if (panelOpen) { closePanel(); e.preventDefault(); } else if (!overlayOpen && game) { openPanel(); e.preventDefault(); } return; }
  if (overlayOpen || panelOpen) return;
  if (e.code === "KeyF") { toggleFullscreen(); e.preventDefault(); }
  else if (e.code === "KeyM") { toggleMute(); e.preventDefault(); }
});
document.addEventListener("visibilitychange", () => { if (document.hidden) audio?.suspend(); });

input.onSelect = () => { if (panelOpen && !capturing) closePanel(); else openPanel(); };

// ---------- start overlay ----------
const btnPlay = $<HTMLButtonElement>("btnPlay"), btnPick = $<HTMLButtonElement>("btnPick"), pick = $<HTMLInputElement>("pick");
const srcStatus = $("srcStatus");
let current: Source | null = null;
let busy = false;

function showSource(extra: string[] = [], cls = "dim") {
  srcStatus.className = cls;
  srcStatus.replaceChildren(...extra.map((t, i) => { const d = document.createElement("div"); d.textContent = t; if (!i) d.style.fontWeight = "bold"; return d; }));
}
/** Status lines are produced by a function so that they can be rebuilt in the other language. */
let srcLines: (() => string[]) | null = null, srcCls = "dim";
function showSourceT(lines: () => string[], cls = "dim") { srcLines = lines; srcCls = cls; showSource(lines(), cls); }
function setBusy(b: boolean) { busy = b; btnPlay.disabled = b || !current; btnPick.disabled = b; }
function selectSource(src: Source) {
  current = src;
  showSourceT(() => [T("ov.using", { label: src.label }), ...src.summary()], src.gameFiles().length ? "ok" : "err");
  btnPlay.textContent = T("ov.play"); setBusy(false); btnPlay.focus();
}

btnPick.onclick = () => pick.click();
pick.onchange = () => {
  const files = Array.from(pick.files ?? []);
  if (!files.length) return;
  const src = pickedSource(files);
  if (!src.gameFiles().length) {
    showSourceT(() => [T("ov.nofiles"), T("ov.nofiles2", { n: files.length })], "err");
    current = null; setBusy(false);
    return;
  }
  selectSource(src);
};

async function loadWasm() {
  const pkg: any = await import(/* @vite-ignore */ "/pkg/snengine.js" as string);
  const wasm = await pkg.default();
  return { pkg, wasm };
}
const wasmP = loadWasm();
wasmP.catch((e) => { note("wasm", "err", T("note.noengine", { e: String(e) })); });

btnPlay.onclick = () => { if (current && !busy) void start(current); };

async function start(src: Source) {
  setBusy(true);
  try {
    const { pkg, wasm } = await wasmP;
    const g = new pkg.Game();
    // game files
    const files = src.gameFiles();
    let ok = 0, fail = 0;
    const pending = files.slice();
    const worker = async () => {
      for (let f = pending.pop(); f; f = pending.pop()) {
        try { g.load_file(f.name, new Uint8Array(await f.read())); ok++; } catch { fail++; }
        btnPlay.textContent = T("ov.loading", { done: ok + fail, total: files.length });
      }
    };
    await Promise.all(Array.from({ length: 6 }, worker));
    btnPlay.textContent = T("ov.play");
    if (!ok) throw new Error(T("note.nofile"));
    note("files", fail ? "warn" : "ok", fail ? T("note.failfiles", { fail, total: files.length }) : null);
    setMsg("msg.files", { n: ok });
    // main-CPU boot code (copyright + Konami logo screens): the disc's HEADER.BIN / IP.BIN; optional
    let boots = 0;
    for (const name of ["HEADER.BIN", "IP.BIN"]) { const b = await src.boot(name); if (b) { g.load_file(name, new Uint8Array(b)); boots++; } }
    note("boot", "warn", boots < 2 ? T("note.noboot") : null);
    applyVsub();
    await syncEngineLanguage(g);
    // saves → CONTINUE slots
    for (const n of SLOTS) { const b = store.bytes(n); if (b) g.set_save(n, b); }
    // audio
    const out = new AudioOut((p) => src.audio(p));
    let missingCount = 0;
    out.onMissing = (p) => { missingCount++; note("audiofiles", "warn", T("note.audio", { n: missingCount, last: p })); };
    for (const [, kind] of VOLS) out.setVolume(kind, volume[kind] / 100);
    out.setMuted(muted);
    const { pcm, fm } = await out.loadManifests();
    if (!pcm || !fm) note("manifests", "warn", T("note.manifests", { list: [!pcm && "audio/web/pcm.json", !fm && "audio/web/fm.json"].filter(Boolean).join(", "), what: !pcm && !fm ? T("note.manifests.all") : T("note.manifests.part") }));
    if (pcm?.voices) for (const [id, v] of Object.entries<any>(pcm.voices)) g.set_voice_len(Number(id), Math.ceil(v.seconds * 60));
    if (fm?.sounds) for (const [id, v] of Object.entries<any>(fm.sounds)) g.set_fm_len(Number(id), Math.ceil(v.seconds * 60), v.loop_start != null);
    if (pcm?.sfx) { // PCM effect lengths per bank (the shooting results wait for the last effect, S:0x13072)
      const len = (seg: any) => Math.ceil((seg.samples / seg.rate) * 60);
      for (const [id, s] of Object.entries<any>(pcm.sfx)) {
        if (s.src === "drum") { const seg = pcm.drum?.[s.offset]; if (seg) for (const b of Object.keys(pcm.banks ?? {})) g.set_sfx_len(Number(b), Number(id), len(seg), s.chan ?? 0); }
        else for (const [b, segs] of Object.entries<any>(pcm.banks ?? {})) { const seg = segs[s.offset]; if (seg) g.set_sfx_len(Number(b), Number(id), len(seg), s.chan ?? 0); }
      }
    }
    // CD-DA tracks are checked lazily (they are big); a missing one is reported when the game asks for it.
    if (params.has("scene")) g.debug_jump(params.get("scene")!);
    else if (params.has("newgame")) g.new_game();
    else g.boot();

    memory = wasm.memory; game = g; audio = out; input.game = g; input.enabled = true;
    (window as any).snatcher = { game, audio, input, store, notes, handleEvent, applyPrompt, setLanguage: applyLanguage, language: () => wantLang, prompt: () => lastPrompt, resolveIcon: (cfg: number, fn: any) => resolveIcon(cfg, fn, input.device, input.maps) };
    applyPrompt();
    overlayOpen = false; overlay.hidden = true;
    note("wasm", "ok", null);
    canvas.focus();
    refreshSaves(); fit();
    requestAnimationFrame(frame);
  } catch (e) {
    showSourceT(() => [T("ov.fail", { e: String((e as Error).message ?? e) })], "err");
    note("start", "err", String(e));
    setBusy(false);
  }
}

// ---------- the 60 Hz loop ----------
const pacer = new Pacer();
const img = ctx.createImageData(W, H);
let wasPaused = true, memory: WebAssembly.Memory | null = null;

function frame(now: number) {
  requestAnimationFrame(frame);
  input.scan(); // gamepads: bits, last used device, View/Share/- button, remap capture (also while a menu is open)
  const p = isPaused();
  if (p !== wasPaused) { wasPaused = p; pacer.reset(); if (p) audio?.suspend(); else audio?.resume(); }
  if (p || crashed) return;
  const n = pacer.advance(now);
  if (!n) return;
  try {
    for (let i = 0; i < n; i++) {
      game.tick(input.poll());
      for (const e of JSON.parse(game.take_events())) handleEvent(e);
    }
  } catch (e) {
    crashed = true; setMsg("msg.engine", { e: String(e) }); console.error(e);
    note("crash", "err", T("note.crash", { e: String(e) }));
    return;
  }
  img.data.set(new Uint8Array(memory!.buffer, game.framebuffer_ptr(), W * H * 4));
  ctx.putImageData(img, 0, 0);
  if (dbgBox.checked) debugEl.textContent = game.debug_info() + "\n" + (audio?.debugInfo() ?? "");
}

function handleEvent(e: any) {
  switch (e.t) {
    case "music": audio!.cd(e.track); break;
    case "music_stop": audio!.stopCd(); break;
    case "fm": audio!.fmCmd(e.id, e.music); break;
    case "pcm": audio!.fmCmd(e.id, false); break; // menus sfx_1c: queue command 0x1C = FM sound effect (S:0x1013E)
    case "voice": audio!.voice(e.id); break;
    case "pcmsfx": audio!.pcmSfx(e.id, e.bank); break;
    case "log": console.log("[engine]", e.msg); break;
    case "save": {
      store.set(e.slot, e.data);
      const b = store.bytes(e.slot); if (b) game.set_save(e.slot, b); // the engine also updates its own copy (game.rs cmd 0x1E)
      if (store.volatile) note("storage", "warn", T("note.storage"));
      setMsg("msg.saved", { n: e.slot });
      break;
    }
  }
}

// ---------- go ----------
(async () => {
  fit();
  await applyLanguage(wantLang, false);
  const r = await probeServed();
  if (r.src) selectSource(r.src);
  else { showSourceT(() => [T("ov.nolocal"), T("ov.nolocal2", { err: r.error ? T(r.error as UiKey) : "" })], "warn"); setBusy(false); btnPick.focus(); }
  setMsg(r.src ? "msg.ready" : "msg.waiting");
  // headless/dev shortcuts skip the overlay (and its click): ?newgame, ?scene=SP01:0, ?autostart
  if (r.src && (params.has("scene") || params.has("newgame") || params.has("autostart"))) await start(r.src);
})();
