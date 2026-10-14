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
`Top::Title` runs the boot screens + DATA_D2 title module (engine/src/modules/title.rs, docs/re/modules.md). `boot()` → intro
DATA_D1/D0 intro and the DATA_Y16 staff roll play completely (the old "stall" was the display bit left off after the
dedication shutters, docs/re/modules.md §3.2; the oracle's missing CD time `$787A` §1). MODULES: done, see its Status.
Running now (sonnet subagents, they commit WIP themselves): MODULES, INTERACT, AUDIO-VERIFY.
1. **MODULES – DONE** (docs/re/modules.md): boot screens + DATA_D2 title/options/continue,
   intro D1/D0 and ending Y16 (staff roll) verified frame-by-frame vs the oracle (D1 70/80
   samples exact). No H40 needed (VDP reg 12 = 0 in all oracle frames). HYPOTHESIS: display
   re-enabled when no H-INT mode is active (the oracle stays black after the dedication).
   Remaining: Y16 tail (START at the end), gun calibration with a gun model, mono/stereo option in web audio.
2. **INTERACT – DONE (except shooting details)**: menu bar pixel-identical to the oracle,
   keyboard/keypad/pointer verified vs oracle (docs/re/interact.md). Remaining: steady held-pad
   input in the oracle, then compare ACT2/ACT3 shooting stages and results phases; gun calibration.
3. **AUDIO-VERIFY – DONE**: FM driver translation equal to the original write-for-write on
   all oracle logs (~3M YM writes, all 122 ids), drum key-ons equal; YM2612 LFO period fixed.
   Remaining: YM2612 output vs hardware (no chip reference), mix levels.
4. **FIDELITY** (running, docs/re/fidelity.md): engine vs oracle through Act 1; engine frames vs oracle (`re/oracle/probe.py`, `probe_mem.py`).
5. Ghidra DB: done once after resume (sub_prg: 406 functions, 303 named; main_ip: 180) via
   `re/analyze.sh` with the concatenated label files; re-run after new labels.

## Blockers
- No Sega CD BIOS (not downloaded by rule) → HLE oracle instead; YM2612/mix levels unverified by ear.
- REA cannot load raw 68000 images; Ghidra headless is used.
