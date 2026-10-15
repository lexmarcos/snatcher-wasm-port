#!/usr/bin/env python3
"""FIDELITY: play the ORIGINAL adaptively from the Junker HQ state with a menu policy and record the pad script.
usage (from re/oracle):
  .venv/bin/python -I fid_play.py --choices 0,0,3,1 --frames 20000 --out out/fid/play1.json [--wait 300] [--quiet]
Policy: whenever the command/choice menu is ready, pick the next number of --choices (0-based line, default 0 when exhausted;
negative = stop playing); whenever a message waits for confirm ($9740 == 5) press C; every other state: no input.
Writes the recorded script as [[rel_frame, padbits],..] (same convention as fid_cmp.py --inputs, rel 0 = state frame 4500 + wait)
and prints the story (menus with item texts, messages, voices, scene changes) with rel frames."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from oracle_sel import Machine
from fid_common import patch_cdtime
patch_cdtime()
ap = argparse.ArgumentParser()
ap.add_argument('--choices', default=''); ap.add_argument('--frames', type=int, default=6000)
ap.add_argument('--wait', type=int, default=300); ap.add_argument('--load', default='junker_hq')
ap.add_argument('--settle', type=int, default=12, help='frames the menu/message must have been ready before the policy presses')
ap.add_argument('--cycle', action='store_true', help='ignore --choices: take the items of every distinct menu in turn (round robin)')
ap.add_argument('--rng', action='store_true', help='poke $786A..D = 0 every frame (deterministic RANDOM nodes; pair with fid_cmp --rng)')
ap.add_argument('--jump', default='', help='start at SPnn:pc (hex pc) instead of the Junker HQ conversation (fid_common.oracle_jump; menu root items are not needed, use the leaf pc)')
ap.add_argument('--save-state', default='', help='write the machine state (pkl) at rel 0')
ap.add_argument('--stall', type=int, default=6000, help='stop after this many frames without a new queue / press / script change')
ap.add_argument('--out', default=''); ap.add_argument('--quiet', action='store_true')
ap.add_argument('--trace', default='', help='trace.jsonl path (default out/fid_play_trace.jsonl, shared by all runs)')
ap.add_argument('--stop-script', default='', help='stop when script id ($980C) equals this hex value')
a = ap.parse_args()
choices = [int(x) for x in a.choices.split(',') if x]
RAW = os.path.join(HERE, '..', '..', 'assets', 'raw', 'files')
m = Machine(trace_path=a.trace or os.path.join(HERE, 'out', 'fid_play_trace.jsonl'))
m.load_state(os.path.join(HERE, 'states', a.load + '.pkl'))
m.inputs = []
if a.jump:
    from fid_common import oracle_jump
    oracle_jump(m, a.jump, a.wait)
else:
    for _ in range(a.wait): m.step_frame(capture=False)
s = m.sub
def text(off):
    out = []
    for i in range(80):
        c = s.r8(0x1E000 + off + i)
        if c == 0xFF: break
        out.append(chr(c) if 0x20 <= c < 0x7F else ('/' if c == 0xF2 else '|' if c == 0xF6 else '~'))
    return ''.join(out)
script = []; pending = None; pending_snap = None; ecount = 0; last_act = 0; ready_since = {}; last_press = -100; ci = 0; prev_busy = 0; last_script = s.r16(0x980C); stage = {}
for rel in range(1, a.frames + 1):
    pad = 0
    busy = s.r8(0x9B44)
    if busy and not prev_busy:     # new queue
        q = [s.r16(0x9940 + 2 * i) for i in range(0, 48)]
        i = 0; desc = []
        while i < len(q) and q[i] != 0:
            c = q[i]
            n = {0x08: 2, 0x07: 1, 0x0D: 1, 0x0C: 1, 0x0E: 1, 0x10: 1, 0x12: 1, 0x1B: 3, 0x1C: 1, 0x20: 1, 0x11: 3, 0x09: 2, 0x01: 2, 0x03: 2, 0x22: 2, 0x05: 2, 0x15: 2}.get(c, 0)
            if c == 0x08: desc.append('MSG spk%d "%s"' % (q[i + 1] & 0xFF, text(q[i + 2])))
            else: desc.append('%02X%s' % (c, ('[' + ','.join('%X' % x for x in q[i + 1:i + 1 + n]) + ']') if n else ''))
            i += 1 + n
        if not a.quiet: print(rel, ' '.join(desc))
    if busy and not prev_busy: last_act = rel
    prev_busy = busy
    msg_ready = s.r8(0x9740) == 5 and s.r8(0x9720) == 0 and busy
    menu_ready = s.r8(0x9853) > 0 and s.r8(0x7A25) == 0 and s.r16(0x788C) != 0 and s.r8(0x79F4) == 0 and s.r8(0x9B44) == 0 and s.r8(0x7A00 + 1) == 1
    # text entry (keypad $79FF = 2 / keyboard 3): the key loop runs once the open effect is done (main $E061 = 0x0D + style,
    # sub copy $7892 -> S:0x11BDE/0x11BEE wait for it)
    entry_ready = s.r8(0x79FF) in (2, 3) and s.r8(0x7892) == 0x0D + s.r8(0x79FF) and s.r8(0x9B44) == 0 and s.r8(0x79F4) == 0
    if not entry_ready: ecount = 0
    for k, v in (('msg', msg_ready), ('menu', menu_ready), ('entry', entry_ready)):
        ready_since[k] = (ready_since.get(k) if ready_since.get(k) is not None else rel) if v else None
    if rel - last_press > 8 and rel > 2:
        if msg_ready and rel - ready_since['msg'] >= a.settle:
            pad = 0x20; pkind = 'm'; psince = rel - ready_since['msg']
        elif entry_ready and rel - ready_since['entry'] >= a.settle and rel - last_press > 20:
            # type keys under the cursor (C; keypad: until the direct-dial length is reached, the dial completes by itself), then OK with START (S:0x116AA)
            pad = 0x20 if ecount < (12 if s.r8(0x79FF) == 2 else 4) else 0x80; ecount += 1; pkind = 'e'; psince = rel - ready_since['entry']
            if not a.quiet: print(rel, 'ENTRY key', hex(pad))
        elif menu_ready and rel - ready_since['menu'] >= a.settle and s.r8(0x9853) > 0 and s.r8(0x7A25) == 0 and s.r16(0x788C) != 0 and s.r8(0x79F4) == 0 and s.r8(0x9B44) == 0 and s.r8(0x7A00 + 1) == 1:
            n = s.r8(0x9853); cur = s.r8(0x9710)
            if 'menu' not in stage or stage['menu'] != (rel // 1):
                if stage.get('printed') != rel:
                    items = [text(s.r16(0x9854 + 2 * i)) for i in range(n)]
                    if stage.get('items') != (items, cur) and not a.quiet: print(rel, 'MENU', items, 'cursor', cur)
                    stage['items'] = (items, cur)
            if a.cycle:
                key = tuple(text(s.r16(0x9854 + 2 * i)) for i in range(n))
                cyc = stage.setdefault('cyc', {})
                if stage.get('want') is None:
                    stage['want'] = cyc.get(key, 0) % n; cyc[key] = stage['want'] + 1
                want = stage['want']
                # never FINISH (SETSTATE(2) = back to the title, which the oracle's HLE does not run)
                its = [text(s.r16(0x9854 + 2 * i)) for i in range(n)]
                if its[want] == 'FINISH': want = next((i for i, x in enumerate(its) if x != 'FINISH'), 0); stage['want'] = want
            else:
                want = choices[ci] if ci < len(choices) else 0
            if want < 0: break
            pkind = 'u'; psince = rel - ready_since['menu']
            if cur != want: pad = 0x02
            else: pad = 0x20; ci += 1; stage['items'] = None; stage['want'] = None
    snap0 = (s.r8(0x9710), s.r8(0x79F4), s.r8(0x9B44), s.r8(0x7A25), s.r8(0x9740), s.r8(0x97F2))
    m.inputs = [[1, pad]]
    if a.rng: s.write(0x786A, bytes(4))
    m.step_frame(capture=False)
    if pad:
        last_press = rel; last_act = rel
        # the sub's pressed flags `$7991` must show the press in this frame (a press that lands in a frame where the sub
        # handler does not run, or is not delivered by the HLE pad model, is dropped from the recorded script)
        seen = s.r8(0x7991) & (pad & 0x8F | (0x20 if pad & 0x20 else 0))
        if seen: script.append([rel, pad, pkind, psince])
        else: pending = (rel, pad, pkind, psince); pending_snap = snap0       # the flag may show up a frame or two later (stalled sub frames latch the press)
    if pending and not pad:
        if rel - pending[0] <= 3:
            snap = (s.r8(0x9710), s.r8(0x79F4), s.r8(0x9B44), s.r8(0x7A25), s.r8(0x9740), s.r8(0x97F2))
            if s.r8(0x7991) & (pending[1] & 0x8F | (0x20 if pending[1] & 0x20 else 0)) or snap != pending_snap:
                script.append(list(pending)); pending = None      # the press counts when it changed the menu/message/entry state (the flag is not always visible after the step)
        else: print(pending[0], 'DROPPED press', hex(pending[1])); pending = None
    sc = s.r16(0x980C)
    if sc != last_script:
        if not a.quiet: print(rel, 'SCRIPT', hex(last_script), '->', hex(sc))
        last_script = sc; last_act = rel
        if a.stop_script and sc == int(a.stop_script, 16): break
    if rel - last_act > a.stall: print(rel, 'STALL (no activity for', a.stall, 'frames)'); break
    if m.main.halted or m.sub.halted: print('HALT'); break
if a.out:
    json.dump(script, open(a.out, 'w'))
print('presses', len(script), 'last rel', script[-1][0] if script else 0)
