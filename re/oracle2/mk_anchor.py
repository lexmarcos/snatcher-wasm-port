#!/usr/bin/env python3
"""Reach the Junker HQ command menu on oracle2 (event driven, real BIOS) and write the anchor save state states/junker_hq.state
at 300 frames before the first ready command menu (the fid_* tools run 300 idle frames after loading the anchor, like the HLE's
states/junker_hq.pkl). Also writes states/title.state (title ready) and a log of the reference-point frames.
usage: mk_anchor.py [--out out/anchor]"""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from o2lib import Machine2
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument('--out', default=os.path.join(HERE, 'out', 'anchor'))
ap.add_argument('--gun', action='store_true')
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True); os.makedirs(os.path.join(HERE, 'states'), exist_ok=True)

def run(save_at=None):
    m = Machine2(trace_path=os.path.join(a.out, 'trace.jsonl') if save_at is not None else None, gun=a.gun)
    t0 = m.boot()
    s = m.sub
    st = {'last_c': 0, 'phase': 0, 'last_press': -100, 'since': 0, 'mod_f': None, 'ready': {}}
    marks = {}
    menu_f = None
    def pressed(bits, n=1):
        m.inputs = [[n, bits]]
    f = 0
    while True:
        f += 1
        m.inputs = []
        r7896, r7842 = s.r16(0x7896), s.r16(0x7842)
        busy = s.r8(0x9B44)
        msg_ready = s.r8(0x9740) == 5 and s.r8(0x9720) == 0 and busy
        menu_ready = s.r8(0x9853) > 0 and s.r8(0x7A25) == 0 and s.r16(0x788C) != 0 and s.r8(0x79F4) == 0 and s.r8(0x9B44) == 0 and s.r8(0x7A00 + 1) == 1
        for k, v in (('title', r7842 == 1 and r7896 == 9), ('options', r7896 == 0xB), ('intro', r7842 == 2), ('game', r7842 == 5),
                     ('msg', msg_ready), ('menu', menu_ready)):
            st['ready'][k] = (st['ready'].get(k) if st['ready'].get(k) is not None else f) if v else None
        rd = st['ready']
        if st['phase'] == 0 and rd['title'] is not None and f - rd['title'] >= 20:
            pressed(128); st['phase'] = 1; marks['title_start'] = m.frame
        elif st['phase'] == 1 and rd['options'] is not None and f - rd['options'] >= 150:
            pressed(128); st['phase'] = 2; marks['options_start'] = m.frame
        elif st['phase'] == 2 and rd['intro'] is not None and f - rd['intro'] == 580:
            pressed(32); st['phase'] = 3
        elif st['phase'] == 3 and rd['intro'] is not None and f - rd['intro'] == 580 + 401:
            pressed(128); st['phase'] = 4
        elif st['phase'] == 4 and rd['game'] is not None:
            # C on every message that waits for confirm, until the command menu is up
            if rd['menu'] is not None and f - rd['menu'] >= 12:
                if save_at is None: marks['last_c'] = st['last_c']; return m.frame, marks
            elif rd['msg'] is not None and f - rd['msg'] >= 12 and f - st['last_press'] > 8:
                pressed(32); st['last_press'] = f; st['last_c'] = m.frame + 1
        if save_at is not None and m.frame + 1 == save_at:
            m.save_state(os.path.join(HERE, 'states', 'junker_hq.state'))
            return m.frame, marks
        m.step_frame()
        if f > 20000: raise RuntimeError('no menu')

mf, marks = run()
print('command menu ready (first, +12 settle) at oracle2 frame', mf, marks)
save_at = mf - 12 - 250 - 50      # menu_ready is first true ~12 frames before this point; the HLE needs ~250 frames after the anchor
# find the first menu_ready frame precisely: mf is that + 12
first = mf - 12
save_at = max(first - 250, marks['last_c'] + 3)   # after the last C (the message that precedes the menu must be dismissed)
f2, _ = run(save_at)
print('saved states/junker_hq.state at frame', f2, '(menu first ready at', first, ')')
