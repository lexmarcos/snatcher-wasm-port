# INTERACT workstream – menus, text entry, pointer mode, shooting, light gun

Code: `engine/src/menus.rs` (list-menu bar, keypad/keyboard entry, pointer mode, cmd 0x15, gun
registers), `engine/src/shoot.rs` (shooting stages, seq cmd 0x18), hooks in `engine/src/game.rs`
(`ix` field, `menus::frame_hook`, `open_menu_hook`, `menu_frame_hook`, `list_cursor`, `list_close`,
seq cmd 0x15/0x18, `apply_7893`, `jump_path`), `Game::set_gun` in `engine/src/lib.rs`,
`web/src/input.ts` (mouse → light gun). Labels: `re/labels/sub_70_interact.txt`.
Ports of S:0x10846–0x11E28 (menus/entry/pointer), S:0x1449A (cmd 0x15), S:0x1208E–0x1314E (shooting).

## Status
All four interaction types are played and compared with the ORIGINAL code (oracle, `re/oracle/`):
| feature | verified how |
|---|---|
| list menu highlight bar (object 0) | Junker HQ command menu rows 136–164 are **pixel identical** to `re/oracle/out/refs/junker_menu.png` (0 differing pixels of 7424). Sub-menus, paging arrows (SP01 debug menu) and the wide layout (`$7893`, Jordan terminal) compared by eye side by side. |
| alphabet keyboard (`$79FE` 3, SP05 via SP04 "LOAD I.D. FILE") | same input script on both: cursor movement, typing, backspace + key-press animation, OK/Start, two-step match (`$971A` 1 → 2) → "Alice Gibson" for ALICE, "No matching names on file." for CPO; open/close window effects |
| phone keypad (`$79FE` 2, direct dial `$971E`, SP35 videophone) | typed digits, cursor, auto-complete after the expected length (table S:0x11938: "177" → SP02 call), window clear on open |
| pointer mode (`$79FE` 1, dark room SP26:0BA4 "TURN ON LIGHT") | `$96D0/$96D4` per frame and `$96DC` region ids equal to the oracle (±1 px); bar hidden while B is held; flashlight sprites match |
| shooting (SYS MINIGAME, seq cmd 0x18) | stage 0 (SP01 debug menu "SHOOTING → ACT1 → SCENE_JUNKER"): READY/GO!, life meter, enemy/civilian targets, crosshair/flash look like the oracle; shot state machine `$78FA` 4→5→1→2→3→0 and `$7908/$790E` counts identical; stage 0x0B and the ACT2 QUEEN01 stage (SP01 debug menu) run to the game-over exit without errors; ACT3 stages were not reached |
| light gun / mouse | engine side tested headless (`GUN=x:y:from GUNB=frame:bits` in the runner: x=100,y=60 → cell 4, hit test runs); web side `input.ts` builds and exposes the state |

### Bugs found and fixed in this pass (all with evidence)
1. **Highlight bar never visible** – not a flags problem: the bar *tiles* were never loaded. The VM-thread
   init batch (`vm_thread_init`: cmd 03 S:0xECB6 / 0xECC4 tile tasks, cmd 01 S:0xED0E bar object, then
   `10 [1]`) was concatenated with the scene load; `10 [1]` (S:0x102AC counts `$9B42` up and completes when it
   equals n, so n = 1 completes in the same frame) did not wait, so cmd 0F's scene reset (S:0xE806, tile channels off)
   ran before `tilech_run` could upload tiles 0x6F0–0x6FF (VRAM 0xDE00, LZKN blob S:0x178F8). Now `10 [2]` (one real frame)
   and `Engine::jump` also runs the init. Oracle: VRAM 0xDE00 holds the tiles at the menu (probe.py), the engine had zeros.
2. Paging arrows (8-item menus, e.g. the debug menu) were drawn as glyph 0x82 at x=0x10; the original draws whole
   streams S:0x10EFA (up) / S:0x10F10 (down) / S:0x10F26 (down, wide) via `menu_item_stream` S:0x10E5A.
3. Scene chunk types 5/6/7 (`$7893 = payload[1]`, S:0xF7CA) were ignored: menus and message windows of
   portrait scenes (Jordan terminal, ...) were 16 px too high. Now `Event::Byte7893` (core/src/gfx/anim.rs) applied by
   `Engine::apply_7893` right after cmd 0x12/0x20.
4. Window split did not zero the horizontal scroll: hint_0EA8/hint_0EFA (M:0xFF0EE0 / M:0xFF0F32) write `0` to the H-scroll
   table (VRAM 0xF800) at the split, so the text window / keyboard never scroll sideways. `LineState.hscroll_zero`
   (core/src/gfx/raster.rs) + render (core/src/gfx.rs). Visible in pointer mode (text window followed the view).
5. Keypad: the text window was not cleared when the keypad opened (`menu_show` S:0x10D24 `sub_013D1C`); the dialled number
   completed instantly (oracle: digits appear 8 frames after the press, the keypad closes ~14 frames after the last digit,
   `Entry::pending`).
6. Keyboard closing: S:0x11C10 waits `$7892` (= main `$E061`, M:0xFF321C) until the closing window effect 0x11 is done before
   the VM continues; modelled with `Ix::closing`.
7. Shooting objects: countdown READY/GO! are slots **56/57** ($8B80/$8BD0) and the counters 58/59 ($8C20/$8C70)
   (`slot = (addr - 0x7A00) / 0x50`; confirmed by `clr.w $8B80` S:0x12390 and `move.w #1,$8B80` S:0x12FA6), the port used 52–55,
   so READY/GO! and the counter bars never showed.
8. Pointer mode speed: oracle `$96D0` grows 0x2.0A00 per frame with one D-pad direction = 261 loop iterations (0x200 each,
   `PTR_STEPS`); diagonal (two bits) ≈ 1.55 px/frame ≈ 200 iterations (`PTR_STEPS_DIAG`, fitted).

## Mapping notes (evidence)
- Menu bar: object 0 X = 0x10, Y = 0x8F + 9·line (+0x10 with `$7893`), `$7A25` = 0 shown / 3 hidden (S:0x10F90, S:0x1106E).
  Text colours: selected line colour 1 (white), others 4 (cyan); the bar is 7 sprites 32×16, attr 0x06F8.
- Text entry result: S:0x11AA8 compares the typed string with every collected item (exact), index+1 of the first match, else 1;
  `$971A == 1` matches by "item contains first typed character" (the "ABCDE" index menu), the following pass (`$971A == 2`)
  re-uses the typed string. Tests: `menus::tests` (match rules, space for empty input, pointer bounds).
- Pointer mode: held B (`in_held` bit 4) + D-pad moves the view inside X ∈ [-0x47, 0x45), Y ∈ [-0x27, 0x25) (16.16, S:0x10950/0x10962);
  on release the region id comes from the map S:0x109E8 at ((X+0x48)>>2, (Y+0x28)>>2) and `$9717` clears.
- Shooting pad aiming reads `in_held` ($7990) bits (S:0x124BA), the shot reads `in_pressed` ($7991): bit 6 = draw/holster, bit 5 = fire.
- Light gun: `$7985` (edge) bit 5 trigger, bit 6 second button; `$798A/$798B` = gun X/Y − calibration; cell = (x<0x60 ? 0 : x<0xA0 ? 1 : 2) +
  3·(y<0x30 ? 0 : y<0x58 ? 3 : 6) (S:0x12516). The target sprites are 32×32 at x 48/112/176, y 16/56/96 (OBJDUMP), so the thresholds
  0x60/0xA0/0x30/0x58 equal the screen-pixel edges between the cells: HYPOTHESIS that after the title calibration the gun values are screen pixels.
- `$7972` (gun mode) is read only by the shooting code (S:0x1205C–0x13086); the original sets it from option `$78B8` + port ID (S:0xD766).
  The web build replaces the option: gun mode = the player clicked the canvas (see below).

## Web input (`web/src/input.ts`)
`Input.poll()` returns the pad bits and calls `game.set_gun(gunMode, x, y, trigger, aux)` every frame (via `input.game` or `window.snatcher.game`).
Mouse position is scaled to the canvas backing size (the game's screen pixels, clamped); left button = trigger (bit 5), right button = second button
(bit 6: draw / holster the blaster); the context menu is suppressed on the canvas. `gunMode` turns on with the first mouse click on the canvas and off as
soon as a keyboard/gamepad pad input is used, so pad aiming keeps working unchanged. Scripted test inputs (`input.script`) behave as before.

## Tools (new)
- `re/oracle/interact_jump.py` – jump the ORIGINAL game into any script position and play it:
  `.venv/bin/python -I interact_jump.py --script SP04 --pc 0 --pre '[[frames,bits],..]' --until 79FE=3 --inputs '[[..]]' --shot-at 135,165 --shot-every 0 --out DIR [--watch 96D0,96D4]`.
  It loads `states/junker_hq.pkl`, runs `--wait` (300) frames to the Junker menu, writes `assets/raw/files/SPnn.BIN` to S:0x1A800, sets `$980C`, **`$9800` = pc** (the next
  pass starts there; a pending jump only applies after a pass, so a garbage first opcode crashes the VM thread), clears the menu path and sets `$79F4 = FF` so the menu loop exits.
  `--until ADDR=VAL[,..]` presses C every 60 frames until a sub word has the value (e.g. `79FE=3` keyboard, `9852=1F04` a 4-item menu is up – `$9852` is
  [cap][count]), shots/`--watch` frames are relative to the start of `--inputs`. `--poke "sub:ADDR=HEX"`, `--script ''` = stay in the loaded state (poke `$79FE` etc.).
  Menu stacks are `[cap:u8][count:u8][u16 items…]` (items start at +2, e.g. path at 0x9894).
- Engine runner extras (`engine/examples/run.rs`): `SPnn:pc[:item,item]` preset menu path (`Engine::jump_path`, items = text offsets, start at the menu root pc),
  env `POKE=frame:hexaddr:value,..`, `WATCH=hexaddr,..` (per-frame word registers), `GUN=x:y:from_frame` + `GUNB=frame:bits`.
  `--trace` logs ("text entry opened", "menu opened: n items", "pointer mode on", "shoot: play phase") give alignment points for relative-time comparisons.
- Useful scripts: keyboard/Jordan: `SP04:0` (C every 60 frames reaches "ENTER NAME"), keypad: `SP35:0`, pointer: `SP26:ba4` (+ menu: DOWN×3, C, C), shooting:
  `SP01:b:455,11,3b8` (engine) / SP01:0 + key navigation (oracle).

## Approximate / not verified (HYPOTHESIS)
- Timing: the oracle's HLE pad model delivers `in_held` only every other frame (`$7990` alternates 0x0808/0000 while a button is held; `_pad_read` TH model),
  so held-button timings of the oracle (aiming, key repeat) cannot be used as ground truth; the shot animation wait is 4 frames in the engine vs 6 in the oracle.
- Keypad/keyboard key-press animation waits time out after 180 frames; redraw latency of the original's render streams is approximated by fixed delays.
- Diagonal pointer speed (200 iterations) is fitted, single direction is exact.
- Gun calibration (`$7988/$7989`) from the title screen is not modelled (0); gun mouse coordinates are used as screen pixels.
- Not compared: shooting results/exit phases (3–5), stages ≥ 1 visually against the oracle, enemy attack/damage rules (logic ported from disassembly only),
  PCM-busy back-pressure (`$1511E`) treated as idle.
- cmd 0x15 (built-in scene + grid position) is ported but not exercised (no script reached it).
- Next: compare more stages (SP01 debug menu paths in `jump_path` form) frame-wise once the oracle pad sampling is fixed; calibrate the gun from the title module.
