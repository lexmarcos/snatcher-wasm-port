# MODULES workstream – boot screens, title (DATA_D2), intro (DATA_D1/D0), late game (DATA_Y16)

Conventions as in docs/RE_NOTES.md ("S:" = sub CPU, "M:" = main CPU). Code: `engine/src/modules/`
(`boot.rs` main-CPU copyright + Konami logo, `title.rs` DATA_D2, `timeline.rs` DATA_D1/D0/Y16, `mod.rs`
services). Listings: `python3 re/dis68k.py <DATA file> 28000 're/labels/sub_*.txt' --entries <hex entry> -o re/out/dis/modules_XX.asm`
(images in `re/images/`, `re/out/dis/modules_{D0,D1,D2,Y16,main}.asm`), labels `re/labels/labels_mod_{D0,D1,D2,Y16}.txt`
(module code overlaps at S:0x28000, so each module has its own label file), `re/labels/sub_60_modules.txt`.
Intro/oracle tooling: `re/py/modules_scn.py` (oracle runner, CD time model), `re/py/modules_cmp.py` (PNG comparison),
`re/oracle/scenarios/modules_*.json`.

## Status
- **Done and verified against the oracle**: boot screens (copyright, laser, Konami logo, fade; `boot.rs`), title module DATA_D2
  with NEW GAME / CONTINUE, slot select, settings screen (`title.rs`), intro DATA_D1 and DATA_D0 (`timeline.rs`), ending DATA_Y16
  timeline (the staff roll of the ending; steps 0–25 incl. the wait for the end of track 19 verified, §6), whole chain `boot → title → options → intro → game`.
- **Approximate / unverified**: gun calibration screen (no light gun peripheral in the oracle), the Y16 steps after 25 (START at the end, S:0x292AE–0x292BC), display-on after the dedication shutters (HYPOTHESIS §3.2), tile-channel DMA throttling (§5), file-load delays
  between the modules (§3.1), the 2-frame lead of the engine's sub-driven display over the oracle (§6).
- **H40 (task 3) not done on purpose**: the oracle's VDP register 12 is 0x00 (H32, 256 px) during the entire boot, title,
  options, DATA_D1 and DATA_D0 playback (27000 frames logged with `reglog`; the IP's register tables `M:0xFF1584`/`0xFF15A2`
  contain `8C00`, no `8C8x` word exists anywhere in IP.BIN or the DATA modules). All reference PNGs in `re/oracle/out/refs`
  are 256×224. There is nothing to implement; `Game::width()` stays 256.
- **Next**: Y16 steps 25–27 with START (the oracle run only reaches the wait), mono/stereo option has no audible effect yet (only the
  `Out::Fm` 0xF3/0xF4 and PCM 0xFC/0xFD commands are emitted), gun calibration verification when a gun model exists.

## 1. Frame and mode machine facts used here
- One sub frame per main VBlank (actions.md §5.1). `mode_tick` (S:0xE046) and `obj_update_all` run only when `$7887 == 0` and
  `$7886 == 0` (S:0xD6EE–0xD71E): `$7886` = a scene VRAM list is being uploaded (set by scene chunk 1, S:0xF6FC; cleared by
  S:0xE39C), `$7887` = scene-reset plane clear handshake (S:0xF682 sets 1, S:0xE360 → B6403 = 3 → 2, then 0). `palch_run`
  (S:0xE4B4) and `tilech_run` (S:0xE846) are skipped while `$7886` is set. Engine: `Engine::mode_blocked()`,
  `Runtime::f7887` (core/src/gfx/anim.rs). A VRAM list entry is followed by at least one idle frame (`tst.w wr_dmaq`,
  S:0xE38A; evidence: DATA_D2 scene 0 has 9 entries and the module is blocked for 19 frames in the oracle).
- `$7848` (S:0x6102) counts sub vints. The intro driver acts only when `$7848 & 0x1F == 0` (§3.1).
- CD time: `cd_time` (S:0xD5FE) reads `CDBSTAT` (BIOS 0x81) word +0 = status (0x0100 playing, 0 stopped, 0x0500 paused) and
  longs +8 / +0xC (`$7876` / `$787A`, 0xFFFFFFFF = unchanged). `$787A` = relative track time `MM SS FF 00` BCD with FF in
  1/75 s.
  **The stock oracle answers 0xFFFFFFFF, so `$787A` stays 0 and every timed module stalls at its first entry** (this is why the
  oracle's old intro scenario never saw anything but the disclaimer). `re/py/modules_scn.py` patches `Machine._cdbios` in
  process: `$787A = frames since MSCPLAY × 75/60` (HYPOTHESIS for the 75/60 conversion, consistent with the tables: the last
  DATA_D1 entry 02:12:40 is where track 3 ends, oracle `end_of_track` 3 at +7974 frames). The engine models the same
  (`cd_time_bcd`, game.rs). Track lengths from `snt extract` confirm the tables: track 3 = 9971 sectors = 02:12:54 (D1 ends 02:12:40), track 5 = 00:19:05 (D0 starts track 4 at 00:19:00), track 4 = 04:32:06 (D0 ends 04:30:00); `cd_status` in the engine reports 0 after the track's length (`TRACK_SECTORS`, mod.rs) because DATA_Y16 waits for the end of track 19.

## 2. Boot screens (main CPU, `boot.rs`)
Main state `$E022` 0 → 1 → 2 → 3 (M:0xFF09FC table), VBlank driven, writes the VDP directly; the sub CPU waits in mode 0.
`$E04A` steps (M:0xFF2B66 for `$E022 = 0`, table M:0xFF2C22 for 1). Oracle watch (snapshot frame at which the value is first
seen, `modules_boot.json`): `$E04A` 1@1 2@2 3@3 5@7 6@8, `$E022 = 1`@9, then 1@13, 2@196 (copyright 0xB4 frames), 3@198,
4@202 (laser), 5@244, 6@261, 7@378, 8@398, 9@418, 0xA@439 (0x200-frame hold), 0xB@951 (fade, 0x28 frames),
`$E022 = 2`@991, 3@992; sub mode 1 at 993, first D2 call at 997. Data comes from the user's IP image
(`HEADER.BIN[0x200..0x800] + IP.BIN` at M:0xFF0000; the web front end loads `/assets/raw/header.bin`+`ip.bin`, missing =
screens skipped).
Fixes made in this workstream: the `$EBE4` colour ramp is `clr.l, 00200040, 00600080, 00A0` (colours 50..56, M:0xFF2D80; the
earlier port was shifted by one and drew a broad glow around the laser), the boot register state is no longer pushed again
after `finish` (the title picture was drawn with 64-wide planes), stalls re-calibrated (+2 frames at step 0 of `$E022 = 1`,
+1 at steps 2 and 3).
Skip: START from step 2 on (COMCMD0 lo bit 7) → `finish` (copyright is not skippable).

## 3. Timeline modules DATA_D1 (entry S:0x29266), DATA_D0 (S:0x2A828), DATA_Y16 (S:0x290D4)
Common shape (S:0x29266 and equivalents):
```
$7896 < 0 : first call → subjt_19(track) (CD play once); then wait until subjt_18 == 0x0100, clr.l $787A, $7896 = 0
else      : entry = table[$7896] (6 bytes: handler word offset from the table base, CD time long MM SS FF 00);
            if entry.time > $787A: (D1/D0) START → $7846 = -1; return. else $7896 += 1; run the handler.
```
Tables: D1 S:0x29298 (29 entries, last 02:12:40 = done), D0 S:0x2A85A (35 entries, last 04:30:00; handler 0x2A966 plays track 4
at entry 3 and the following entry waits for it and resets `$787A`, so the remaining times are relative to track 4), Y16
S:0x290FC (28 entries; times up to 02:04:00 then wait-for-track-end/timer steps without time). Handlers are tiny: scene
`subfn_17` + `subfn_16(n)`, palette channel descriptor starts (`83..` fade), `subfn_13(8..12)` shutter effects (D1 entries
5–9), `subfn_13(1)` + `$8F1C = 0x110`, the final `$7846 = 1`. The tables and handler translations are in `timeline.rs`;
every handler address is in `re/labels/labels_mod_*.txt` and `re/out/dis/modules_*.asm`.
Driver `mode2_intro` (S:0x14A52, states in `$7844`): 0 = (gate) CD pause if playing / `subfn_04` load of the file in table
S:0x14AA0 (`00 03 00 02` = DATA_D1 then DATA_D0), 1 = (gate) poll the load, `$7896 = -1`, 2 = call the module each frame,
`$7846 > 0` → part 2 (`$7852`) / `subfn_03`, `$7846 < 0` (START) → fade `83 00 00 3F` for 10 frames → `$7846 = -1`, `subfn_17`.

### 3.1 Timing model of the driver
Both gate steps need `$7848 & 0x1F == 0`. Oracle (title run, START at 1612): CD pause at 1633, `ROMREADN` at 1669, CD play at
1704; with START at 1660: `$7844` 0→1 at 1734, `$7896 = -1` at 1769, play 1770. Model in `timeline.rs`: gate frames ≡ 5 mod 32
(engine frame = oracle frame on the title run), pause at the first gate when music plays, load at the next gate, ready
35 frames later: predicts 1637/1669/1704 and 1701/1733/1769 (±1). For DATA_D0 no pause (track 3 has ended) and the same
gate model; the real `$7848` drifts against the frame counter (skipped vints), so the D1→D0 gap is only reproduced ±30 frames.

### 3.2 Display state after the dedication (HYPOTHESIS, evidence)
Shutter effects fx 8–12 (M:0xFF3836, `hint_handler_c` M:0xFF0D6E) cut the display at the band end each frame (`bclr #6,$FFEF03`
at M:0xFF0E14) and the last one (fx 12, e060 = 0xC) resets the effect channel (`loc_ff3406`) without setting the bit again.
Nothing in IP.BIN sets it for the intro scenes (the only `bset #6,$FFEF03` are the fx 7 handler end M:0xFF0D32, the band start
M:0xFF0DC4, `sub_ff1720` for B6406 requests/boot), D1/D0 never request B6406 (no scene chunk 8) or fx 7. The oracle therefore
shows a black screen from fx 12 on for the rest of D1 and all of D0 (reg 1 = 0x24 for 27000 frames, `$FFEF02` = 0x8124),
although the scenes are loaded and animate: forcing `$FFEF03 |= 0x40` at frame 3500 (`ora_force` run) plays the whole prologue
("Moscow: June 6, 1996", corridor, DNA screens, Neo Kobe Times, SNATCHER logo, JAPAN, "Dec. 2047 NEO KOBE CITY"). So some
mechanism that neither the oracle nor my reading of the IP reproduces re-enables the display. The engine does the minimal thing:
when no H-INT mode is active (`$E06C == 0 && $E036 == 0`) the display bit is on again (`raster.rs`, `Raster` `display`);
`modules_scn.py` has the same rule behind `"force_display": true` so that oracle and engine can be compared.

### 3.3 Other fixes
- Anim op 0x9A is the same handler as 0x99 (S:0xFB92; wait for CD time `+$20`, bit 2 of `+$25`); it was ignored, so the scene objects
  that wait for CD time (scene 5 of DATA_D1: tables 00:57:16, 01:04:05, …) ran unsynchronised.
- `palch_run` skipped while a VRAM list is pending (S:0xE4B4).
- `$7887` handshake and mode gating (see §1).

## 4. Title module DATA_D2 (entry S:0x285CE; `title.rs`, `labels_mod_D2.txt`)
Entry: debounce (`$28ADB`: same non-zero new-press byte as last frame → ignore). Variables: state `$7896`, sub `$7898`, timer `$784E`,
cursor row `$28AD8`, cancel flag `$28ADA`, backup RAM flags `$CDAC` (0 internal) / `$CDAD` (formatted) / `$CDAF` (free space),
slot bits `$78B6`, selected slot `$78B7`, slot present words `$79E2[4]`, options `$78A2` (sound, 1 = mono: F3/FC), `$78B8` (gun),
`$78BA` (button config 0–5), `$7988/9` (gun calibration), continue flag `$785C` (word!). `sub_0287BE` = next state, `sub_0287C8` = next sub.
| st | what (oracle frame of the first call) |
|---|---|
| 0 | sub 0: sound mode from `$78A2` (FM F4 + PCM FD at power-on), BRMINIT; sub 1: `$B6502` (RAM cartridge) → none → next (997/998) |
| 1 | `subjt_19(2)` CD track 2, wait status 0x100 (999/1000) |
| 2 | `subfn_17`, `subfn_13(1)`, `$8F1C = 0x100`, scene 0 (title picture), timer 0x18 (1002) |
| 3 | timer → `subfn_13(7)` (the 4-frame squeeze that brings the picture into view), timer 4 (1045 = 4) |
| 4 | timer → palette list S:0x288E6 (32 frames fade in from white), timer 0x56 (1049 = 5) |
| 5 | timer → next (1135 = 6) |
| 6 | formatted backup RAM → next (the "no backup RAM" message scene 2 is not used) |
| 7 | sub 0: read the 4 slot headers (BRMSERCH/BRMREAD "SNATCHER_01..04" → `$78B6/$79E2/$78B7`, table of the first 8 bytes of each block); sub 1: → 8, and → 9 directly when saves exist (1136–1138) |
| 8 | free-space check (`BRMSTAT` ≥ 0x0E blocks → `$CDAF`); full → scene 3 message (not reachable) |
| 9 | `subfn_13(0xFF)`; with saves: object 17 toggles NEW GAME/CONTINUE on Up/Down (script of object 17: PAUSE, frame 1, PAUSE, frame 0 …; default = CONTINUE; START + `obj17.idx == 3` = NEW GAME); without: START → next. FM sfx 0x38. NEW GAME: `$785C = 0`, st+2; CONTINUE: `$785C = 1`, st+1 |
| 10 | slot select: timer 0..0x14 starts tile lists A[t] (S:0x29142, 21 words) and, for t ≠ 0, B[t] (S:0x29412); `sub_028D26` draws the slot panel (play time BCD, counter, `$79F0`) into Word RAM buffer 0 at B8256+0x100·slot at t = 3; t = 0x15 → interactive: Up/Down next existing slot (objects 24/25 `+$28 = slot·4`), START → BRMREAD → restore button config/gun/sound from `$9C0A`, `$7988` from `$9C08`; B → cancel (sub 2: back to st 9 after 2 frames, object 17 `+$28 = 0`); sub 1 plays the close lists C (S:0x296C6, 20 words) |
| 11 | settings: sub 0: lists D[t] (S:0x29292, 24 words) + E[t] (S:0x29554), then rows 0–4 (cursor `$28AD8`, objects 32–36 `+$28`): 0 gun on/off (only with a gun on port 2, `$7983 == 1`), 1 button config Left/Right (0..5), 2 sound mode (toggles `$78A2`, F3/FC ↔ F4/FD), 3 gun adjust, 4 QUIT. START/A/B/C: row 3 → calibration, else PCM 0x38, fade (palette `83 00 00 3F`, timer 0xA) → st 12. Calibration: sub 1 scene 7, sub 2 init (`$7970 = 1`, sub flag bit 5), sub 3 steps in `$784E` (trigger/aux of `$7985`, crosshair = `$798A/B` − (0xB, 0x23), 0xB4-frame timeout), sub 4 back to scene 8 |
| 12 | timer 10 → `$7846 = 1`, `subfn_17` |
Engine mapping: `$785C` is a word (`regs.get(0x785C)`; `game.rs` used the high byte before), the settings write `Input.config`
(`$78BA`) and the regs read by the save snapshot (vm.rs 0x78BA/0x78B8/0x78A2/0x7988). Saves come from `Engine.saves[slot]`
(0x1C0-byte block; first 8 bytes feed the panel). Mouse "gun": the engine stores the calibration offsets as 0 (UNVERIFIED, the mouse
is pixel exact).

## 5. Services and approximations
`SubServices` (mod.rs): `sub_scene`, `sub_scene_reset` (S:0xF5E8: black-out, palette channels off, tile channels off, scroll reset FC,
objects without the persistent bit killed, planes A/B cleared + `$7887`), palette/tile channel starts, `subfn_13`, CD play/status/time,
sound12/sound06 (FM sfx/music with `$980F/$980E`, PCM sfx), raw pressed bits (inverse of the S:0xDFF6 button remap).
Approximations: tile channels with buffer index 0 wait for an empty DMA queue in the original (`tst.w wr_dmaq`, S:0xE8F2…); modelling
that as "skip while a push happened this/last frame" starves the 24+23 list starts of the options screen (lists dropped), so the
engine runs channels unthrottled and the panels build ~2–4 frames faster than the oracle (frames 1306–1329, 1205–1225 of the
scenarios). Sub→display latency: the engine renders the state of the same tick; in the oracle the palette reaches CRAM two vints
later and the effect mailbox (B6408/B640C → `$E060`/`$E070`, then the H-INT setup next vint) three, so engine frame = oracle
frame − 2 (palette/tiles) / − 3 (screen effects) in transitions. Static frames are identical.

## 6. Verification (oracle = original code, PNG comparison with `modules_cmp.py`)
| scenario | result |
|---|---|
| `modules_boot.json` frames 1–460 (offset 0) | mean abs diff 0.20 (max 17 at the 196–200 and 380–381 transitions, ±2 frames); frames 945–1000 (fade) 0.00 |
| title picture 1094+ (offset 0..3) | static frames identical (0.00); squeeze/fade-in 1046–1093: offset 3, mean 1.37 |
| `modules_options.json` (START, DOWN/RIGHT/LEFT/UP on the rows, START) | frames 1330–1660 (offset 2) identical, build-up 1306–1329 ≤ 8.6, fade 1662–1670 ≤ 45 (the oracle takes START 2 frames later), D1 start identical at offset 2 except fade rounding ≤ 1.9 |
| `modules_continue.json` (two saves, slot select, B cancel, load) | `$7896` transitions identical (10@1200, 9@1414/1413, 10@1470, 11@1661), frames mean 0.5–0.6 at offset 2–3 |
| `modules_intro.json` (D1+D0 with the CD time model, forced display) | 100-frame samples D1 0.00 for 70 of 80 samples, others ≤ 9.2 (transitions, random-number displays); D0 mean 3.4 (fades off by a few frames, random numbers); the engine ends D0 at 25406 (track 4 + 270 s) |
| `modules_d1_late.json` | black-screen analysis, state `modules_d1_3500.pkl` (gitignored) |
| DATA_Y16 staff roll (oracle: `junker_hq.pkl`, patch `$7842 = 6, $7844 = 0, $7846 = 0` five frames after loading, CD time model; engine `run … ending`) | `$7896` transitions identical to ±3 frames for 28000 frames (constant start offset: music at +4 oracle vs +1 engine): step 17 (track 19 ends at 31548 sectors) at 25241, 19@25541, 20@25605 (CD track 2 jingle), 21–23@25606–25631, wait for START from 25945; pictures: credit text scrolls differ by 1–3 px (scroll phase), portraits/robot/sunset identical (mean 5.9 over 280 samples, max 26 on scroll frames) |
Whole chain `boot → copyright → laser → logo → title → OPTIONS → disclaimer → (START skips) → Omni Building → ACT 1 → Junker HQ + Mika`
runs in the engine (`cargo run --release -p snengine --example run -- 5000 250 OUT "1301:128,1612:128,2200:128" boot`), tests in
`title.rs` (`title_to_intro`, `title_without_ip`, `continue_slot_select`).

## 7. How to reproduce
```
cd re/oracle
.venv/bin/python -I ../py/modules_scn.py scenarios/modules_boot.json --out OUT_O          # copyright..title (needs no state)
.venv/bin/python -I ../py/modules_scn.py scenarios/modules_options.json --out OUT_O       # needs states/title.pkl
.venv/bin/python -I ../py/modules_scn.py scenarios/modules_continue.json --out OUT_O      # two saves via "bram"
.venv/bin/python -I ../py/modules_scn.py scenarios/modules_intro.json --out OUT_O         # D1 + D0 (3 min)
cd ../.. && SHOTAT=1,2,3,… cargo run --release -p snengine --example run -- N 10 OUT_E "frame:bits,…" boot   # SAVES=0:blk.hex,2:blk.hex
re/oracle/.venv/bin/python -I re/py/modules_cmp.py OUT_E OUT_O OFFSET [--lo A --hi B] [--bad T]
```
The inputs of a scenario are `[[frames, pad bits], …]` relative to the loaded state; the engine takes the same presses as absolute
`frame:bits`. `watch` entries print sub/main memory when it changes; `reglog` prints VDP reg 12/16/11 changes.
