// Snatcher web port – browser front end.
// The engine (Rust → wasm) owns all game logic and renders a 256x224 RGBA frame (H32 mode).
// This file handles: asset loading, the 60 Hz loop, canvas scaling, input, audio and saves.
import { AudioOut } from "./audio";
import { Input } from "./input";

const W = 256, H = 224;
const params = new URLSearchParams(location.search);
const msg = document.getElementById("msg")!;
const canvas = document.getElementById("screen") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;
const debugEl = document.getElementById("debug")!;
const smoothBox = document.getElementById("smooth") as HTMLInputElement;
const dbgBox = document.getElementById("dbg") as HTMLInputElement;

// ---------- canvas scaling ----------
function fit() {
  const s = Math.max(1, Math.floor(Math.min((innerWidth - 16) / W, (innerHeight - 60) / H)));
  canvas.style.width = `${W * s}px`;
  canvas.style.height = `${H * s}px`;
}
addEventListener("resize", fit);
fit();
try { smoothBox.checked = localStorage.getItem("snatcher.smooth") === "1"; } catch {}
const applySmooth = () => { canvas.classList.toggle("smooth", smoothBox.checked); try { localStorage.setItem("snatcher.smooth", smoothBox.checked ? "1" : "0"); } catch {} };
smoothBox.onchange = applySmooth; applySmooth();
dbgBox.checked = params.has("debug");
const applyDbg = () => { debugEl.style.display = dbgBox.checked ? "block" : "none"; };
dbgBox.onchange = applyDbg; applyDbg();
addEventListener("keydown", (e) => { if (e.key === "F1") { dbgBox.checked = !dbgBox.checked; applyDbg(); e.preventDefault(); } });

// ---------- game files ----------
// The list of disc files the engine needs. They come from the user's own extraction
// (served from /assets/raw/files by the local dev server) or from a folder the user picks.
const FILES_URL = "/assets/raw/files/";
async function fileList(): Promise<string[]> {
  const r = await fetch("/assets/raw/filelist.tsv");
  if (!r.ok) throw new Error("assets/raw/filelist.tsv not found – run the extraction (see README)");
  return (await r.text()).split("\n").slice(1).map((l) => l.split("\t")[0]).filter((n) => n && n !== "PCMLD_01.BIN");
}

async function main() {
  const pkg: any = await import(/* @vite-ignore */ "/pkg/snengine.js" as string);
  const wasm = await pkg.default();
  const game = new pkg.Game();
  const memory: WebAssembly.Memory = wasm.memory;

  const pick = document.getElementById("pick") as HTMLInputElement;
  pick.onchange = async () => {
    for (const f of Array.from(pick.files ?? [])) game.load_file(f.name, new Uint8Array(await f.arrayBuffer()));
    msg.textContent = `loaded ${pick.files?.length} picked files`;
  };

  try {
    const names = await fileList();
    let n = 0;
    await Promise.all(names.map(async (name) => {
      const r = await fetch(FILES_URL + name);
      if (r.ok) { game.load_file(name, new Uint8Array(await r.arrayBuffer())); n++; }
    }));
    msg.textContent = `${n} game files loaded`;
  } catch (e) {
    msg.textContent = String(e) + " – or pick the extracted files folder";
  }
  // main-CPU boot code (copyright + Konami logo screens): the disc's HEADER.BIN / IP.BIN; optional (the engine
  // skips those two screens when they are missing)
  for (const [url, name] of [["/assets/raw/header.bin", "HEADER.BIN"], ["/assets/raw/ip.bin", "IP.BIN"]]) {
    try { const r = await fetch(url); if (r.ok) game.load_file(name, new Uint8Array(await r.arrayBuffer())); } catch {}
  }
  // saves (localStorage, base64 of the 0x1C0-byte save block) → CONTINUE slots 1..4
  for (let slot = 1; slot <= 4; slot++) {
    let d: string | null = null;
    try { d = localStorage.getItem(`snatcher.save.${slot}`); } catch {}
    if (d) game.set_save(slot, Uint8Array.from(atob(d), (c) => c.charCodeAt(0)));
  }
  if (params.has("scene")) game.debug_jump(params.get("scene")!);
  else if (params.has("newgame")) game.new_game();
  else game.boot();

  const audio = new AudioOut("/assets/");
  const { pcm, fm } = await audio.loadManifests();
  if (pcm?.voices) for (const [id, v] of Object.entries<any>(pcm.voices)) game.set_voice_len(Number(id), Math.ceil(v.seconds * 60));
  if (fm?.sounds) for (const [id, v] of Object.entries<any>(fm.sounds)) game.set_fm_len(Number(id), Math.ceil(v.seconds * 60), v.loop_start != null);
  if (!pcm || !fm) console.warn("audio manifests missing – run the audio pipeline (README)");
  const input = new Input(canvas);
  const img = ctx.createImageData(W, H);
  let last = performance.now(), acc = 0;
  const STEP = 1000 / 60;

  function frame(now: number) {
    acc += Math.min(now - last, 250); last = now;
    while (acc >= STEP) {
      game.tick(input.poll());
      acc -= STEP;
      const ev = JSON.parse(game.take_events());
      for (const e of ev) handleEvent(e);
    }
    const ptr = game.framebuffer_ptr();
    img.data.set(new Uint8Array(memory.buffer, ptr, W * H * 4));
    ctx.putImageData(img, 0, 0);
    if (dbgBox.checked) debugEl.textContent = game.debug_info();
    requestAnimationFrame(frame);
  }
  function handleEvent(e: any) {
    switch (e.t) {
      case "music": audio.cd(e.track); break;
      case "fm": audio.fmCmd(e.id, e.music); break;
      case "voice": audio.voice(e.id); break;
      case "pcmsfx": audio.pcmSfx(e.id, e.bank); break;
      case "log": console.log("[engine]", e.msg); break;
      case "save": try { localStorage.setItem(`snatcher.save.${e.slot}`, e.data); } catch {} break;
      case "load": { let d: string | null = null; try { d = localStorage.getItem(`snatcher.save.${e.slot}`); } catch {} game.save_loaded?.(e.slot, d ?? ""); break; }
    }
  }
  (window as any).snatcher = { game, audio, input };
  requestAnimationFrame(frame);
}

main().catch((e) => { msg.textContent = "error: " + e; console.error(e); });
