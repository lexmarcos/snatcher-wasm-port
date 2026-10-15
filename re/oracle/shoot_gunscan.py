#!/usr/bin/env python3
"""SHOOTING / light gun: scans of the ORIGINAL with the emulated Justifier (re/oracle/machine.py `m.gun`).
usage (from re/oracle):  .venv/bin/python -I shoot_gunscan.py raw      # raw X/Y the main CPU reports per aimed pixel (M:0xFF148A / 0xFF14E0)
                         .venv/bin/python -I shoot_gunscan.py cells    # shooting cell per aimed pixel after calibrating on the bullseye (128,101)
                         .venv/bin/python -I shoot_gunscan.py cal      # title screen GUN ADJUST: calibrate and print $7988/$7989, values and crosshair
Results are quoted in docs/re/shooting.md ("Light gun")."""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from shoot_common import *


def gun_stage():
    m = new_machine()
    m.gun = {'x': 128, 'y': 101, 'trig': 0, 'aux': 0}
    m.sub.w16(0x78B8, 1)
    return m


def ready(m):
    start_stage(m, 0, 0, log=lambda *a: None)
    run(m, 150); m.gun['aux'] = 1; run(m, 3); m.gun['aux'] = 0; run(m, 10)


def raw():
    m = gun_stage(); ready(m); s = m.sub
    print('x:   ' + ' '.join('%3d' % x for x in range(0, 256, 16)))
    for y in (20, 60, 100, 150):
        row = []
        for x in range(0, 256, 16):
            m.gun['x'], m.gun['y'] = x, y
            run(m, 3)
            row.append('%3d/%3d' % (s.r8(0x7986), s.r8(0x7987)))
        print('y%3d ' % y, ' '.join(row))


def cells():
    m = gun_stage(); s = m.sub
    import os
    cal = os.environ.get("GUNCAL", "C0,E6").split(","); s.w8(0x7988, int(cal[0], 16)); s.w8(0x7989, int(cal[1], 16))       # what the calibration at the bullseye stores (see `cal`; HLE C0/E6, oracle2 00/E5)
    ready(m)

    def cell_at(x, y):
        m.gun['x'], m.gun['y'] = x, y
        m.gun['trig'] = 1; run(m, 1); m.gun['trig'] = 0
        run(m, 8)
        return s.r16(0x7900)
    print('x:    ' + ' '.join('%3d' % x for x in range(80, 256, 16)))
    for y in range(0, 160, 8):
        print('y%3d  ' % y, ' '.join('%3d' % cell_at(x, y) for x in range(80, 256, 16)))


def cal():
    m = Machine(); m.load_state(os.path.join(HERE, 'states', 'title.pkl')); m.inputs = []
    s = m.sub
    m.comcmd[1] = (m.comcmd[1] & 0xFF00) | 1; m.pad2_ctrl = 0xB0       # the gun was detected at power-up
    m.gun = {'x': 128, 'y': 101, 'trig': 0, 'aux': 0}

    def go(n, bits=0):
        for _ in range(n):
            run(m, 1, bits)
    go(10); go(1, START); go(120)
    go(1, RIGHT); go(20)                                                 # row 0: light gun on ($78B8 = 1)
    for _ in range(3):
        go(1, DOWN); go(19)                                              # row 3: gun calibration
    go(1, START); go(60)
    print('calibration screen: state %d sub %d step %d' % (s.r16(0x7896), s.r16(0x7898), s.r16(0x784E)))
    for _ in range(3):
        m.gun['trig'] = 1; go(1)
    m.gun['trig'] = 0; go(30)
    for _ in range(3):
        m.gun['trig'] = 1; go(1)                                         # second trigger: test shooting (the crosshair follows the gun)
    m.gun['trig'] = 0; go(10)
    print('raw $7986/7 = %02X/%02X, calibration $7988/9 = %02X/%02X, value $798A/B = %02X/%02X' % (
        s.r8(0x7986), s.r8(0x7987), s.r8(0x7988), s.r8(0x7989), s.r8(0x798A), s.r8(0x798B)))
    for (x, y) in ((128, 101), (100, 80), (160, 120), (200, 60)):
        m.gun['x'], m.gun['y'] = x, y; go(4)
        m.gun['trig'] = 1; go(1); m.gun['trig'] = 0; go(3)             # a test shot moves the crosshair (S:0x299B2)
        o = 0x7A00 + 16 * 0x50
        print('aim (%3d,%3d): value %02X/%02X, crosshair object at (%d, %d)' % (x, y, s.r8(0x798A), s.r8(0x798B), s.r16(o + 6), s.r16(o + 0xA)))


if __name__ == '__main__':
    {'raw': raw, 'cells': cells, 'cal': cal}[sys.argv[1] if len(sys.argv) > 1 else 'cal']()
