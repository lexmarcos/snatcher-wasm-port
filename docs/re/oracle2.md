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

## Verified against the HLE oracle (phase 2)
Method: `scenario2.py scenarios/intro_real.json --match ../oracle/out/refs` (event-aligned intro; every 3rd frame is compared with the HLE reference PNGs after
`normalize_levels`, which maps the core's colour levels 0..238 to the HLE renderer's `L*36`).
| reference | differing pixels (of 57344) | game frame in oracle2 |
|---|---|---|
| copyright, konami_laser, konami_logo, title, options, disclaimer, omni_building, omni_caption, act1, junker_hq_mika, junker_menu, mika_reply | **0** (all 12 identical to the HLE PNGs) | e.g. title 1346, options 1394, act1 3971, junker_hq_mika 4457 |
- Message box palette split (H-INT / IE1, M:0xFF0C4A, `scenarios/hint_check.json`): on the real BIOS the Mika dialogue panel is dark (black background, white text),
  VDP reg 0 at the end of the frame = 0x04 (IE1 off, the handler's last write), CRAM 0..3 = 000/1FF/03F/11F in the core's internal format (no 0x0EEE white). The 2026-10-15 bug fix is confirmed on real code.
- Differences found and their class:
  1. **Load time** (CD access, core model): boot to SUBCODE running (`$7840` = 6) HLE frame 25, real BIOS 234 (+209: BIOS boot-to-game load of FMWR_1, SUBCODE, PCM data with a 12-frame
     drive latency per read); title ready (`$7896` = 9) 1140 vs 1342. The copyright + Konami logo phase (`$7842` = 0) is *shorter* on the real BIOS (837 vs 967 frames, -13 %): CPU-bound
     work (LZ decompression) runs faster there; GPGX's s68k/m68k cycle tables vs Musashi's, not investigated further.
  2. **Real BIOS behaviour the HLE gets wrong**: after a game over / QUIT INVESTIGATION (VM state 4) the SP reloads SUBCODE and the game restarts at copyright -> Konami logo -> title
     (the HLE restarts as far as the logo, then stays black in the title phase, see "GAME OVER" below). The HLE also zeroes PRG-RAM; the real console keeps BIOS leftovers in `$9DC0..` (boot-screen text), harmless for the game flow tested.
  3. **Colour scale**: the core outputs the DAC levels 0,34,..,238 (pure white = #EEEEEE), the HLE renderer 0..252; not a game difference (normalised on compare).
  4. **Gun**: GPGX latches the HV counter at exactly the aimed pixel (`raw X = x`, `raw Y = y`, no jitter), the HLE model reports (x - 66, y + 1) with jitter; the game's calibration cancels
     the offsets, see "Shooting" below.
- Gotcha: a libretro option that is not set reads as NULL; `genesis_plus_gx_gun_input` NULL selects *touchscreen*. `build.sh` therefore generates `build/o2_defaults.h` from the core's own option table
  (declared defaults). Declared defaults include a 4 Mbit RAM cartridge; `Machine2` sets `cart_size = disabled`.

## Phase 3: open questions settled with oracle2 evidence
Tools (all in `re/oracle2/`, run from `re/oracle` with the venv; `ORACLE=2` switches the old fid/shoot tools): `fid_play.py`/`o2_replay.py` (adaptive play / replay of a recorded pad script), `o2_syncplay.py`
(replay synchronised to the game state: same inputs in game terms on both oracles), `o2_cmpq.py` (queue alignment of two traces), `o2_seqlog.py` (per sequencer command durations),
`o2_voice.py`, `o2_sndstall.py`, `o2_fmcap.py`, `shoot_gunscan.py` (`GUNCAL=00,E5` for the oracle2 calibration). Logs: `re/oracle/out/fid2/*.{story.txt,json,trace.jsonl}`, `out/replay_o2taxi/` (PNG).

### 1. GAME OVER (fidelity.md item 4, a2_taxi)
Run: `ORACLE=2 fid_session.py o2_a2_taxi SP01:250 36000` (jump = "TIKADO TO TAXI", SP01 node 0x24d; pad script `out/fid2/o2_a2_taxi.json`).
Evidence (oracle2 absolute frames; `out/replay_o2taxi/r23100..r24300.png`): the failed stage shows the GAME OVER picture (gold title background) with the menu CONTINUE / QUIT INVESTIGATION; after QUIT the queue
`07[FF] 10[56] 0D[30] 0A` runs (trace `seq_start` at rel 23285), the GAME OVER picture stays until the queue ends (rel 23944), then the screen goes black, `$9808` = 4 (VM state 4, S:0x1670C) and the SP
reloads SUBCODE (`$7840` 4 at +36 frames, 5 at +44, 6 at +124), `$7842` = 0 and the game **restarts**: Konami laser line, Konami logo, title with NEW GAME / CONTINUE (CD track 2 at abs frame 30532).
Verdict: **the engine is right to show the GAME OVER module; the HLE oracle is wrong after it**: it also reloads the SP (`$7840` 4 -> 6 at rel 23610..23623) and shows the Konami laser / logo, but then its title phase never advances (`$7896` stays 0, the sub sits in the Word RAM hand-back loop S:0x6138 with the main CPU requesting DMNA every ~3 frames, no CD-DA track 2) and the screen stays black; on the real BIOS `$7896` reaches 9 (title ready) 800 frames later and CD track 2 starts. Cause not found in the HLE (a Word RAM ownership livelock is the suspect; its "DMNA = immediate" rule is a HYPOTHESIS); not fixed, oracle2 is the reference for anything that restarts the game. The engine had a second defect found here: it stayed on the GAME OVER
picture forever (VM state 4 stopped the VM thread) - the original returns to the title. **Fixed** (`Engine::reset_to_title`, `VmPhase::ExitSp`, test `state4_restarts_at_the_title` in
`engine/tests/player_path.rs`, fails without the fix): queue [02] (black-out), `SP_RELOAD_FRAMES` = 90, soft reset keeping files/saves/audio manifests, then the title chain restarts.
(The 36-frame black-out and 88-frame SP reload are real-BIOS numbers; HLE cannot give them.)

### 2. Shooting: white flash, sound stalls, gun Y bias
- **Trigger flash (M:0xFF1A44)**: real BIOS, stage 0, blaster drawn (`$7970`/`$E057` = 1): the frame in which the trigger edge is seen is **100 % white** (all CRAM entries 0x0EEE, `fraction of white pixels = 1.0`), the next
  frame is normal again (0.02); without the second button first (blaster not drawn) no flash (0.01). So it is a full-screen one-frame flash for the gun's sensor, exactly as read from the code; the engine does not draw it
  (a mouse needs none). `re/oracle2` t18-style check: trigger at frame N -> white at N, normal N+1..N+5.
- **Stalls when a sound starts**: `o2_sndstall.py 0 0 2400 1` (stage 0, policy seed 1): handler runs per frame {1: 1981, 0: 416, 2: 3} on the real BIOS vs {1: 2025, 0: 368, 2: 7} on the HLE; handler-less frames in the 8 frames
  from a PCM effect start: **2.62 (oracle2) vs 2.68 (HLE)**, same patterns (`[1,0,1,0,0,1]` ...). The sample-copy stalls are real; the HLE model of them was right. Still unmodelled in the engine (lost presses while a sound plays).
- **Gun Y calibration bias (`GUN_CAL_BIAS_Y` = 31)**: still a HYPOTHESIS, but the evidence changed: with the real BIOS and the game's own calibration screen (`ORACLE=2 shoot_gunscan.py cal`) the bullseye (128,101) gives raw (0x80,0x65),
  calibration `$7988/9 = 00/E5`, value (0x80,0x80); aiming (100,80) gives value (0x64,0x6B) = (x, y+27), the crosshair object is drawn at (value-11, value-35) = on the aim point, and the cell scan
  (`GUNCAL=00,E5 shoot_gunscan.py cells`) switches rows at y = 21 / 61 (rows 0/1/2 for y <= 16 / 24..56 / >= 64), exactly like the HLE scan. GPGX latches the aimed pixel with no offset (raw = (x, y), the HLE model gave (x-66, y+1))
  and the algebra is the same, so the 31-line discrepancy is **not an artefact of the HLE gun model**: it comes from the game's calibration (value = y + 27) versus its cell thresholds. Whether a real Justifier reports a V counter
  about 31 lines different from the beam position cannot be decided by an emulator whose latch is idealised; it needs a real light gun. The engine's bias stays as is.

### 3. Timing drift: real-BIOS load times and voice start stall
Method: `o2_seqlog.py` (hook on seq_run S:0xFFC4: frames each sequencer command takes), `o2_voice.py` (hook S:0x154F8), `o2_syncplay.py`; CD events in `trace.jsonl` (`cd_read`, `cd_sectors`).
- **CD model of the core** (`cdd.c`): a read is a drive command + `latency = 2 + 10` CDD interrupts (75 Hz = 9.6 frames) + seek `distance * 120 / 270000` interrupts, 75 sectors/s. Measured (trace of an Act 1 session, 81 reads): command -> first sector
  12.0 frames for seeks < 10000 sectors, 29-30 frames for seeks of about 50000 sectors; 1.30 sectors/frame (nominal 75/s = 1.2517/frame). The core's own comment calls the seek time "a rough approximation" tuned for compatibility (Sonic CD, Switch...).
- **File loads (sequencer cmd 11)**, Act 1 session, 14 loads: HLE 3 frames each (engine: instant), real BIOS **12 + sectors/1.2517 + 7.4 frames** (e.g. DATA_H02 16 sectors 32 frames, DATA_H07 27 sectors 41, SP29 9 sectors 27, DATA_Q3 47 sectors 57);
  +20 frames when the head has to come back from far away (the first DATA_H01 load after a PCM read: 45/43 instead of 25).
- **Voice start**: request (`pcm_play_voice`) -> stream playing: **37.8 frames mean (21..61)** on the real BIOS vs 4.0 (3..7) on the HLE (engine: 2); playing time = manifest + 1.1 frames (HLE +0.9): the stream length is right,
  the start is the CD seek + prime. Handler-less frames in the 80 frames after a start: 5.25 vs 6.55 (the stalls come from the stream refills, not from the start).
- **Whole-session effect**: the same recorded Act 1 session (503 presses, `o2_syncplay.py`, identical game inputs on both oracles, 106 queues, all identical) takes **39771 frames on the real BIOS vs 36773 on the HLE (+8.2 %)**;
  engine vs HLE was -0.3..-0.7 %, so the engine is about 8 % faster than the real console model. Sources by size: voice starts (about 34 frames each, 37-42 per session), file loads (about 28 frames each, 14), boot (+209), the rest are CD-DA/shorter stalls.
- **Can the numbers be modelled?** The sector rate and the minimum latency are hardware-grounded (75 sectors/s; BIOS timeouts need ~10 interrupts); the seek part is a linear heuristic, real drives vary (0.2-1.5 s). The core is therefore
  trustworthy for relative speed (loads are 1x CD, not instant) but not for exact seek frames. Suggestion if the engine should feel like the console: `load_frames = 20 + sectors * 0.8` (+ 20 when the previous read was > 10000 sectors away) and
  `voice_start = 12 or 30 frames` (head near / far); expect the same game events, only later. Not done: it would encode a heuristic, and nothing checks game logic against it.

### 4. SP29 turbocycle chase
`o2_c_boso1` (SP01:868 = "Turbocycle boso zenhan", SP29, 40000 frames, 165 queues): all 165 queues of the engine equal the real-BIOS ones (engine vs oracle2 report; HLE: 183/184). Per iteration of the same pass the engine is faster than the
real BIOS by 36 frames per 502 (voice-heavy loop `0008 0002 4009`) and 140 per 1745; the loop's looping voice 0x3D (manifest 236.6 frames) lasts 237.5 (HLE) / **278.5 frames (oracle2: +41 = CD seek + prime before the stream plays)**. So the old
"18 frames per iteration" is not the main error: against the real BIOS the engine is about 41 frames per voice start (and about 7 % overall) too fast; against the HLE it was 0.4 %. Same cause as 3 (voice-start CD seek).

### 5. FM timbre (`o2_fmcap.py`, Nuked YM2612 ym2612 mode)
Original driver run from the idle title screen (F2 first: the music stops, silence measured = exactly 0), id injected like `audio_inject.py`, 44.1 kHz stereo WAV in `re/oracle2/out/fm/o2_<id>.wav`, compared with `assets/audio/fm/*.wav` (engine renders):
| id | peak eng/o2 dBFS | RMS eng/o2 | envelope corr | note |
|---|---|---|---|---|
| song 3F | -7.0 / -6.1 | -23.5 / -23.3 | 0.96 | tonal bands within 1 dB, o2 +10..20 dB above 3 kHz |
| song 45 | -18.6 / -16.6 | -33.4 / -31.7 | 0.99 | |
| song 7A | -18.7 / -18.3 | -29.2 / -27.8 | 0.95 | |
| song 6F (noise ambience) | -23.4 / -22.7 | -34.8 / -33.6 | 0.999 | o2 +25 dB noise floor above 760 Hz |
| songs 4F, 40, 50 (23-30 s, drums) | -8.2/-6.9, -7.2/-4.3, -15.4/-8.2 | -25.9/-24.5, -22.6/-18.9, -26.3/-27.4 | 0.44, 0.50, 0.36 | lag of first/last 5 s <= 20 ms (no tempo drift); correlation falls with length (PCM drum phase, noise patches) |
| sfx 01, 0B, 0A, 20, 1D | -12.7/-14.1, -12.0/-11.0, -12.1/-12.4, -16.9/-15.5, -19.6/-18.7 | -28.8/-30.0, ... | 0.94-0.998 | band means within 0.1..1.4 dB (1D: +5 dB) |
| sfx 33, 02 | -18.7/-18.5, -26.3/-24.9 | -29.1/-28.1 | 0.95, 0.66 | 33: o2 +10 dB above 2.4 kHz |
Differences: (1) levels agree within about 2 dB peak / 1.7 dB RMS for 12 of 14 ids (the engine's "unity" mixing is close to the core's), (2) tonal content and envelopes agree, so driver + YM translation are right, (3) the real chip model adds a
**broadband noise floor / DAC ladder distortion** (high bands 10-30 dB above the engine in quiet or noisy sounds) which the engine's chip model (no ladder effect, documented) does not have, (4) song 40 has +10 dB below 100 Hz
(PCM drum kit at 60 Hz band: +13 dB; engine drum level vs the real RF5C164 path, unverified). Nothing was retuned: (3) is a property of the real chip that a web port arguably should not copy.

### 6. Act 1 side by side (HLE vs oracle2) and the engine
Session: `fid_play.py --cycle --rng --frames 40000` from the Junker HQ anchor on each oracle (`act1_hle.*`, `act1_2.*` in `re/oracle/out/fid2`). The two adaptive runs diverge once (policy: a menu key lost to a sub-CPU stall
picks another ASK topic, queue 89), so the *same* game-level inputs were replayed on both: `o2_syncplay.py` (presses synchronised to menu/message readiness with the recorded `since`, lost presses retried like `fid_play`).
- **Queues: 106 of 106 identical** (`o2_cmpq.py out/fid2/sync_act1_hle.trace.jsonl out/fid2/sync_act1_2.trace.jsonl`), over 36773 (HLE) / 39771 (oracle2) frames. No behavioural divergence of the script/sequencer logic.
- **Screens** 60 frames after each queue start (`qshots_*`, normalised colours): 48 identical, 31 within 60 px, 27 with more; the 27 are timing phase, checked by eye: text typed one line less (q62, q66), fade-in phase (q21), scene still loading
  on the real BIOS (q19, q20: black where the HLE already shows the scene; the load is 20-30 frames longer), a few sprite phases. No content difference.
- **Engine vs both**: `fid_cmp --sync` Act 1: engine matches 108/108 (HLE) and 106/106 (oracle2) queues, the same 3 engine-only passes before the anchor in both (new-game prelude). Screens at drift-corrected frames: engine vs HLE 30 exact / 23 <= 60 px / 27 more,
  engine vs oracle2 14 / 26 / 40 (the real BIOS drift is uneven - loads and voice starts - so the +-12-frame refine does not always find the phase).
- A divergence "oracle2 differs from the HLE and the engine follows the HLE" therefore exists only in **timing** (section 3), not in logic. No engine bug of that kind was found in Act 1, SP29 (165/165 queues) or a2_taxi (74 of 75, the 75th = the state-4 queue [02] that is fixed now).
- One more difference seen only on the real console path: the game also depends on uninitialised PRG-RAM (`$9DC0..` holds BIOS boot-screen text on the real BIOS, zero in the HLE). Zeroing it (`O2_ZERO_VARS=1` in `fid_common.oracle_jump`) changed nothing in the sessions tested.

## Limits of oracle2
- The core is an emulator, not hardware: CD seek/latency is a heuristic (section 3), the Justifier latch is idealised, no real-time effects of the physical drive, no region/BIOS variants run (only Model 1 v1.10; the Model 2 v2.00w image was not needed).
- PC hooks step the CPU per instruction (identical state, same speed here); they exist for the sub CPU (and main CPU) only, no memory-write watches (use per-frame polling or hooks on the writing instruction).
- `seq_start` comes from hooks on S:0xFFC4 and the `clr.b seq_busy` sites, equal to the HLE's rising-edge events in all sessions compared (106/106), but it is not the same code path.
- Not produced: `cd_dest`, `buram`, `cdbios`, `pcm_on` events; trace `f` of the HLE is the frame index before the step (the same convention is kept: events carry the frame in which they were logged).
- BIOS and disc images are never copied (the BIOS is a symlink in a gitignored directory).
