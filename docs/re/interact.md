# INTERACT workstream – menus, text entry, pointer mode, shooting, light gun

Code: `engine/src/menus.rs`, `engine/src/shoot.rs` (ports of S:0x10846–0x11E28, S:0x1449A,
S:0x1208E–0x1314E). Hooks (uncommitted WIP in game.rs/lib.rs): `ix` field on Engine,
`menus::frame_hook` in tick, `open_menu_hook`/`menu_frame_hook`/`list_cursor`/`list_close`
in the menu path, seq cmd 0x15 → `menus::cmd15`, 0x18 → `shoot::cmd18`, `Game::set_gun`.

## Status (WIP, stopped on quota)
- Builds and runs in a scratch copy with stub modules; text entry, pointer mode and shooting
  are ported but NOT yet verified by play (no runs reached keypad/keyboard/shooting scenes).
- Menu highlight bar: object 0 (init list S:0xED0E, tiles 0x6F8 from S:0x178F8) is positioned
  (X 0x10, Y 0x8F+9*line) but stays hidden: something sets its +$25 flags back to 3 after
  `list_cursor` – find it (next step), then compare with re/oracle/out/refs/junker_menu.png.
- Next: debug the bar flags, then use SP01:0 (debug menu) to reach a phone scene (keypad),
  SP05 "ENTER NAME" (keyboard, `$971A` two-step match) and MINIGAME stages; compare with oracle.
- HYPOTHESES: gun $798A/$798B ≈ screen pixels after calibration; pointer mode speed
  (`PTR_STEPS` = 200 iterations/frame); PCM-busy `$1511E` treated as idle; key-press
  animation waits time out after 180 frames.
