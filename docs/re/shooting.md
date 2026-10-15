# SHOOTING fidelity – engine vs ORIGINAL (oracle) for every shooting stage, light gun and calibration

Code: `engine/src/shoot.rs` (port of `shoot_tick` S:0x1208E–0x1314E, seq cmd 0x18; cmd 0x1F tally S:0x13150), gun registers / raw model in
`engine/src/menus.rs` (`frame_hook`, `gun_raw`, `gun_default_calib`), calibration screen in `engine/src/modules/title.rs`, web mouse in
`web/src/input.ts`. Background and recipes: `docs/re/interact.md`, `docs/re/actions.md` §10.

## Status

- **Pad play, all 15 stages ($78E0 0..0x0E, incl. the practice range 0x0E) is frame-exact against the original** (oracle-driven random pad
  policy, 3000 frames, 2 seeds, 30 runs, `re/oracle/shoot_batch.py`): phases `$78E8/$78EA`, spawner `$78FC/$7944/$7906`, target states, enemy
  attacks `$78FE`, life `$7910`, hits/misses/kills/civilians `$790C/$790E/$790A/$7914`, shot machine `$78FA`, speed `$78E2`, sounds (function and
  id sequence) and the object fields are identical; the final counters (shots, hits, miss, kills, civilians, life, exit) are equal in every run.
  The only differences are single values shifted by 1-2 frames where a PCM sample copy stalls the sub CPU inside a tick (see "Sub-CPU stalls")
  and the length of the results wait (phase 4, a few frames, see "PCM busy").
- **Light-gun play is exact too**: with the oracle's gun registers (`$7985/$798A/$798B` of each handler run) forced into the engine a 1800-frame
  gun session of stage 0 is identical (86 shots, 8 hits, 62 misses, 16 kills, all state words). Without forcing, the engine's own mapping
  mouse pixel -> `$798A/$798B` agrees with the oracle's: X within +-4 (the oracle's HV-counter model is 2 px / 6-cycle granular), Y within 1.
- Oracle bug fixed: held buttons arrived every other frame (see "Oracle pad model").
- Oracle: Justifier (port 2, device ID 1, TH interrupt, HV counter) is emulated, see "Light gun".
- Tests: `cargo test --release -p snengine --lib` (shoot::tests: gun cell thresholds, hit/miss/innocent/trap resolution, life meter, target timers,
  results phases, PCM channel busy; menus::tests: raw gun values, default calibration, calibration on the bullseye).

## Harness (re/oracle, engine/examples)

```
cd re/oracle
.venv/bin/python -I shoot_batch.py --seeds 1,2 --frames 3000          # all 15 stages: oracle run + engine replay + compare (about 40 s per run)
.venv/bin/python -I shoot_batch.py --stages 0:2 --seeds 1 --objs      # one stage, object fields too
.venv/bin/python -I shoot_rec.py --group 0 --item 0 --gun 1 --frames 1800 --out out/shoot/gun.o.jsonl   # light-gun session
GUNFORCE=1 GUNS=@out/shoot/gun.o.jsonl.gun.json SCHED=out/shoot/gun.o.jsonl.sched SFXLEN=out/shoot/sfxlen.txt \
  ../../target/release/examples/shoot_trace 0 0 "" 1800 out/shoot/gun.e.jsonl   # engine replay (without GUNFORCE the engine maps the pixels itself)
.venv/bin/python -I shoot_cmp.py out/shoot/gun.o.jsonl out/shoot/gun.e.jsonl
```
Stage = `group:item` of the SP01 debug menu (SHOOTING -> ACT1/2/3 SHOOTING -> list): 0:0 -> `$78E0` 0, 0:1 -> 0B, 0:2 -> 1, 0:3 -> 2, 0:4 -> 3,
0:5 -> 4, 1:0 -> 5, 1:1 -> 6, 1:2 -> 8, 1:3 -> 7, 1:4 -> 9, 1:5 -> 0A, 2:0 -> 0C, 2:1 -> 0D, 2:2 -> 0E (the lists wrap, the 4th item repeats).
- `shoot_common.py` drives the ORIGINAL through the menu (3-frame presses, 25-frame gaps; a 1-frame press is sometimes lost by the VM thread) and
  `cur_cmd` reads the sequencer command (queue S:$9940, index $9B40).
- `shoot_rec.py` records per frame the shooting words `$78E0..$794E`, `$1511E`, `$7970/72`, gun/pad bytes and 24 object slots (active, flags,
  x, y, idx, +0x17, +0x24, +0x2C) plus the sound calls (`subfn_06` S:0x1530E, `subfn_12` S:0x15EB4) and the sequencer command. Frame 0 = the first
  frame in which setup step 1 completed (`$78EA >= 2`). `--auto SEED` is an oracle-driven pad policy (draw, aim at live targets or empty cells,
  fire, idle so that enemies attack), `--gun SEED` the same with the gun (cell centres +-12/8 px).
- `engine/examples/shoot_trace.rs` navigates the SP01 menu with feedback (cursor position `TextMenu::cursor`) and writes the same format.
- `shoot_cmp.py` compares word by word and object by object; differences count only when they persist >= 4 frames (`--min-run`), prints sound
  sequence, phase timeline, sequencer commands and the final counters.

### Determinism and sub-CPU schedule (what makes a frame-exact comparison possible)
- The original steps its RNG `$786A..D` once per frame-handler run **before the sequencer** (S:0xD644 `jsr sub_00FD96`); the engine did not
  (its first `rand()` of a tick returned a different value than the original's). Engine: `Engine::tick` calls `disp.sub.rand()` there now.
- RNG pinning for comparisons: the oracle hook at S:0xD644 writes `$786A..D = n * 2654435761` (n = handler runs since frame 0), the engine does
  the same with `Ix::rng_seq`. (A constant state per frame makes the spawner pick the same - maybe unusable - cell forever.)
- Sub-CPU stalls: a PCM sample copy (`subfn_06` S:0x1530E -> loop S:0x15C32, 16-bit stores of up to 4096 bytes) takes 1-2 frames of sub CPU, so a
  frame has 0, 1 or 2 handler runs and a run may see a pad sampled in an earlier frame. The oracle records `h` = runs per frame and for each run
  the logical pad bytes `$7990/$7991/$7992` (+ gun `$7985/$798A/$798B`); the engine replays that schedule (`SCHED`, `Ix::force_in`) so the logic
  can be compared exactly. This schedule is a harness feature, the engine itself does not stall on sound.
  Consequence for the original: an edge (press) that happens in a frame without handler run is lost (the main CPU recomputes the edge every
  frame, S:0xFF1298), so presses are sometimes eaten while sounds play.

## Oracle pad model (the "held buttons every other frame" bug)
`re/oracle/machine.py step_frame` consumed a frame for **zero-length input entries**: scripts built as `[[f-t-1,0],[1,bits],..]`
(`fid_watch.py`, `fid_probe.py`, `fid_cmp.py`) produce `[0,0]` for consecutive presses and each `[0,0]` ate one frame with pad = 0, so a button
held over N consecutive script frames arrived on every other frame (`$7990` alternated 0x0202/0x0000 in `fid_watch --inputs '[[10,2],[11,2],[12,2],[13,2]]'`).
The pad read itself (`_pad_read` vs M:0xFF1298: TH=0 read -> start/A, TH=1 -> C/B/R/L/D/U, edge byte = new & (old ^ new), cleanup of opposite
directions) was correct: held keys in the menus, key repeat (32 frames, then every 4: `$7992`), and 80-frame holds in stage 0 are exact.
Fix: `step_frame` skips entries with n <= 0. Old scripts whose first entry was `[0,0]` now start one frame earlier.

## Light gun

### What the main CPU does (M: addresses)
- Setup (shooting phase 0 step 0, S:0x120D2): the sub raises COMFLAG bit 5; main (M:0xFF080C..0xFF083A) runs `sub_ff1316` (pad control 0x40),
  reads the device ID of port 2 with `sub_ff1358` (table M:0xFF1386: TH=1 / mask 0x0C, TH=1 / 0x03, TH=0 / 0x0C, TH=0 / 0x03 -> bits 8/4/2/1) into
  `$A12013` (-> sub `$7983`), and for ID 1 (gun) sets `IO_PAD2_CTRL = 0xB0` (TH interrupt enable); then acknowledges with COMFLAG_MAIN bit 5.
  `$7972` (gun mode) = `$78B8` (option) && `$7983 == 1` (S:0xD766).
- Per frame (M:0xFF0750 `sub_ff13e2` -> `sub_ff13f6`): `clr.b $E050` (arm the capture), read PAD2 data (D0 = trigger, D1 = second button, active
  low), edge -> `$A12015` bit 5 / bit 6 (-> sub `$7985`). A trigger edge while `$E056 != 0` (COMSTAT0 = `$7970`, "blaster drawn") sets `$E058` = 1,
  which makes the VINT code (M:0xFF1A44) write the white buffer `$EC80` into CRAM 0..1 for that frame (the screen flashes so that the gun sees light).
- `$7970` -> COMSTAT0 -> `sub_ff13ba` sets the **external-interrupt enable** (VDP mode register 3 = reg 0x0B bit 3; the shadow word is `$FFEF06`).
- The level-2 vector goes through the RAM vector `$FFFD14` (-> M:0xFF148A): if `$E050 == 0` and not in V-blank, delay loop (0x15 `dbra`), read the HV
  counter twice, `$E054` = (V of the first read) << 8 | (low byte of H1 + H2) >> 1, `$E050++` if V != 0.
  M:0xFF14E0 reports `$A12016 = $E055 * 2` (X) and `$A12017 = $E054 >> 8` (Y) -> sub `$7986/$7987`; S:0xD7B2 derives
  `$798A = $7986 - $7988`, `$798B = $7987 - $7989` (byte wrap).
### Oracle model (`re/oracle/machine.py`, `m.gun = {'x','y','trig','aux'}` in screen pixels)
Port 2 ID/button reads (`_pad2_read`), the TH interrupt raised when the beam passes (x, y) (main CPU run in small slices from the trigger so that the
live H counter read in the handler is meaningful, `_hcount`: 342 clocks per line, value = clock >> 1, 0x93 -> 0xE9 jump), vector stub
`2F38 FD14 4E75` at M:0x430. Result (aim x = 128): raw X = x - 66 (even steps of 2, jitter +-4), raw Y = y + 1 (y for x < 80); X is unusable for x < 32
(the handler reads inside the blanking). The absolute offsets are model-dependent (HYPOTHESIS), the 1:1 scale is not; the calibration removes offsets.
### Calibration screen (DATA_D2 state 11 sub 3, S:0x298B4; needs `$78B8 = 1`, i.e. a gun at power-up)
"GUN ADJUST - AIM FOR THE CENTER AND PULL THE TRIGGER": trigger -> `$7988 = $7986 - 0x80`, `$7989 = $7987 - 0x80` (S:0x29930); the test crosshair (object 16)
is at (`$798A` - 0x0B, `$798B` - 0x23) (S:0x299B2). Oracle: aim at the bullseye centre (128, 101) -> `$7988/9 = C0/E6`, value 0x80/0x80, the crosshair is drawn
exactly on the target; aiming at (100,80) gives value (0x64, 0x6B) = (x, y + 27).
### The 31-line discrepancy (HYPOTHESIS, handled in the port)
Shooting cells (S:0x12516): x < 0x60 / < 0xA0 and y < 0x30 / < 0x58. With the calibration above (value = (x, y + 27)) the oracle splits the picture at
x = 96/160 (matches the 3x3 picture) but at **y = 21 / 61**, while the pictured rows are [8,46) [46,86) [86,130] (targets 32x32 at y = 16/56/96): scanning
the oracle (`g5`-style scan: gun at (x,y), shot, read `$7900`) gives row 0 for y < ~21, row 1 for 24..61, row 2 from 64. Designers' thresholds correspond to
value = y - 4 (row boundaries 52/92). Real hardware may have measured differently (HYPOTHESIS); the engine therefore carries `GUN_CAL_BIAS_Y = 31` in the
calibration (menus.rs) and its default calibration so that aiming at a cell of the picture shoots that cell.
### Engine
`menus::frame_hook`: raw = `gun_raw(px, py)` (X = (px - 66) & 0xFE, Y = py + 1) -> `$7986/$7987`; `$798A/$798B = raw - $7988/$7989` (wrapping);
until the player calibrated (`Ix::gun_cal_done`, set by the calibration screen or a loaded save with a calibration) a default calibration for value =
(px, py - 4) is used. `title.rs` calibration stores `$7988 = $7986 - 0x80`, `$7989 = $7987 - 0x80 + GUN_CAL_BIAS_Y` and draws the test crosshair at
(`$798A` - 0x0B, `$798B` + bias - 0x23), i.e. on the mouse.

## Findings fixed in the engine (each with evidence)
1. **RNG step per frame** at S:0xD644 (before the sequencer): without it every random decision (spawn cell, type, timers) diverged (oracle: second RNG
   call of a frame gives table index 4 instead of 1).
2. `$791C:$791E` is a 32-bit pointer to the spawned object (`move.l a0,$791C`, S:0x12BE4); the port stored the slot number.
3. `init_targets` S:0x12248 clears only target 0's active word (the loop does not advance `a0`).
4. **Shooting sounds were silent on the web**: `shoot.rs` emitted `Out::Pcm` (ignored by `web/src/main.ts`); `subfn_06` is now `Out::PcmSfx{id, bank}`
   (bank = last bank-select voice id), `subfn_12(n, 0)` is `fm_cmd(n, false)` (`$980F`).
5. **Phase 4 waits for the PCM driver** (S:0x13072 `btst #3,$1511E`): the results of a stage appear only after the last effect has been played.
   `$1511E` bit 3 is set by `subfn_06` (S:0x15342) and cleared by the driver (S:0x15998) when an effect channel played to its end; oracle (stage 4,
   bank 33): channel-1 effect of 77 frames started at f37 cleared the bit at f118 although the channel-0 effect of f44 (179 frames) was still playing, so
   the **first** finishing channel clears it. Model: lengths from `pcm.json` (`set_sfx_len(bank, id, frames, chan)`, web: `main.ts`), error <= 6 frames.
6. Sequencer **command 0x1F** (S:0x13150) was a no-op: it is the counter tally (every other vint, counter objects 58/59 count down, then reset).
7. Gun: `$798A/B = raw - calib` with byte wrap (the port clamped), raw hardware values, calibration screen semantics (see above).

## Known differences / not verified
- Sub-CPU load: sample copies (1-2 frames per effect) and the other PCM driver work stall the original's frame handler; the engine is not slowed by sound.
  Visible only as lost presses while a sound plays in the original.
- `$1511E` bit 3 outside phase 4 is only approximated (the engine reports it for the trace); the results wait is within <= 6 frames of the original.
- Calibration absolute raw values (-66 / +1) come from the oracle's HV model (timing of the handler vs a line model), the 31-line bias is derived from the oracle
  scan above; neither could be checked against real hardware.
- Screens: see "Screens" below.
