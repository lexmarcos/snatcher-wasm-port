# FIDELITY workstream – engine vs ORIGINAL (oracle) through Act 1

## Status
- **Result**: a 58,000-frame (16 min) oracle session through Act 1 – Junker HQ lobby (all LOOK/INVESTIGATE/TALK/ASK branches, Mika), the guided tour (Chief's office, engineering, detective's room,
  computer room, shooting range), Harry/Metal Gear Mk II, the turbocycle ride, the abandoned factory exterior and first interior rooms (SP06 → SP07, MOVE/LISTEN/SAVE/USE METAL GEAR menus) – replayed in the engine:
  **158 of 159 action queues are identical** (the only differences are the 3 engine-only boot passes before the anchor), drift ≤ 80 frames after 58,000 frames (`fid_cmp.py --sync`, `docs/re/fidelity.md`
  "Harness"). Screens (233 samples): the bulk of the remaining differences are looping-animation phase (fan, blink, cloud wave, palette cycles; the same image appears at an offset of a few frames or ≤ 40 frames
  for the factory sky) – see "Known differences".
- Message box, command menu (open / move / confirm / close), portrait panels, scene changes (cmd 0F/12/0D), cmd 0B/0E CD waits, cmd 15 popups: verified frame-exact (menus) or within 1-3 frames (scene loads).
- Web smoke test (headless Chrome, `scripts/build-web.sh`, server :5300): `?newgame`, `?scene=SP01:0`, `?newgame&debug` load without console errors, audio manifests 200, message arrow visible.

## Harness (re/oracle)
- `fid_play.py --cycle --rng --frames N --out play.json`: plays the ORIGINAL adaptively (menu policy: every distinct menu's items in turn; confirm every message) and records the pad script
  `[[rel,pad,kind,since],..]` (kind m = message waiting, u = menu key loop; `since` = frames after readiness; lost presses are dropped); prints the story (menus, messages, queues, script changes).
- `fid_cmp.py --rng --sync --inputs-file play.json --frames N --every K [--refine PX]`: oracle replay with the absolute script, engine replay with `SYNC` (presses relative to the engine's own readiness,
  `Engine::wait_state`), queue comparison per VM pass (difflib), screens at drift-corrected frames; `--refine` searches engine frames +-12 (`--refine-range`) for the best match to tell phase differences from content differences.
  `--rng`: oracle pokes `$786A..D = 0` every frame, engine `FIDRNG` (RANDOM nodes deterministic on both sides).
- `fid_watch.py` per-frame memory watch (`s:`/`m:` addresses), `fid_probe.py` VDP/sprite/object dump at a frame, `fid_sheet.py` contact sheets, `hookpc` style PC hooks were used ad hoc (see Findings).

## Tools
- `cd re/oracle && .venv/bin/python -I fid_cmp.py --name NAME --inputs '[[rel_frame,bits],..]' --shots 100,200 [--every K]`
  Oracle = `states/junker_hq.pkl` (+300 idle frames, the 3-item command menu is up), engine = new game, 1500: press C (dismisses the first Mika message),
  rel 0 = engine frame 1530. Output `out/fid/NAME/{oracle,engine,diff}/rNNNNN.png` (diff = oracle | engine | |diff|), `report.txt`.
  Queue comparison drops the oracle's menu-draw passes (cmd 22/16/05 only, `1C 1D`) – the engine draws menus natively.
- `fid_watch.py --inputs .. --frames N --watch s:7892,m:E061 [--changes]` prints sub (`s:`) / main (`m:`) memory per frame (line n = state after frame n,
  the pad of rel frame n is applied in frame n).

## Findings (each fixed in the engine; evidence = oracle watch/probe, original code address)

### Message box (S:0x135B2, `core/src/actions.rs` MessageBox, `Engine::cmd_message`)
1. **Wait-for-confirm arrow** (state 5, `msg_wait_confirm` S:0x137E8): every `$9743` frames (table `msg_wait_anim_tbl` S:0x1391C: `(src,duration)` pairs
   `20/0C 24/4 28/4 2C/4 30/4 34/4 28/4 38/4 20/4 FF/0`) a 2x2 nametable block (4 words per row from S:0x138C4 + 8k) is written with tile channel kind 0x52 (descriptor S:0x1387C;
   `$7893` = 1: S:0x13894, 2: S:0x138AC, source pointers patched at +2/+0xC, `sub_01383c` S:0x1383C). Not drawn when `$9744 == 0` (narration). With `$97CC != 0` (text code F4) the animation end completes
   the wait by itself. Oracle: arrow at (233..241, 193..204) animating with a 44-frame cycle; engine now equal within a frame (diff 0 px in half of the sampled frames, <60 px otherwise).
2. **States 0 and 1 existed in the original but not in the port**: state 0 (S:0x13604): `sub_0139D4` (objects of slots 5..14: `clr.w 0(a0)`), palette channel S:0x13622, `obj_slots` word 0 = 0 (menu bar object off);
   state 1 (S:0x13626): `$7891 = FF`. They cost 2 frames; oracle: pass 62, `$9740` 1/9/10/2 in frames 62/63/64/65, main `$E060 = 0F` in frame 66.
   State 10 also sets `$9717 = 0` (S:0x13674 → `sub_01395A` S:0x13984), state 2 with narration sets `$9717 = FF` (S:0x1369E): portrait objects test `$9717` for the talking animation.
3. Mailbox latency: `scroll_mode` (S:0xF3F8) values reach the main CPU one frame later (`Runtime::scroll_tick` `mail_next`); the sub's copy `$7892` of main `$E061` (and `$788C` of `$E06E`) is 2 frames late (S:0xD746/0xD74E read Word RAM $B6409/$B640A).

### Command menu (S:0x1398E, 0x10CBE, 0x11004, 0x11DCE)
4. **Open sequence** (frame 0 = menu items pushed): +1 `sub_01398E` (objects 5..14 off, palette channel S:0x139D0, `$7891 = FF`), +3 `txt_window_init` (cmd 16), +5.. one render stream per visible line and two more
   (previous/current cursor line) every 2 frames (cmd 05 waits for the glyph renderer + `sub_00D4AC` frame wait), 2 frames after the last stream `scroll_mode(0x0D)` (0x12 wide), the key loop starts when
   `$788C` (main `$E06E`, set when the window effect ends) is set, the bar object becomes active 2 frames later (`move.w #1,obj_slots` S:0x1104E; before that `$7A25 = 0` S:0x10FFE only clears the hide flag).
   Oracle (`fid_watch.py`): items 253, streams 258..268, bar flag 270, `$E060 = 0D` 271, `$788C` 279; engine pixel-identical from frame 252 to 300.
5. **Cursor move** costs two streams: `$9710` changes one frame after the pad (the VM thread runs one frame behind the input interrupt), previous line redrawn in colour 4, bar moves, new line in colour 1
   (bar Y changes 3 frames after the pad). **Confirm**: SFX 0x1D is queued (cmd 1C, the thread waits for the sequencer), next frame `$7A25 = 3` and `scroll_mode(0x0E)` (0x13 wide), one frame wait, then wait for `$7892 == 0`
   (S:0x11E3C) – the next pass starts 11-12 frames after the pad (before: same frame, window cleared at once). The remembered cursor `$9716` is cleared when a pending jump is applied (S:0x168C2) – the port kept it.
6. `text_window_init` at menu open (S:0x10D24) – without it old text lines stayed under shorter menus.

### Sequencer
7. **Command 0D (voice) is a 4-step command** (S:0x101DC) and `S:0x13FEC` is a 1198-byte table voice id → scene number (0xFF = none), not "all FF" (docs/re/actions.md was wrong): step 0 starts the voice (`$9717 = FF`),
   step 1 idle, step 2 `sub_013FAA` raises `$7891 = 1` (main palette split: white speech panel + portrait) when a scene exists, step ≥3 `sub_013FC6` runs scene table[n] (the speaking portrait objects, e.g. Gillian = scene 3 of DATA_H00).
   Starting a voice blocks the sub frame handler for 2 frames (`Engine::sub_stall`; oracle: no S:0xD6EE hit in frames 5298/5299 after the voice start at 5296) and the oracle's voice intervals are 5-7 frames longer than the manifest duration (`voice_len + 6`).
8. **The sequencer is skipped while `$7886` (scene VRAM list loading) or `$7887` (screen reset handshake) is set** (S:0xD6EE..0xD71A) – the port ran it anyway, so commands after a scene load ran before the tiles were in VRAM.
9. **Command 0F** (S:0x1027E) is stepped: frame A black-out (S:0xF560), frame B `subfn_17` (S:0xF5E8) raises `$7887`, 2 more blocked frames, then done (oracle 463/464/465/466).
10. Palette split request (`$7891` → `$B6405` → main `$E036`, S:0xD78C/0xFF3298): about 3-4 frames until the raster split shows (`Runtime::split_next`). The split's HINT handler (M:0xFF0C4A) writes
    one pass more than the 12 line groups: the 13th pass (line 148) writes the white buffer `$EC80` (filled with 0x0EEE at M:0xFF1396) to CRAM 0..3 through the wrapped address 0x80 – this is what makes the portrait panel white
    (`core/src/gfx.rs` `y16 == sl + 12`). Oracle CRAM sets per line: `fid_probe.py`.
11. Scene object chunk (S:0xF718): `tst.b (a1)` tests the high byte of the active word, which is 0 for a live object – the slot "busy" test never stops the chunk in practice (HYPOTHESIS, not changed).

12. **Sequencer command 0B/0E**: 0E plays a CD-DA track once (MSCPLAY1), 0B (S:0x101A6) waits until the CDBSTAT high byte is 0 (track ended) or 5 – the port completed 0B at once, so cutscenes that wait for a
    jingle ran 800-1300 frames early (turbocycle departure). Uses `SubServices::cd_status` (track lengths from the extract report).
13. **Scene object chunk, busy slot** (S:0xF718): the test is `tst.b (a1)` on the HIGH byte of the active word, never set for live objects, so an occupied slot is re-created, not skipped
    (hypothesis 11 above proved right): command 15 popups (the BLASTER picture box, object 1) were never drawn because slot 1 was occupied.

### Not modelled / known differences
- Random: the original's RNG `$786A..D` is stepped every sub frame, so RANDOM nodes (and random-duration object waits) differ run to run; the harness pins both sides (`fid_cmp.py --rng`: oracle pokes `$786A..D = 0` every frame, engine `FIDRNG` sets (a,c) = (0,1) before each pass).
- Sub CPU load (LZ decompression, PCM stream prime) is only modelled for voice starts; scene loads end 0-3 frames earlier than the oracle (palette channel runs in the next handler frame).
- Oracle `trace.jsonl` `seq_start.f` is the frame index before the step (line - 1); the report prints the engine frame number, so "d +1" = identical frame.
