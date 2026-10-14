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

## Resume here (order)
State after resume (commit after cfc038a): build is green (`cargo build --release`,
`cargo test --release -p snengine -p sncore --lib`, `scripts/build-web.sh`).
`Top::Title` is skipped (engine/src/modules/title.rs was never written). `boot()` → intro
DATA_D1 timeline: disclaimer, RSS logo and dedication screens render, then the screen stays black
(timeline stalls ~frame 1600; see `cargo run --release -p snengine --example run -- 4800 400 OUT "" boot`).
Running now (sonnet subagents, they commit WIP themselves): MODULES, INTERACT, AUDIO-VERIFY.
1. **MODULES** (engine/src/modules/, docs/re/modules.md — doc missing, create it): fix the D1/D0
   timeline stall, write the DATA_D2 title module and H40 (320 px) rendering; compare with
   `re/oracle/out/refs/*.png`.
2. **INTERACT – DONE (except shooting details)**: menu bar pixel-identical to the oracle,
   keyboard/keypad/pointer verified vs oracle (docs/re/interact.md). Remaining: steady held-pad
   input in the oracle, then compare ACT2/ACT3 shooting stages and results phases; gun calibration.
3. **AUDIO-VERIFY** (docs/re/audio.md; core/src/audio/verify.rs WIP): compare kdriver YM writes
   with the oracle trace `ym` field.
4. Fidelity loop: engine frames vs oracle (`re/oracle/probe.py`, `probe_mem.py`).
5. Ghidra DB: `re/analyze.sh` with `cat re/labels/sub_*.txt`.

## Blockers
- No Sega CD BIOS (not downloaded by rule) → HLE oracle instead; YM2612/mix levels unverified by ear.
- REA cannot load raw 68000 images; Ghidra headless is used.
