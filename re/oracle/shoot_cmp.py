#!/usr/bin/env python3
"""SHOOTING fidelity: compare two per-frame traces (oracle: shoot_rec.py, engine: snengine example shoot_trace).
usage: python3 -I shoot_cmp.py ORACLE.jsonl ENGINE.jsonl [--fields 78E8,78EA,...] [--objs] [--from N] [--lag K] [--all]
Reports, per register word / object field, the first frame of divergence and the number of differing frames; with --lag K the engine
trace is compared shifted by K frames (engine frame f+K vs oracle frame f) to tell timing offsets from logic differences."""
import argparse, json, sys

NAMES = {'78E0': 'stage', '78E2': 'speed', '78E4': 'tmr', '78E6': 'tmr2', '78E8': 'phase', '78EA': 'sub', '78EC': 'active', '78EE': 'practice',
         '78F2': 'lock', '78F8': 'drawn', '78FA': 'shot', '78FC': 'spawn', '78FE': 'atk', '7900': 'aim', '7906': 'spawns', '7908': 'shots',
         '790A': 'kills', '790C': 'hits', '790E': 'miss', '7910': 'life', '7912': 'tog', '7914': 'civ', '7916': 'flag', '791A': 'atk_cell',
         '791C': 'sp_slot', '7944': 'sp_tmr', '7946': 'recoil', '7970': 'gunrdy', '7972': 'gunmode'}
OBJ = ['active', 'flags', 'x', 'y', 'idx', 'f17', 'f24', 'f2c']


def load(p):
    return [json.loads(l) for l in open(p)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('oracle'); ap.add_argument('engine')
    ap.add_argument('--fields', default='')
    ap.add_argument('--objs', action='store_true')
    ap.add_argument('--lag', type=int, default=0)
    ap.add_argument('--from', dest='frm', type=int, default=0)
    ap.add_argument('--n', type=int, default=6, help='first N divergences per field')
    ap.add_argument('--min-run', type=int, default=4, help='only differences that persist for N consecutive frames count (sub-CPU stalls inside a tick shift single values by 1-2 frames)')
    ap.add_argument('--skip', default='7984,7986,7988,798A,7990,7992,7902,7904', help='words not compared')
    a = ap.parse_args()
    o, e = load(a.oracle), load(a.engine)
    n = min(len(o), len(e) - a.lag)
    fields = a.fields.split(',') if a.fields else [k for k in o[0]['w'].keys() if k not in a.skip.split(',')]
    tot = 0

    def runs(diffs):
        """keep only runs of >= min-run consecutive differing frames (first element of each run)"""
        out, cur = [], []
        for d in diffs + [(10 ** 9, 0, 0)]:
            if cur and d[0] != cur[-1][0] + 1:
                if len(cur) >= a.min_run:
                    out.extend(cur)
                cur = []
            cur.append(d)
        return out

    for k in fields:
        diffs = runs([(f, o[f]['w'][k], e[f + a.lag]['w'][k]) for f in range(a.frm, n) if o[f]['w'][k] != e[f + a.lag]['w'][k]])
        if diffs:
            tot += 1
            print('%s %-8s %4d frames differ, first: %s' % (k, NAMES.get(k, ''), len(diffs), ' '.join('f%d:%x/%x' % d for d in diffs[:a.n])))
    if a.objs:
        for s in o[0]['o']:
            for i, fld in enumerate(OBJ):
                diffs = runs([(f, o[f]['o'][s][i], e[f + a.lag]['o'][s][i]) for f in range(a.frm, n) if o[f]['o'][s][i] != e[f + a.lag]['o'][s][i]])
                if diffs:
                    tot += 1
                    print('obj %2s %-6s %4d frames differ, first: %s' % (s, fld, len(diffs), ' '.join('f%d:%x/%x' % d for d in diffs[:a.n])))
    # summary: phase timeline and final counters of both sides
    def tl(tr):
        out, last = [], None
        for r in tr:
            v = r['w']['78E8']
            if v != last:
                out.append('f%d:%x' % (r['f'], v)); last = v
        return ' '.join(out)

    def fin(tr):
        w = tr[-1]['w']
        return 'shots=%d hits=%d miss=%d kills=%d civ=%d life=%d af=%x' % (w['7908'], w['790C'], w['790E'], w['790A'], w['7914'], w['7910'], w['78AF'])
    def snds(tr):
        return [(r['f'], x[0], x[1]) for r in tr for x in r.get('s', [])]
    so, se = snds(o), snds(e)
    bad = next((i for i, (a, b) in enumerate(zip(so, se)) if a[1:] != b[1:]), None)
    dmax = max([abs(a[0] - b[0]) for a, b in zip(so, se) if a[1:] == b[1:]] or [0])
    print('sounds: oracle %d engine %d, first sequence mismatch %s, max frame delta %d' % (len(so), len(se), (so[bad], se[bad]) if bad is not None else None, dmax))
    print('phases oracle:', tl(o)); print('phases engine:', tl(e))
    print('final oracle:', fin(o)); print('final engine:', fin(e))
    print('compared %d frames, %d differing fields' % (n - a.frm, tot))


if __name__ == '__main__':
    main()
