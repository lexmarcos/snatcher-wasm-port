# Progress

## How to run
See `README.md`. Short version:
```
cargo build --release
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw
scripts/convert-music.sh                                   # CD-DA → Opus
./target/release/snt audio-pcm assets/raw/files assets/audio
./target/release/snt audio-fm assets/raw assets/audio && python3 -I re/py/audio_encode.py assets/audio
scripts/build-web.sh && bun web/server.ts                  # http://localhost:5300/ (?newgame, ?scene=SP01:0, ?debug)
cargo run --release -p snengine --example run -- <frames> <every> <outdir> [auto|f:bits,...] [SPnn:pc]   # native, PNG + sheet
re/oracle: .venv/bin/python -I scenario.py scenarios/intro.json   # original code, reference PNGs (docs/re/oracle.md)
```
Ports: 5300 web port, 5310 asset viewer (`bun web/viewer/server.ts 5310`).

## Milestones (state matches the git history)
- **M0 extraction – DONE.** IP/SP carved (sizes match header), 97 ISO files + 3 txt, 20 CD-DA tracks.
- **M1 code analysis – DONE (ongoing labels).** SP @S:0x6000, SUBCODE @S:0xD400, main CPU is a
  VBlank-driven display slave; label files `re/labels/*.txt`; Ghidra headless + capstone listings.
- **M2 LZKN1 – DONE.** Same as Contra Hard Corps variant; Rust decoder byte-identical on 1440/1440 blobs.
- **M3 assets – DONE.** Scenes/palettes/font decoded and rendered (viewer :5310); voices (1162),
  PCM banks/drums/sfx, 122 FM sounds pre-rendered from a Rust translation of the 68k FM driver.
- **M4 script VM – DONE.** 75 opcodes, disassembler consumes 100% of 39 SP files, reference VM.
- **ORACLE – DONE.** BIOS-free HLE emulator (Musashi) runs the original from power-on to Junker HQ.
- **M5 engine – IN PROGRESS.** New game plays the opening (Omni Building caption, ACT 1 card,
  Junker HQ, Mika dialogue, command menu) matching oracle screenshots; scene timing within ~3%.
  Web front end: canvas, input, WebAudio (CD/FM/voice/PCM), saves in localStorage.
  Missing: title/intro modules, menu highlight bar, pointer/keypad menus, shooting, audio check.

## Resume here
All workstreams finished (2026-10-15). Build, tests (sncore 33, snengine 21), `scripts/build-web.sh`
and `scripts/test-web.sh` (0 console errors) are green. Fidelity vs the original (oracle):
Act 1 158/159, Acts 2–3 + endings 2984/2986 action queues identical; staff roll pixel-identical;
CONTINUE round trip equal; all 15 shooting stages frame-exact with pad; FM driver write-exact.
User bug fixes (2026-10-15): white message panel without text in portrait scenes (oracle
artefact copied into the renderer – fixed in both) and intro CD-DA leaking into the game (CD now
stops on every disc load like cmd 11's cd_pause). Regression tests from the title screen:
`cargo test --release -p snengine --test player_path` (fail without the fixes).
Open items (each with evidence in the named doc), in suggested order:
1. Timing drift 0.3–0.7% (scene/sub-CPU load time and the 3–7 frame voice-start stall not modelled) – docs/re/fidelity.md.
2. Turbocycle chase loop (SP29) ~18 frames/iteration fast; GAME OVER after a failed shooting stage
   shows the module while the oracle stays black (a2_taxi) – docs/re/fidelity.md.
3. Shooting: trigger white flash (M:0xFF1A44), sound-start stalls, gun Y bias HYPOTHESIS – docs/re/shooting.md.
4. Front end: touch controls, voice pan / music ducking – docs/re/web.md, docs/re/audio.md.
5. YM2612 output vs real hardware (no chip reference available) – docs/re/audio.md.

## Controllers (Xbox / PlayStation / Switch, button icons) – status 2026-10-15
Done and tested (details, tables, checklist: `docs/re/web.md` section 6): family detection (30 real id strings), standard + raw (Firefox-style) layouts,
radial dead zone, last-used-device tracking with hot-plug, per-family default maps and a remap screen (`snatcher.padmaps` in localStorage), View/Share/-
opens the web menu, 32 icon glyphs (codes 0x86-0xA5, `core/src/prompt.rs`) printed for `{EC}/{EE}/{F0}` and the word START (SP09), `set_prompt` wasm API,
help panel with SVG icons. Engine default stays `Style::Genesis` (original letters): fidelity/regression output unchanged.
Tests: `bun test web/test` (gamepads), `bun web/test/pads.ts` (stubbed pads in headless Chrome, screenshots in `work/shots`), `cargo test -p snengine --test prompts`,
`core/src/prompt.rs` unit tests; all in `scripts/test-web.sh` / cargo.
Next steps: (1) verify with real pads (checklist in web.md 6.7), especially the Firefox/Linux raw tables for Switch Pro (HYPOTHESIS);
(2) rumble needs an engine event for shot/player hit (web.md 6.6); (3) icons for the title "PUSH START" and the options pad-configuration screen
(baked into graphics); (4) optional: gamepad navigation of the web menu.

## Blockers
- No Sega CD BIOS (not downloaded by rule) → HLE oracle instead; YM2612/mix levels unverified by ear.
- REA cannot load raw 68000 images; Ghidra headless is used.
