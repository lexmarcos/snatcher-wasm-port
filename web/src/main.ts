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
import { AutosaveStore, Autosaver, PREF_KEY, readPref, reasonKey, type AutoRecord } from "./autosave";
import { pickedSource, probeServed, type Source } from "./sources";
import { loadEngine, siteUrl } from "./urls";
import { clearCache, fmtBytes, opfsAvailable, readMeta, requestPersist, storageInfo } from "./cache";
import { ImportedSource, ImportWorker, resolvePick, type Pick } from "./imported";
import { T, applyStatic, getLang, parseLang, setUiDict, type Lang, type UiKey } from "./i18n";

const W = 256, H = 224;
const params = new URLSearchParams(location.search);
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const msg = $("msg"), canvas = $<HTMLCanvasElement>("screen"), ctx = canvas.getContext("2d")!, debugEl = $("debug");
const bar = $("bar"), overlay = $("overlay"), panel = $("panel"), monitor = $("monitor");
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
  // the monitor frame (bezel padding) and the toolbar are not part of the canvas: measure them instead of hard-coding
  const mr = monitor.getBoundingClientRect(), cr = canvas.getBoundingClientRect();
  const frameX = mr.width - cr.width, frameY = mr.height - cr.height;
  const fs = !!document.fullscreenElement;
  const availW = innerWidth - (fs ? 0 : 32) - frameX;
  const availH = innerHeight - (fs ? 0 : bar.offsetHeight + 24) - frameY;
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
    const b = el("button", "btn", capturing === g ? T("pad.press") : T("pad.assign")); b.type = "button";
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
const langBtns = Array.from(document.querySelectorAll<HTMLButtonElement>("button[data-lang]"));
let wantLang: Lang = parseLang(params.get("lang")) ?? parseLang(ls.get("snatcher.lang")) ?? "en";
let uiCache: Record<string, string> | null = null;
const packLoaded = new WeakSet<object>();
async function fetchUi(): Promise<Record<string, string> | null> {
  if (uiCache) return uiCache;
  try { const r = await fetch(siteUrl("i18n/pt-BR/ui.json")); if (!r.ok) return null; const j = await r.json(); uiCache = j.strings ?? j; return uiCache; } catch { return null; }
}
/** Install the pt-BR pack in an engine instance (once). */
async function ensurePack(g: any): Promise<boolean> {
  if (packLoaded.has(g)) return true;
  try {
    const r = await fetch(siteUrl("i18n/pt-BR.pack"));
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
  if (impLine) showImp(impLine, impCls);
  btnClear.textContent = T("imp.remove"); void refreshStorage(); refreshImportButtons();
  if (altLines) showAlt(altLines, altCls);
  if (!busy) btnPlay.textContent = T("ov.play");
  refreshAuto();
}
async function applyLanguage(l: Lang, persist = true) {
  wantLang = l;
  if (persist) ls.set("snatcher.lang", l);
  setUiDict(l, l === "pt-BR" ? await fetchUi() : null);
  for (const b of langBtns) b.setAttribute("aria-pressed", String(b.dataset.lang === l));
  applyVsub();
  applyStatic();
  renderDynamic();
  if (game) await syncEngineLanguage(game);
}
for (const b of langBtns) b.onclick = () => { void applyLanguage(b.dataset.lang as Lang); };

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
  const paint = () => { out.textContent = `${el.value}%`; el.style.setProperty("--v", `${el.value}%`); }; // --v: filled part of the segmented track
  el.value = String(volume[kind]); paint();
  el.oninput = () => { volume[kind] = Number(el.value); paint(); ls.set("snatcher.vol." + kind, el.value); audio?.setVolume(kind, volume[kind] / 100); };
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
// tabs of the pause panel (WAI-ARIA tabs: arrow keys, Home and End move and select, only the selected tab is in the tab order)
const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>("[role=tab]"));
function selectTab(t: HTMLButtonElement, focus = false) {
  for (const x of tabs) { const on = x === t; x.setAttribute("aria-selected", String(on)); x.tabIndex = on ? 0 : -1; $(x.getAttribute("aria-controls")!).hidden = !on; }
  if (focus) t.focus();
}
for (const t of tabs) {
  t.onclick = () => selectTab(t);
  t.onkeydown = (e) => {
    const i = tabs.indexOf(t);
    const n = e.key === "ArrowRight" ? (i + 1) % tabs.length : e.key === "ArrowLeft" ? (i + tabs.length - 1) % tabs.length : e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : -1;
    if (n >= 0) { e.preventDefault(); selectTab(tabs[n], true); }
  };
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
document.addEventListener("visibilitychange", () => { if (document.hidden) { audio?.suspend(); void autosaver.flush(); } });
addEventListener("pagehide", () => { void autosaver.flush(); });

input.onSelect = () => { if (panelOpen && !capturing) closePanel(); else openPanel(); };

// ---------- start overlay ----------
const btnPlay = $<HTMLButtonElement>("btnPlay"), btnPick = $<HTMLButtonElement>("btnPick"), pick = $<HTMLInputElement>("pick");
const srcStatus = $("srcStatus");
let current: Source | null = null, servedSrc: Source | null = null;
let busy = false;

// ---------- autosave (docs/re/web.md section 9) ----------
// One snapshot of the running game in IndexedDB, apart from the four save slots. Saved at quiet moments (command menu, a message that waits for a
// button) and when the page is hidden; "Continue where you left off" restores it. On by default; the choice is kept in localStorage (snatcher.autosave.on).
const autoBox = $<HTMLInputElement>("autoBox"), btnContinue = $<HTMLButtonElement>("btnContinue"), autoStatus = $("autoStatus");
const pnAuto = $("pnAuto"), ovAuto = $("ovAuto"), pnAutoDelete = $<HTMLButtonElement>("pnAutoDelete"), ovAutoDelete = $<HTMLButtonElement>("ovAutoDelete");
const autoStore = new AutosaveStore();
let autoOn = readPref((k) => ls.get(k));
let autoRec: AutoRecord | null = null;
/** why the last autosave was thrown away, shown under the buttons until the next start */
let autoDropped: UiKey | null = null;
const autosaver = new Autosaver(autoStore, () => game, () => ({ fm: audio?.mono.fm ?? false, pcm: audio?.mono.pcm ?? false }), (rec) => {
  if (rec) { autoRec = rec; autoDropped = null; setMsg("msg.autosaved"); } else setMsg("msg.autosave.fail");
  note("autostore", "warn", autoStore.volatile ? T("auto.volatile") : null);
  refreshAuto();
});
autosaver.on = autoOn;
autoBox.checked = autoOn;
(window as any).snatcherAutosave = { store: autoStore, saver: autosaver, record: () => autoRec }; // for the tests, also while the start window is open
autoBox.onchange = () => { autoOn = autoBox.checked; autosaver.on = autoOn; ls.set(PREF_KEY, autoOn ? "1" : "0"); refreshAuto(); };

const autoWhen = (rec: AutoRecord) => new Date(rec.stamp).toLocaleString(wantLang === "pt-BR" ? "pt-BR" : "en", { dateStyle: "medium", timeStyle: "short" });
function playText(hms: string): string { // "HH:MM:SS" (game clock) -> "1 h 05 min" / "25 min 12 s" / "40 s"
  const [h, m, s] = hms.split(":").map(Number);
  return h > 0 ? `${h} h ${String(m).padStart(2, "0")} min` : m > 0 ? `${m} min ${String(s).padStart(2, "0")} s` : `${s} s`;
}
function autoLine(rec: AutoRecord): string {
  const vars = { when: autoWhen(rec), act: rec.act, play: playText(rec.play) };
  return T(rec.act > 0 ? "auto.line" : "auto.line.noact", vars);
}
/** The Continue button, the status lines of the start window and of the Saves tab; called whenever the autosave, its setting or the start state changes. */
/** The last sentence of the source line: which button starts the game. */
const goHint = () => T(btnContinue.hidden ? "ov.go.play" : "ov.go.cont");
function refreshAuto() {
  const show = !!autoRec && !!current && !importing;
  btnContinue.hidden = !show; btnContinue.disabled = busy;
  btnPlay.classList.toggle("primary", !show); // Continue is the primary action when there is an autosave
  autoStatus.className = "line " + (autoDropped ? "warn" : "dim");
  autoStatus.textContent = autoDropped ? T(autoDropped) : show ? autoLine(autoRec!) : "";
  const lines: string[] = [];
  if (autoRec) lines.push(autoOn ? autoLine(autoRec) : T("auto.keep", { when: autoWhen(autoRec) }));
  else if (autoOn) lines.push(T("auto.none"));
  if (!autoOn) lines.push(T("auto.off"));
  if (autoStore.volatile) lines.push(T("auto.volatile"));
  pnAuto.replaceChildren(...lines.map((t) => { const d = document.createElement("div"); d.textContent = t; return d; }));
  ovAuto.textContent = autoRec ? autoLine(autoRec) : "";
  for (const b of [pnAutoDelete, ovAutoDelete]) b.hidden = !autoRec;
  if (srcLines) showSource(srcLines(), srcCls); // the "press Play" hint names Continue when it is shown
}
let autoDelArmed = 0;
async function deleteAuto() {
  if (!autoDelArmed) {
    for (const x of [pnAutoDelete, ovAutoDelete]) x.textContent = T("auto.delete.confirm");
    autoDelArmed = window.setTimeout(() => { autoDelArmed = 0; for (const x of [pnAutoDelete, ovAutoDelete]) x.textContent = T("auto.delete"); }, 4000);
    return;
  }
  clearTimeout(autoDelArmed); autoDelArmed = 0;
  for (const x of [pnAutoDelete, ovAutoDelete]) x.textContent = T("auto.delete");
  await autoStore.clear(); autoRec = null; autoDropped = null;
  setMsg("auto.deleted"); refreshAuto();
}
pnAutoDelete.onclick = () => void deleteAuto(); ovAutoDelete.onclick = () => void deleteAuto();
/** Throw the stored autosave away with a plain message (another engine build or disc, or damage). */
async function dropAuto(code: string) {
  await autoStore.clear(); autoRec = null;
  autoDropped = code.startsWith("lang") ? "auto.err.lang" : reasonKey(code);
  refreshAuto();
}
/** Read the stored autosave and check that this engine build understands it. */
async function loadAuto() {
  autoRec = await autoStore.get();
  if (autoRec) {
    try {
      const { pkg } = await wasmP;
      const h = JSON.parse(pkg.snapshot_header(autoRec.data));
      if (!h.ok) await dropAuto(h.error);
    } catch { /* no engine yet: the restore checks again */ }
  }
  refreshAuto();
  // Continue is the primary action: it takes the focus Play had (an overlay that is not open any more, or a focus the player moved, is left alone)
  if (!btnContinue.hidden && overlayOpen && document.activeElement === btnPlay) btnContinue.focus();
}

function lineBlock(host: HTMLElement, lines: string[], cls: string) {
  host.className = "line " + cls;
  host.replaceChildren(...lines.map((t) => { const d = document.createElement("div"); d.textContent = t; return d; }));
}
function showSource(extra: string[] = [], cls = "dim") { lineBlock(srcStatus, extra, cls); }
/** Status of the "Other ways to load" disclosure (served copy, picked folder); rebuilt on a language change like the others. */
const altStatus = $("altStatus");
let altLines: (() => string[]) | null = null, altCls = "dim";
function showAlt(lines: (() => string[]) | null, cls = "dim") { altLines = lines; altCls = cls; lineBlock(altStatus, lines ? lines() : [], cls); }
/** Status lines are produced by a function so that they can be rebuilt in the other language. */
let srcLines: (() => string[]) | null = null, srcCls = "dim";
function showSourceT(lines: () => string[], cls = "dim") { srcLines = lines; srcCls = cls; showSource(lines(), cls); }
/** First visit (no game source yet): only step 1 is shown; step 2 and the folds appear once a source exists. */
function refreshStage() {
  // disc: nothing yet, only the import; imported: a disc is in this browser, only Play and the rest; ready: a served / picked copy, both steps
  const stage = !current && !importedSrc ? "disc" : importedSrc && !importing ? "imported" : "ready";
  $("ovWin").dataset.stage = stage; $("ovAfter").hidden = stage === "disc"; $("ovStep1").hidden = stage === "imported";
  refreshAuto();
}
function setBusy(b: boolean) { busy = b; btnPlay.disabled = b || !current; btnPick.disabled = b; btnContinue.disabled = b; refreshImportButtons(); }
const invite = () => showSourceT(() => [T("ov.invite")], "dim");
function selectSource(src: Source) {
  current = src;
  const cls = src.gameFiles().length ? "ok" : "err";
  if (src === importedSrc) showSourceT(() => [T("ov.using", { label: src.label }), ...src.summary()], cls);
  else { // a served or picked copy: one plain line here, the details under "Other ways to load"
    showSourceT(() => [src === servedSrc ? T("ov.usinglocal", { go: goHint() }) : T("ov.using", { label: src.label })], cls);
    showAlt(() => [T("ov.using", { label: src.label }), ...src.summary()], cls === "ok" ? "dim" : "err");
  }
  btnPlay.textContent = T("ov.play"); setBusy(false); btnPlay.focus();
  refreshImportButtons();
}

btnPick.onclick = () => pick.click();
pick.onchange = () => {
  const files = Array.from(pick.files ?? []);
  if (!files.length) return;
  const src = pickedSource(files);
  if (!src.gameFiles().length) {
    showAlt(() => [T("ov.nofiles"), T("ov.nofiles2", { n: files.length })], "err");
    current = null; invite(); setBusy(false);
    return;
  }
  selectSource(src);
};

// ---------- disc image import (docs/re/web.md section 8): .cue + .bin file(s) -> cache in OPFS -> "imported" Source ----------
const btnImport = $<HTMLButtonElement>("btnImport"), discPick = $<HTMLInputElement>("discPick"), impCd = $<HTMLInputElement>("impCd");
const impBox = $("impBox"), impProg = $("impProg"), impBar = $<HTMLProgressElement>("impBar"), impPhase = $("impPhase");
const impStatus = $("impStatus"), impFm = $("impFm"), impStorage = $("impStorage"), bgStatus = $("bgStatus");
const btnUseCache = $<HTMLButtonElement>("btnUseCache"), btnClear = $<HTMLButtonElement>("btnClear"), btnCancel = $<HTMLButtonElement>("btnCancel");
const importer = new ImportWorker();
let importedSrc: ImportedSource | null = null;
let importing = false;
const importStats = { runs: 0, lastMs: 0, worker: importer };
(window as any).snatcherImport = importStats;

/** Import status line: a function so that it is rebuilt when the language changes. */
let impLine: (() => string) | null = null, impCls = "dim";
function showImp(line: (() => string) | null, cls = "dim") { impLine = line; impCls = cls; impStatus.className = "line " + cls; impStatus.textContent = line ? line() : ""; }
const PHASES = ["header", "files", "voices", "fm", "cd", "done"] as const;
const phaseText = (p: string) => T(`imp.phase.${(PHASES as readonly string[]).includes(p) ? p : "files"}` as UiKey);

async function refreshStorage() {
  if (!opfsAvailable()) { impStorage.textContent = ""; return; }
  const si = await storageInfo();
  impStorage.textContent = T("imp.storage", { used: fmtBytes(si.used), quota: fmtBytes(si.quota), cache: fmtBytes(si.cache), persist: si.persisted ? T("imp.persist.yes") : T("imp.persist.no") });
  impStorage.dataset.cache = String(si.cache);
}
function refreshImportButtons() {
  refreshStage();
  btnUseCache.hidden = !importedSrc || current === importedSrc || importing;
  btnClear.hidden = !importedSrc || importing;
  btnCancel.hidden = !importing;
  // one button, three labels: Import disc image -> Importing -> Imported (Remove imported data brings the first one back)
  btnImport.textContent = importing ? T("imp.button.busy") : importedSrc ? T("imp.button.done") : T("imp.button");
  btnImport.disabled = busy || importing || !!importedSrc;
}
/** Fill the block meter: the value always ends on a block edge (the meter is a whole number of 28 px blocks wide, see system.css). */
function setProgress(frac: number) {
  const n = Math.max(1, Math.round((impBar.clientWidth + 4) / 28));
  const k = Math.min(n, Math.ceil(Math.min(1, frac) * n - 1e-9));
  impBar.value = k <= 0 ? 0 : Math.round((1000 * (28 * k - 4)) / (28 * n - 4));
}
let lastStorage = 0;
function fmStatus(text: string | null) {
  bgStatus.textContent = text ?? "";
  impFm.textContent = text ?? "";
  if (text === null || performance.now() - lastStorage > 5000) { lastStorage = performance.now(); void refreshStorage(); } // the cache grows with every sound
}
async function openCached(): Promise<ImportedSource | null> {
  const meta = await readMeta();
  if (!meta) return null;
  const src = await new ImportedSource(meta, importer, null).init();
  src.onStatus = fmStatus;
  importedSrc = src;
  return src;
}
function foundLine(src: ImportedSource) { return () => T("imp.found", { title: src.meta.title, size: fmtBytes(Object.values(src.meta.streams).reduce((a, b) => a + b, 0)), go: goHint() }); }

/** Message for an import / pick error: `imp.err.<code>` with the details. */
function errText(code: string, detail: string, extra: Record<string, string | number> = {}): string {
  const key = ("imp.err." + code) as UiKey;
  return T(key, { detail, ...extra });
}

async function runImport(picked: File[]) {
  if (importing || busy || !picked.length) return;
  if (!opfsAvailable()) { showImp(() => T("imp.nosupport"), "err"); return; }
  importing = true; setBusy(false); refreshImportButtons();
  impProg.hidden = true;
  try {
    const { pkg } = await wasmP;
    showImp(() => T("imp.reading"), "dim");
    const pick = await resolvePick(picked, (text) => JSON.parse(pkg.cue_files(text)));
    if (!("cueText" in pick)) { showImp(() => errText(pick.code, pick.detail, { n: picked.length }), "err"); return; }
    const copyCd = impCd.checked;
    // storage: the import needs about 175 MB, 490 MB more for the CD music, ~560 MB for the FM sounds rendered afterwards
    try {
      const est = await navigator.storage.estimate();
      const free = (est.quota ?? Infinity) - (est.usage ?? 0), need = (175 + 560 + (copyCd ? 490 : 0)) * 2 ** 20;
      if (free < need) { showImp(() => T("imp.nospace", { need: fmtBytes(need), free: fmtBytes(Math.max(0, free)) }), "err"); return; }
    } catch { /* no estimate: try anyway */ }
    await requestPersist(); // must come from this click; the browser may say no
    importStats.runs++;
    importedSrc?.dispose(); importedSrc = null; refreshImportButtons();
    const t0 = performance.now();
    impProg.hidden = false; setProgress(0); impPhase.textContent = phaseText("header");
    showImp(() => pick.notes.join("; "), "dim");
    const meta = await importer.import(pick, copyCd, (p) => {
      setProgress(p.done / Math.max(1, p.total));
      impPhase.textContent = `${phaseText(p.phase)} ${Math.round((p.done / Math.max(1, p.total)) * 100)}%`;
    });
    importStats.lastMs = performance.now() - t0;
    const src = await new ImportedSource(meta, importer, copyCd ? null : pick.files).init();
    src.onStatus = fmStatus;
    importedSrc = src;
    selectSource(src);
    src.start(); // FM sounds are rendered in the background from now on
    const done = () => T("imp.done", { s: (importStats.lastMs / 1000).toFixed(1), go: goHint() });
    showImp(done, "ok");
    showSourceT(() => [done(), ...src.summary()], "ok"); // step 1 is hidden from now on: the result is told under Play
  } catch (e: any) {
    const code = e?.code ?? "io", detail = e?.detail ?? String(e?.message ?? e);
    showImp(() => code === "cancelled" ? T("imp.err.cancelled") : errText(code, detail), code === "cancelled" ? "warn" : "err");
    importedSrc = await openCached().catch(() => null);
  } finally {
    importing = false; impProg.hidden = true; setBusy(false); refreshImportButtons(); void refreshStorage();
  }
}

btnImport.onclick = () => { discPick.value = ""; discPick.click(); };
discPick.onchange = () => { void runImport(Array.from(discPick.files ?? [])); };
btnCancel.onclick = async () => { importer.cancel(); try { await clearCache(); } catch { /* ignore */ } };
btnUseCache.onclick = () => { if (importedSrc) selectSource(importedSrc); };
let clearArmed = 0;
btnClear.onclick = async () => {
  if (!clearArmed) {
    btnClear.textContent = T("imp.remove.confirm");
    clearArmed = window.setTimeout(() => { clearArmed = 0; btnClear.textContent = T("imp.remove"); }, 4000);
    return;
  }
  clearTimeout(clearArmed); clearArmed = 0; btnClear.textContent = T("imp.remove");
  importedSrc?.dispose();
  if (current === importedSrc) { current = null; btnPlay.disabled = true; invite(); }
  importedSrc = null;
  try { await clearCache(); showImp(() => T("imp.removed"), "ok"); } catch (e) { showImp(() => errText("io", String(e)), "err"); }
  refreshImportButtons(); void refreshStorage(); void impFm;
};

// "Clear the site": the imported disc (OPFS) and every snatcher.* key of this origin (saves, gamepad buttons, settings), then a fresh first visit
const btnWipe = $<HTMLButtonElement>("btnWipe"), wipeStatus = $("wipeStatus");
let wipeArmed = 0;
btnWipe.onclick = async () => {
  if (!wipeArmed) {
    btnWipe.textContent = T("ov.wipe.confirm");
    wipeArmed = window.setTimeout(() => { wipeArmed = 0; btnWipe.textContent = T("ov.wipe.button"); }, 4000);
    return;
  }
  clearTimeout(wipeArmed); wipeArmed = 0; btnWipe.disabled = true;
  if (importing) importer.cancel();
  importedSrc?.dispose(); importedSrc = null;
  try {
    if (opfsAvailable()) await clearCache();
    await AutosaveStore.wipe(); // the autosave (IndexedDB) goes with everything else; "Remove imported data" keeps it
    try { for (const k of Object.keys(localStorage)) if (k.startsWith("snatcher")) localStorage.removeItem(k); } catch { /* blocked storage: nothing kept there */ }
    try { sessionStorage.clear(); } catch { /* ignore */ }
    location.replace(location.pathname); // reload without ?params: the page opens as a first visit
  } catch (e) {
    btnWipe.disabled = false; btnWipe.textContent = T("ov.wipe.button");
    wipeStatus.className = "line err"; wipeStatus.textContent = errText("io", String(e));
  }
};

// drag and drop of the files (or of the folder that holds them) anywhere on the overlay
async function filesFromDrop(dt: DataTransfer): Promise<File[]> {
  const entries = Array.from(dt.items ?? []).map((i) => (i.kind === "file" ? (i as any).webkitGetAsEntry?.() ?? null : null));
  if (!entries.some((e) => e?.isDirectory)) return Array.from(dt.files);
  const out: File[] = [];
  const readDir = async (d: any, depth: number) => {
    const r = d.createReader();
    for (;;) {
      const batch: any[] = await new Promise((ok, no) => r.readEntries(ok, no));
      if (!batch.length) break;
      for (const e of batch) {
        if (e.isFile) out.push(await new Promise<File>((ok, no) => e.file(ok, no)));
        else if (e.isDirectory && depth < 2) await readDir(e, depth + 1);
      }
    }
  };
  for (const e of entries) {
    if (!e) continue;
    if (e.isFile) out.push(await new Promise<File>((ok, no) => e.file(ok, no)));
    else await readDir(e, 0);
  }
  return out;
}
const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes("Files");
let dragDepth = 0;
overlay.addEventListener("dragenter", (e) => { if (hasFiles(e)) { dragDepth++; impBox.classList.add("drag"); e.preventDefault(); } });
overlay.addEventListener("dragover", (e) => { if (hasFiles(e)) e.preventDefault(); });
overlay.addEventListener("dragleave", () => { dragDepth = Math.max(0, dragDepth - 1); if (!dragDepth) impBox.classList.remove("drag"); });
overlay.addEventListener("drop", (e) => {
  if (!hasFiles(e)) return;
  e.preventDefault(); dragDepth = 0; impBox.classList.remove("drag");
  if (!overlayOpen) return;
  void filesFromDrop(e.dataTransfer!).then((f) => runImport(f));
});
addEventListener("dragover", (e) => { if (hasFiles(e)) e.preventDefault(); }); // a file dropped outside the card must not navigate away
addEventListener("drop", (e) => { if (hasFiles(e)) e.preventDefault(); });

async function loadWasm() {
  return loadEngine();
}
const wasmP = loadWasm();
wasmP.catch((e) => { note("wasm", "err", T("note.noengine", { e: String(e) })); });

btnPlay.onclick = () => { if (current && !busy) void start(current); };
btnContinue.onclick = async () => {
  const rec = autoRec;
  if (!current || busy || !rec) return;
  autoDropped = null;
  // the snapshot holds the screen in the language it was made in: continue in that language
  if (rec.pt !== (wantLang === "pt-BR")) await applyLanguage(rec.pt ? "pt-BR" : "en", false);
  void start(current, rec);
};

async function start(src: Source, resume?: AutoRecord) {
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
    if (src.direct) out.direct = (p, c) => src.direct!(p, c);
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
    if (resume) {
      // "Continue": the engine takes the autosave back (screen, scripts, menus, music position); the page restores the sound mode the game had set
      const err: string = g.restore(resume.data);
      if (err) { try { g.free(); } catch { /* ignore */ } out.stopCd(); await dropAuto(err); setBusy(false); return; }
      out.setSoundMode("fm", resume.mono.fm); out.setSoundMode("pcm", resume.mono.pcm);
    } else if (params.has("scene")) g.debug_jump(params.get("scene")!);
    else if (params.has("newgame")) g.new_game();
    else g.boot();

    memory = wasm.memory; game = g; audio = out; input.game = g; input.enabled = true;
    autosaver.adopt(g, !!resume);
    src.start?.();
    (window as any).snatcher = { game, audio, input, store, autosaver, autoStore, autoRecord: () => autoRec, memory, started: { frame: JSON.parse(g.debug_info()).frame, fb: fbHash(g), resumed: !!resume }, notes, source: src, handleEvent, applyPrompt, setLanguage: applyLanguage, language: () => wantLang, prompt: () => lastPrompt, resolveIcon: (cfg: number, fn: any) => resolveIcon(cfg, fn, input.device, input.maps) };
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
  audio?.syncCd(game.cd_position()); // CD-DA audio follows the engine's CD clock (the subtitles use it)
  autosaver.poll();
  img.data.set(new Uint8Array(memory!.buffer, game.framebuffer_ptr(), W * H * 4));
  ctx.putImageData(img, 0, 0);
  if (dbgBox.checked) debugEl.textContent = game.debug_info() + "\n" + (audio?.debugInfo() ?? "") + "\n" + autosaveDebug();
}

/** FNV-1a of the engine's frame buffer (tests compare the screen a restore brings back with the screen at the time of the save). */
function fbHash(g: any): number {
  const b = new Uint8Array(memory!.buffer, g.framebuffer_ptr(), W * H * 4);
  let h = 0x811c9dc5;
  for (let i = 0; i < b.length; i++) h = Math.imul(h ^ b[i], 0x01000193) >>> 0;
  return h;
}

function autosaveDebug(): string {
  const s = autosaver.stats;
  return `autosave: ${autoOn ? "on" : "off"}, ${game.autosave_reason() || "safe point"}, saved ${s.saves}x, last ${s.lastBytes} B in ${s.lastTotalMs.toFixed(1)} ms (engine ${s.lastSnapMs.toFixed(2)} ms)`;
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
  servedSrc = r.src ?? null;
  const cached = await openCached().catch(() => null);
  // a disc imported in this browser wins over the dev server's ./assets: the player chose it
  const first: Source | null = cached ?? r.src ?? null;
  if (cached) { selectSource(cached); showSourceT(() => [foundLine(cached)(), ...cached.summary().slice(1)], "ok"); /* the found line already names the disc */ }
  else if (r.src) selectSource(r.src);
  else {
    // a player without data gets an invitation, not an error; the probe's detail is for the developer, under "Other ways to load"
    invite(); showAlt(() => [T("ov.nolocal"), ...(r.error ? [T(r.error as UiKey)] : [])], "dim");
    setBusy(false); btnImport.focus();
  }
  if (cached) showImp(foundLine(cached), "ok");
  refreshImportButtons(); void refreshStorage();
  await loadAuto();
  setMsg(first ? "msg.ready" : "msg.waiting");
  // headless/dev shortcuts skip the overlay (and its click): ?newgame, ?scene=SP01:0, ?autostart
  if (first && (params.has("scene") || params.has("newgame") || params.has("autostart"))) await start(first);
})();
