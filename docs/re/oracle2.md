# oracle2: the original game on the REAL Sega CD BIOS (dev-only test oracle)

Second oracle next to the BIOS-free HLE oracle (`docs/re/oracle.md`). The unmodified **Genesis Plus GX** core (libretro tree) runs the original
disc image with a real BIOS, driven frame by frame from Python. It removes the whole class of HLE modelling errors (the H-INT/IE1 bug of
2026-10-15 was one). Never shipped: the core's license forbids commercial use, it is not linked into the web port; BIOS images and game data
are never copied or committed (`Sega Mega CD BIOS/` and `re/oracle2/third_party|build|sys|states|out` are gitignored; `sys/bios_CD_U.bin` is a symlink).

## What is used
- Core: github.com/libretro/Genesis-Plus-GX, commit `393f70cd64c7334564c19a424c383f53986c13f4` (2026-10-09), cloned to `re/oracle2/third_party/Genesis-Plus-GX`
  (gitignored), built by `re/oracle2/build.sh` with the core's own Makefile (`Makefile.libretro platform=unix`), no core file is edited.
- BIOS: `Sega CD (U) - Model 1 v1.10 (1992).bin`, 131072 bytes, **SHA-1 f4f315adcef9b8feb0364c21ab7f0eaf5457f3ed**. This is the hash listed for
  `bios_CD_U.bin` (US Sega CD, model 1 v1.10) in the Genesis Plus GX / libretro documentation (from memory of the docs, no network check was made).
  Cross-check candidates in the folder: `Model 2 v2.00w` (SHA-1 5adb6c3af218c60868e6b723ec47e36bbdf5e6f0) and `[BIOS] Sega CD (USA) (v1.00)` (c5c24e6439a148b7f4c7ea269d09b7a23fe25075).
  The core wants it as `<system dir>/bios_CD_U.bin` (`libretro.c` `CD_BIOS_US`); the region comes from the disc's security code, here US.
- YM2612: option `genesis_plus_gx_ym2612 = nuked (ym2612)` (cycle-accurate Nuked OPN2, YM2612 variant), the most accurate mode of the core.
- Other options set by `Machine2`: CD access time `enabled` (core default, models seek/latency approximately), no RAM cartridge
  (`genesis_plus_gx_cart_size = disabled`; with the default cart the game asks "CD Backup RAM Cart SET. Which will you use?"), audio filter off.
  Internal backup RAM lives in a fresh temp dir per run (otherwise a leftover `scd_U.brm` would break determinism).
- The core detects `SNATCHER` in the header and forces a **Konami Justifier in port 2** (`core/loadrom.c`), so the game finds a gun at power-up (`A12013 = 1`).

## Files (`re/oracle2/`)
| file | purpose |
|---|---|
| `build.sh` | builds `build/liboracle2.so` = core objects + `o2.c` (links with `--wrap=cdd_process --wrap=cdd_read_data`) |
| `o2.c` | headless libretro frontend (callbacks) + exported accessors: main RAM, sub PRG-RAM, Word RAM (2M/1M), VRAM/CRAM/VSRAM/VDP regs, PCM RAM, gate-array regs, CPU regs/PC, YM2612 write log (installed over the `fm_write` pointer), CD drive events (CDD commands, data sectors), serialize, audio, RGB frame |
| `o2lib.py` | `Machine2`: the HLE `Machine` interface (`step_frame`, `inputs`, `sub/main.r8/r16/r32/write/w8/w16`, `vdp.frame_image`, `save_state/load_state`, `m.gun`) and the `trace.jsonl` writer; `boot()` presses START on the BIOS screen and returns t0 |
| `o2compat.py` + `re/oracle/oracle_sel.py` | `ORACLE=2` makes the existing `fid_*`, `shoot_*`, `interact_jump`, `scenario` tools use oracle2 (`.pkl` state names map to `re/oracle2/states/NAME.state`) |
| `scenario2.py` | scenario runner for the existing `scenarios/*.json` plus extensions (`until`, inline `shot`/`save`/`dump`) |
| `mk_anchor.py` | event-driven reach of the Junker HQ command menu; writes `states/junker_hq.state` (anchor of the fid tools) |
| `o2_timeline.py`, `o2_sheet.py` | game-event timeline of one/two traces; contact sheets |
| `scenarios/intro_real.json`, `hint_check.json` | event-aligned variants of `intro.json` |

Run: `re/oracle/.venv/bin/python -I re/oracle2/scenario2.py re/oracle2/scenarios/intro_real.json --out re/oracle2/out/x`; tools: `ORACLE=2 .venv/bin/python -I fid_play.py --cycle ...` from `re/oracle`.
Build once: `git clone https://github.com/libretro/Genesis-Plus-GX re/oracle2/third_party/Genesis-Plus-GX && (cd ...; git checkout 393f70cd)`, `ln -s "../../../Sega Mega CD BIOS/Sega CD (U) - Model 1 v1.10 (1992).bin" re/oracle2/sys/bios_CD_U.bin`, `re/oracle2/build.sh`.

## Trace format (compatible with `docs/re/oracle.md`)
Frame record `{"t":"frame","f","sp_state","state","vm_pc","script","comcmd":[8],"comstat":[8],"flags":[main,sub],"main_pc","sub_pc","ym":[[part,reg,val]..]}`
identical in meaning (read from PRG-RAM and the gate array registers every frame). `f` counts from power-on of the BIOS; the event `{"t":"boot","f":t0}` marks the first frame in which the game's IP runs
(compare HLE frame N with t0 + N). Events: `script_file`, `seq_start` (**polled once per frame**, the HLE logs the exact instant), `cd_read` (fn `CDD_PLAY`/`CDD_SEEK`: the drive command with the
target LBA, `count` -1, `file` null, `from_lba` = drive position), `cd_sectors` (`lba`, `count`, `f`..`f_end`: data sectors the CD controller decoded), `cdd` (other drive commands), `music`
(`MSCPLAY`/`MSCSTOP` derived from the drive state: CD-DA track playing), `shot`, `save`, `until_done`.
Fields that cannot be produced: `cd_dest`, `buram`, `cdbios`, `unmapped`, `cpu_exception` (no BIOS hooks in the real BIOS path), `pcm_on` (could be added from PCM registers), per-instruction PC hooks
(`core_add_hook` of the HLE; the real-BIOS tools use `$7848`, the per-run counter of the sub frame handler S:0x6102, instead).
Colours: the core outputs the VDP levels 0,34,68,...,238 (3-bit DAC), the HLE renderer 0,36,73,...,255, so pure white is #EEEEEE here; compare quantised levels, not RGB.

## Speed and determinism
- ~730-790 emulated frames/s for the bare core with Nuked YM2612 on this machine, ~500 frames/s with trace + audio capture (12 core PC).
- Determinism: the same scenario run twice gives byte-identical `trace.jsonl`, `a.wav` and PNGs (checked on `scenarios/intro_real.json`, 2500 frames).
  Save states are the core's serialize blob; load + continue is exact (menu ready at the same frame as in the original run).
