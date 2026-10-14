# FIDELITY workstream – engine vs ORIGINAL (oracle) through Act 1

## Status
(updated as the loop proceeds; see "Findings" for evidence)
- Harness: `re/oracle/fid_cmp.py` (queues per VM pass + screen diffs), `fid_watch.py` (per-frame watch of original memory), `fid_sheet.py`.
- Junker HQ command menu / message box open/close timing and the wait-for-confirm arrow now match the oracle within 1 frame (see below).

## Tools
- `cd re/oracle && .venv/bin/python -I fid_cmp.py --name NAME --inputs '[[rel_frame,bits],..]' --shots 100,200 [--every K]`
  Oracle = `states/junker_hq.pkl` (+300 idle frames, the 3-item command menu is up), engine = new game, 1500: press C (dismisses the first Mika message),
  rel 0 = engine frame 1530. Output `out/fid/NAME/{oracle,engine,diff}/rNNNNN.png` (diff = oracle | engine | |diff|), `report.txt`.
  Queue comparison drops the oracle's menu-draw passes (cmd 22/16/05 only, `1C 1D`) – the engine draws menus natively.
- `fid_watch.py --inputs .. --frames N --watch s:7892,m:E061 [--changes]` prints sub (`s:`) / main (`m:`) memory per frame (line n = state after frame n,
  the pad of rel frame n is applied in frame n).

## Findings
