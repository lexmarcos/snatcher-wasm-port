# Snatcher (USA, Sega CD) – native web port (private preservation project)

A reimplementation of the Snatcher engine in Rust, compiled to WebAssembly, that plays the
original game data from **your own disc image**. It is not an emulator: the script VM,
sequencer and scene composer are rewritten from the reverse-engineered 68000 code.

No game data is included in this repository. `assets/`, `work/`, `*.chd/*.bin/*.cue` are ignored.

## Requirements
`chdman`, Rust (with `wasm32-unknown-unknown` target and `wasm-bindgen-cli 0.2.129`),
`bun`, `ffmpeg`, Python 3 with `capstone` (RE only), Ghidra 12.1.4 (RE only).

## Build and run
```sh
# 1. extract your disc (once)
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
cargo build --release
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw   # assets/raw/{files,header.bin,ip.bin,filelist.tsv}

# 2. audio pipeline (optional but recommended; see docs/re/audio.md §5)
scripts/convert-music.sh                                          # CD-DA -> assets/web/music/trackNN.ogg
./target/release/snt audio-pcm assets/raw/files assets/audio      # voices, banks, drum kit, pcm.json
./target/release/snt audio-fm  assets/raw assets/audio            # FM songs + effects, fm.json (~1.5 min)
python3 -I re/py/audio_encode.py assets/audio                     # -> assets/audio/web/**.ogg + json

# 3. build the wasm engine + web front end (web/pkg, web/dist)
scripts/build-web.sh

# 4. serve locally (http://localhost:5300/, PORT=... to change)
bun web/server.ts
```
The page opens a start overlay. If the server finds `assets/raw` it offers **Play**; otherwise (or in addition)
**Choose extracted folder...** loads the files from a folder you pick in the browser (best: the whole `assets` folder;
it needs `raw/files`, optionally `raw/header.bin`, `raw/ip.bin`, `audio/web`, `web/music`). Nothing is uploaded,
and missing optional files only produce a note (sound off, boot logo screens skipped). Details: `docs/re/web.md`.

URL parameters: `?newgame` (skip title/intro), `?scene=SP01:0` (debug scene jump), `?autostart` (skip the overlay),
`?debug` (engine state overlay, also F1). Tests: `scripts/test-web.sh [outdir]` (unit tests + headless Chrome UI
checks + `web/test/shot.ts` screenshots; uses port 5301, or `PORT`).

Controls: arrows = D-pad, Z/X/C (or A/S/D) = A/B/C, Space = B, Enter = Start; Xbox, PlayStation and Switch gamepads via the Gamepad API
(per-family defaults, remappable in the menu; the game text shows the icon of the button of the last used device, see `docs/re/web.md` section 6).
Light gun (shooting sections): mouse to aim, left button = trigger, right button = second button; any pad key switches
back to the pad. Esc = menu (pauses; volume sliders, smooth scaling, save export/import, controls), F = fullscreen,
M = mute, F1 = debug. The game pauses while the tab is hidden. Saves live in `localStorage` (`snatcher.save.N`);
export/import them as JSON from the menu.

## Documentation
- `docs/PROGRESS.md` – milestones, current state, next steps.
- `docs/RE_NOTES.md` and `docs/re/*.md` – reverse-engineering notes with evidence.
- `docs/re/oracle.md`, `docs/re/oracle2.md` – the two dev-only test oracles (BIOS-free HLE; real Sega CD BIOS on Genesis Plus GX, needs your own BIOS image).
- `docs/re/web.md` – web front end (sources, audio graph, saves, tests, status).
- `re/README.md` – disassembly/Ghidra workflow.
