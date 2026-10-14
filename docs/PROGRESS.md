# Progress

## How to run
See `README.md` for the full command list. Short version:
```
cargo build --release                                  # tools (snt)
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw
scripts/convert-music.sh                               # CD-DA → Opus
scripts/build-web.sh && bun web/server.ts              # http://localhost:5300/
re/disall.sh                                           # regenerate 68000 listings (re/out/dis)
```
Ports: 5300 = web port dev server; 5310 = asset viewer (web/viewer).

## Milestones
- **M0 extraction – DONE.** `snt extract` checks MODE1 sync/mode of every sector, carves
  IP/SP from the header fields (sizes match: 0x6000 / 0x1800), parses ISO9660 (both-endian
  root record check, all 100 extents inside track 1), dumps files and 20 CD-DA tracks.
- **M1 code analysis – IN PROGRESS.** Sub program layout found (SP @0x6000, SUBCODE @0xD400),
  script VM dispatch located, main CPU = vblank-driven slave. Tooling: Ghidra headless
  (`re/analyze.sh`) + capstone recursive disassembler (`re/dis68k.py`) driven by label files.
- M2 LZKN1 – workstream GRAPHICS (running).
- M3 asset pipeline/viewer – workstreams GRAPHICS + AUDIO (running).
- M4 script VM – workstream SCRIPT (running); ACTIONS (sequencer/text/menus) running.
- **M5 engine – scaffold DONE**: `engine/` crate builds for wasm32, `web/` front end loads the
  wasm, all game files from `assets/raw/files`, renders the framebuffer with integer scaling,
  keyboard/gamepad input, WebAudio music/voice/sfx player, localStorage saves, debug overlay,
  puppeteer screenshot test (`web/test/shot.ts`). Game logic not yet implemented.
- ORACLE (dev-only HLE emulator of the original code, no BIOS) – workstream running.

## Workstreams (parallel agents) and file ownership
| workstream | notes | code |
|---|---|---|
| GRAPHICS | docs/re/graphics.md | core/src/{lzkn,gfx}.rs, tools/src/cmds/gfx.rs, web/viewer |
| SCRIPT | docs/re/script.md | core/src/script.rs, tools/src/cmds/script.rs |
| ACTIONS | docs/re/actions.md | core/src/actions.rs, tools/src/cmds/actions.rs |
| AUDIO | docs/re/audio.md | core/src/audio.rs, tools/src/cmds/audio.rs |
| ORACLE | docs/re/oracle.md | re/oracle/ |

## Blockers
- No Sega CD BIOS (by rule, not downloaded). Mitigation: HLE oracle (BIOS calls stubbed).
- REA `open_binary` cannot load raw 68000 images; Ghidra is used headless instead.
