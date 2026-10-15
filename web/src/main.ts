// Snatcher web port – browser front end.
// The engine (Rust → wasm) owns all game logic and renders a 256x224 RGBA frame (H32 mode).
// This file handles: asset sources, the start overlay and menu, the fixed 60 Hz loop, canvas scaling,
// input, audio and saves. See docs/re/web.md.
import { AudioOut } from "./audio";
import { Input } from "./input";
import { Pacer } from "./pacer";
import { SaveStore, SLOTS } from "./saves";
import { pickedSource, probeServed, type Source } from "./sources";

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
  for (const id of ["notes", "pnNotes"]) {
    const ul = $(id); ul.replaceChildren();
    for (const n of notes.values()) { const li = document.createElement("li"); li.className = n.level; li.textContent = n.text; ul.append(li); }
    if (!notes.size && id === "pnNotes") { const li = document.createElement("li"); li.className = "ok"; li.textContent = "Everything loaded."; ul.append(li); }
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

// ---------- controls help ----------
const CONTROLS: [string, string][] = [
  ["D-pad", "Arrow keys / gamepad D-pad or left stick"],
  ["A / B / C", "Z / X / C (or A / S / D); Space = B; gamepad X / A / B buttons"],
  ["Start", "Enter / gamepad Start"],
  ["Light gun", "Mouse: move to aim, left button = trigger, right button = second button (holster); pressing any pad key switches back to the pad"],
  ["Menu", "Esc (pauses the game)"],
  ["Fullscreen / mute", "F / M"],
  ["Debug overlay", "F1"],
];
for (const el of Array.from(document.querySelectorAll(".controls"))) {
  const t = document.createElement("table"); t.className = "keys";
  for (const [k, v] of CONTROLS) { const r = t.insertRow(); r.insertCell().textContent = k; r.insertCell().textContent = v; }
  el.append(t);
}

// ---------- state shared with the running game ----------
const store = new SaveStore();
let game: any = null, audio: AudioOut | null = null, input: Input | null = null;
let crashed = false;

function savesLabel(): string {
  const s = store.slots().map(([n]) => n);
  const base = s.length ? `Saved slots: ${s.join(", ")}.` : "No saves yet.";
  return base + (store.volatile ? " Browser storage is unavailable: saves are kept for this session only, export them." : "");
}
function refreshSaves() { $("ovSaves").textContent = savesLabel(); $("pnSaves").textContent = savesLabel() + (game ? " Imported saves show up the next time the title screen is opened." : ""); }
refreshSaves();

function download(name: string, text: string) {
  const a = document.createElement("a");
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  a.href = url; a.download = name; document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
function exportSaves() {
  if (!store.slots().length) { msg.textContent = "no saves to export"; note("saveio", "warn", "No saves to export."); return; }
  download(`snatcher-saves-${new Date().toISOString().slice(0, 10)}.json`, store.exportJson());
  note("saveio", "ok", "Saves exported.");
}
const importFile = $<HTMLInputElement>("importFile");
function importSaves() { importFile.value = ""; importFile.click(); }
importFile.onchange = async () => {
  const f = importFile.files?.[0];
  if (!f) return;
  const r = store.importJson(await f.text());
  for (const n of r.imported) { const b = store.bytes(n); if (b && game) game.set_save(n, b); }
  const text = r.imported.length ? `Imported save slot(s) ${r.imported.join(", ")}.` : "No save imported.";
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
function applyMute() { btnMute.setAttribute("aria-pressed", String(muted)); btnMute.textContent = muted ? "Unmute (M)" : "Mute (M)"; audio?.setMuted(muted); }
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
  panelOpen = true; panel.hidden = false; if (input) input.enabled = false;
  refreshSaves(); updateSoundMode();
  $("pnResume").focus();
}
function closePanel() {
  if (!panelOpen) return;
  panelOpen = false; panel.hidden = true; if (input) input.enabled = true;
  canvas.focus();
}
function updateSoundMode() {
  $("soundMode").textContent = audio ? `Sound mode (set by the game's options screen): FM ${audio.mono.fm ? "mono" : "stereo"}, PCM ${audio.mono.pcm ? "mono" : "stereo"}.` : "";
}
$("btnMenu").onclick = () => openPanel();
$("pnResume").onclick = closePanel;
panel.addEventListener("mousedown", (e) => { if (e.target === panel) closePanel(); });

addEventListener("keydown", (e) => {
  if (e.key === "F1") { dbgBox.checked = !dbgBox.checked; applyDbg(); e.preventDefault(); return; }
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.key === "Escape") { if (panelOpen) { closePanel(); e.preventDefault(); } else if (!overlayOpen && game) { openPanel(); e.preventDefault(); } return; }
  if (overlayOpen || panelOpen) return;
  if (e.code === "KeyF") { toggleFullscreen(); e.preventDefault(); }
  else if (e.code === "KeyM") { toggleMute(); e.preventDefault(); }
});
document.addEventListener("visibilitychange", () => { if (document.hidden) audio?.suspend(); });

// ---------- start overlay ----------
const btnPlay = $<HTMLButtonElement>("btnPlay"), btnPick = $<HTMLButtonElement>("btnPick"), pick = $<HTMLInputElement>("pick");
const srcStatus = $("srcStatus");
let current: Source | null = null;
let busy = false;

function showSource(extra: string[] = [], cls = "dim") {
  srcStatus.className = cls;
  srcStatus.replaceChildren(...extra.map((t, i) => { const d = document.createElement("div"); d.textContent = t; if (!i) d.style.fontWeight = "bold"; return d; }));
}
function setBusy(b: boolean) { busy = b; btnPlay.disabled = b || !current; btnPick.disabled = b; }
function selectSource(src: Source) {
  current = src;
  showSource([`Using: ${src.label}`, ...src.summary()], src.gameFiles().length ? "ok" : "err");
  btnPlay.textContent = "Play"; setBusy(false); btnPlay.focus();
}

btnPick.onclick = () => pick.click();
pick.onchange = () => {
  const files = Array.from(pick.files ?? []);
  if (!files.length) return;
  const src = pickedSource(files);
  if (!src.gameFiles().length) {
    showSource(["No game files found in that selection.", `Looked at ${files.length} files. Pick the folder that contains the extracted "files" folder (assets/raw) or the whole assets folder.`], "err");
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
wasmP.catch((e) => { note("wasm", "err", "The engine (web/pkg) is not built: run scripts/build-web.sh. " + e); });

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
        btnPlay.textContent = `Loading ${ok + fail}/${files.length}…`;
      }
    };
    await Promise.all(Array.from({ length: 6 }, worker));
    btnPlay.textContent = "Play";
    if (!ok) throw new Error("none of the game files could be read");
    note("files", fail ? "warn" : "ok", fail ? `${fail} of ${files.length} game files could not be read; the game may stall.` : null);
    msg.textContent = `${ok} game files loaded`;
    // main-CPU boot code (copyright + Konami logo screens): the disc's HEADER.BIN / IP.BIN; optional
    let boots = 0;
    for (const name of ["HEADER.BIN", "IP.BIN"]) { const b = await src.boot(name); if (b) { g.load_file(name, new Uint8Array(b)); boots++; } }
    note("boot", "warn", boots < 2 ? "header.bin / ip.bin not found: the copyright and Konami logo screens are skipped (optional)." : null);
    // saves → CONTINUE slots
    for (const n of SLOTS) { const b = store.bytes(n); if (b) g.set_save(n, b); }
    // audio
    const out = new AudioOut((p) => src.audio(p));
    let missingCount = 0;
    out.onMissing = (p) => { missingCount++; note("audiofiles", "warn", `${missingCount} audio file(s) not found or not decodable (last: ${p}); those sounds are silent.`); };
    for (const [, kind] of VOLS) out.setVolume(kind, volume[kind] / 100);
    out.setMuted(muted);
    const { pcm, fm } = await out.loadManifests();
    if (!pcm || !fm) note("manifests", "warn", `Audio manifests missing (${[!pcm && "audio/web/pcm.json", !fm && "audio/web/fm.json"].filter(Boolean).join(", ")}): ${!pcm && !fm ? "voices, FM music and effects are silent" : "part of the audio is silent"}. Run the audio pipeline (README).`);
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

    memory = wasm.memory; game = g; audio = out; input = new Input(canvas); input.game = g;
    (window as any).snatcher = { game, audio, input, store, notes, handleEvent };
    overlayOpen = false; overlay.hidden = true;
    note("wasm", "ok", null);
    canvas.focus();
    refreshSaves(); fit();
    requestAnimationFrame(frame);
  } catch (e) {
    showSource([`Could not start: ${(e as Error).message ?? e}`], "err");
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
  const p = isPaused();
  if (p !== wasPaused) { wasPaused = p; pacer.reset(); if (p) audio?.suspend(); else audio?.resume(); }
  if (p || crashed) return;
  const n = pacer.advance(now);
  if (!n) return;
  try {
    for (let i = 0; i < n; i++) {
      game.tick(input!.poll());
      for (const e of JSON.parse(game.take_events())) handleEvent(e);
    }
  } catch (e) {
    crashed = true; msg.textContent = "engine error: " + e; console.error(e);
    note("crash", "err", "The engine stopped with an error: " + e);
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
      if (store.volatile) note("storage", "warn", "Browser storage is blocked: saves last only until this page closes. Use Export saves.");
      msg.textContent = `saved to slot ${e.slot}`;
      break;
    }
  }
}

// ---------- go ----------
(async () => {
  fit();
  const r = await probeServed();
  if (r.src) selectSource(r.src);
  else { showSource(["No local game files found.", `${r.error ?? ""}. Choose the extracted folder below, or run the extraction and start bun web/server.ts (see README).`], "warn"); setBusy(false); btnPick.focus(); }
  msg.textContent = r.src ? "ready" : "waiting for game files";
  // headless/dev shortcuts skip the overlay (and its click): ?newgame, ?scene=SP01:0, ?autostart
  if (r.src && (params.has("scene") || params.has("newgame") || params.has("autostart"))) await start(r.src);
})();
