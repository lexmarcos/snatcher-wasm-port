#!/usr/bin/env python3
"""Compare the sequencer queues (seq_start events) of two traces (HLE oracle vs oracle2, or any two): difflib alignment, divergences and timing drift.
usage: o2_cmpq.py trace_a.jsonl trace_b.jsonl [--ctx 3] [--t0a N] [--t0b N] [--keep-menu]
Frames are relative to the first queue of each trace. Menu redraw passes (first command 0x22/0x16/0x05/0x23 or [1C 1D]) are dropped like in fid_cmp.py
unless --keep-menu. Prints: matching blocks, A-only / B-only passes with frame and queue, and per matched pass the delta (B - A) and its change."""
import difflib, json, sys
def load(p, keep):
    out = []
    for l in open(p):
        d = json.loads(l)
        if d['t'] != 'seq_start': continue
        hx = d['queue']; w = [int(hx[i:i + 4], 16) for i in range(0, len(hx), 4)]
        while w and w[-1] == 0: w.pop()
        if not keep and (len(w) == 0 or w[0] in (0x22, 0x16, 0x05, 0x23) or w == [0x1C, 0x1D]): continue
        out.append((d['f'], w))
    return out
args = [x for x in sys.argv[1:] if not x.startswith('--')]
keep = '--keep-menu' in sys.argv
A, B = load(args[0], keep), load(args[1], keep)
ctx = 3
if '--ctx' in sys.argv: ctx = int(sys.argv[sys.argv.index('--ctx') + 1])
fa0 = A[0][0] if A else 0; fb0 = B[0][0] if B else 0
sm = difflib.SequenceMatcher(None, [tuple(w) for _, w in A], [tuple(w) for _, w in B], autojunk=False)
blocks = [m for m in sm.get_matching_blocks() if m.size]
print('queues: A %d, B %d, matching %d (%d blocks)' % (len(A), len(B), sum(m.size for m in blocks), len(blocks)))
def fmt(w): return ' '.join('%04X' % x for x in w[:14]) + (' ...' if len(w) > 14 else '')
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == 'equal': continue
    print('-- %s: A[%d:%d] B[%d:%d]' % (tag, i1, i2, j1, j2))
    for k in range(i1, i2): print('   A-only  f=%6d  %s' % (A[k][0] - fa0, fmt(A[k][1])))
    for k in range(j1, j2): print('   B-only  f=%6d  %s' % (B[k][0] - fb0, fmt(B[k][1])))
# drift
rows = []
for m in blocks:
    for k in range(m.size):
        fa, fb = A[m.a + k][0] - fa0, B[m.b + k][0] - fb0
        rows.append((fa, fb, fb - fa, A[m.a + k][1]))
if rows:
    print('matched passes %d; first delta %d, last delta %d (B-A), A span %d frames, B span %d frames' % (len(rows), rows[0][2], rows[-1][2], rows[-1][0], rows[-1][1]))
    if '--drift' in sys.argv:
        prev = rows[0][2]
        for fa, fb, d, w in rows:
            if d != prev: print('   f_A %6d f_B %6d delta %5d (%+d)  %s' % (fa, fb, d, d - prev, fmt(w))); prev = d
