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
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw
scripts/convert-music.sh            # CD-DA → assets/web/music/*.ogg (Opus)
# 2. build the wasm engine + web front end
scripts/build-web.sh
# 3. serve locally (http://localhost:5300/)
bun web/server.ts
```
Debug: `?debug` URL parameter or F1 shows the engine state overlay.
Headless screenshot test: `cd web && bun test/shot.ts "<query>" out.png <frames> '<inputs-json>'`.

Controls: arrows = D-pad, Z/X/C (or A/S/D) = A/B/C, Enter = Start; gamepads via the Gamepad API.

## Documentation
- `docs/PROGRESS.md` – milestones, current state, next steps.
- `docs/RE_NOTES.md` and `docs/re/*.md` – reverse-engineering notes with evidence.
- `re/README.md` – disassembly/Ghidra workflow.
