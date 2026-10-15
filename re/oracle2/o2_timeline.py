#!/usr/bin/env python3
"""Game-event timeline of one or two traces (HLE oracle trace.jsonl and/or oracle2 trace.jsonl) aligned on game events.
Events: first frame of each (sp_state), (state $9808) and script ($980C) value change, CD-DA start/stop (music), cd file reads.
usage: o2_timeline.py hle_trace.jsonl [oracle2_trace.jsonl] [--max N]
Frames are relative to the game start (HLE: frame 1 = IP entry; oracle2: its 'boot' event frame t0)."""
import json, sys

def load(path):
    t0 = 0; ev = []; prev = {}
    for l in open(path):
        e = json.loads(l)
        t = e['t']
        if t == 'boot': t0 = e['f']
        elif t == 'frame':
            for k in ('sp_state', 'state', 'script'):
                if prev.get(k) != e[k]:
                    if prev.get(k) is not None or e[k] != 0:
                        ev.append((e['f'], '%s=%d' % (k, e[k])))
                    prev[k] = e[k]
        elif t == 'music':
            fn = e['fn']
            if fn.startswith('MSCPLAY'): ev.append((e['f'], 'music play %s' % e.get('track', '')))
            elif fn in ('MSCPAUSEON', 'MSCSTOP'): ev.append((e['f'], 'music stop'))
    return t0, ev

def main():
    args = sys.argv[1:]
    if '--max' in args: i = args.index('--max'); del args[i:i + 2]
    paths = args
    mx = int(sys.argv[sys.argv.index('--max') + 1]) if '--max' in sys.argv else 10**9
    loaded = [load(p) for p in paths]
    if len(loaded) == 1:
        t0, ev = loaded[0]
        for f, lab in ev[:mx]: print('%6d  %s' % (f - t0, lab))
        return
    (ta, ea), (tb, eb) = loaded
    # align by label: for each label in a find the next occurrence in b (in order)
    j = 0
    rows = []
    for f, lab in ea:
        k = j
        while k < len(eb) and eb[k][1] != lab: k += 1
        if k < len(eb):
            rows.append((f - ta, lab, eb[k][0] - tb)); j = k + 1
        else:
            rows.append((f - ta, lab, None))
    print('%7s %-34s %7s %7s' % ('HLE', 'event', 'oracle2', 'delta'))
    for a, lab, b in rows[:mx]:
        print('%7d %-34s %7s %7s' % (a, lab, '-' if b is None else b, '' if b is None else b - a))

main()
