"""SHOOTING fidelity helpers: drive the ORIGINAL into any shooting stage through the SP01 debug menu
("SHOOTING DEBUG" -> ACT1/ACT2/ACT3 SHOOTING -> stage list), then run pad/gun scripts and record
per-frame state.  usage from re/oracle:  .venv/bin/python -I shoot_common.py --group 0 --item 0 --save out/shoot/st0.pkl

Menu navigation uses 3-frame presses with a 25-frame gap (a 1-frame press is sometimes lost by the VM thread)."""
import os, sys, json
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from machine import Machine

DOWN, UP, LEFT, RIGHT, B, C, A, START = 2, 1, 4, 8, 16, 32, 64, 128
ROOT = os.path.join(HERE, '..', '..')


def new_machine(trace=None):
    m = Machine(trace_path=trace)
    m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
    m.inputs = []
    for _ in range(300):
        m.step_frame()
    return m


def jump(m, script, pc=0):
    """same as interact_jump.py: write SPnn.BIN into the script area and set the pending jump."""
    s = m.sub
    data = open(os.path.join(ROOT, 'assets', 'raw', 'files', 'SP%02d.BIN' % script), 'rb').read()
    s.write(0x1A800, data[:0xD000])
    s.w16(0x980C, 57 + script)
    s.w16(0x9800, pc); s.w16(0x9802, 0xFFFF)
    s.w8(0x9893, 0); s.w8(0x9841, 0)
    s.w8(0x79F4, 0xFF)
    for a in (0x78E4, 0x78E6, 0x78E8, 0x78EA):   # stale shooting state of the saved game (op_42 sub 4 clears them anyway)
        s.w16(a, 0)


def run(m, n, bits=0):
    for _ in range(n):
        m.inputs = [[1, bits]]
        m.step_frame()


def press(m, bits, hold=3, gap=25):
    run(m, hold, bits)
    run(m, gap, 0)


def wait_for(m, addr, val, limit=3000, nudge=None):
    k = 0
    while m.sub.r16(addr) != val and k < limit:
        run(m, 1, nudge if nudge and k % 60 == 30 else 0)
        k += 1
    return k


def cur_cmd(m):
    """sequencer command currently executed (queue at S:$9940, index $9B40)."""
    s = m.sub
    return s.r16(0x9940 + s.r16(0x9B40))


def to_debug_menu(m):
    jump(m, 1, 0)
    run(m, 100)
    press(m, C, gap=60)          # open command menu
    return m


def start_stage(m, group, item, log=print):
    """SP01 debug menu: SHOOTING (6th entry) -> message -> ACT{1,2,3} SHOOTING -> stage item.
    Returns once the stage's setup has begun ($78AF != 0)."""
    to_debug_menu(m)
    for _ in range(5):
        press(m, DOWN)
    press(m, C, gap=80)           # SHOOTING DEBUG. message
    press(m, C, gap=80)           # act menu
    for _ in range(group):
        press(m, DOWN)
    press(m, C, gap=100)          # stage list
    for _ in range(item):
        press(m, DOWN)
    run(m, 3, C)
    k = 0
    # dismiss the intro message(s) until the shooting sequencer (cmd 0x18) has taken its first step
    while cur_cmd(m) != 0x18 and k < 1500:
        run(m, 1, C if k % 50 == 49 else 0); k += 1
    log('stage started after %d frames: $78E0=%04x $78E8=%x $78EA=%x' % (k, m.sub.r16(0x78E0), m.sub.r16(0x78E8), m.sub.r16(0x78EA)))
    return k


if __name__ == '__main__':
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument('--group', type=int, default=0)
    ap.add_argument('--item', type=int, default=0)
    ap.add_argument('--save', default='')
    a = ap.parse_args()
    m = new_machine()
    start_stage(m, a.group, a.item)
    if a.save:
        os.makedirs(os.path.dirname(os.path.abspath(a.save)), exist_ok=True)
        m.save_state(a.save)
