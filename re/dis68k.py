#!/usr/bin/env python3
"""Recursive-descent 68000 disassembler for Snatcher RE (capstone backend).

Why: Ghidra's 32-bit 68000 model does not mirror 24-bit addresses (code calls
$FF4372 as `jsr $4372.w`), and its jump-table recovery misses Konami's tables.
This tool masks every address to 24 bits, follows code from labelled entry
points and annotated jump tables, and prints a full listing (code + data).

usage: dis68k.py <image> <base-hex> <labels.txt[,more.txt,glob*.txt]> [-t tables.txt] [-o out.asm]
labels.txt lines: ADDR NAME [f]        (f = function entry / code seed)
tables.txt lines: ADDR KIND COUNT [BASE] [NAME]
   KIND w_rel   : COUNT words, target = BASE(default ADDR) + sext(word)
        l_abs   : COUNT longs, absolute targets
        jmp     : COUNT x 6-byte `jmp abs.l` / 4-byte `bra.w` instructions (code)
        code    : seed code at ADDR (COUNT ignored)
"""
import re, sys, argparse, collections
import capstone

M = 0xFFFFFF


def load_labels(paths):
    labels, funcs = {}, set()
    if not paths:
        return labels, funcs
    import glob as _g
    files = []
    for p in paths.split(','):
        files += sorted(_g.glob(p)) or [p]
    lines = []
    for f in files:
        lines += open(f).read().splitlines()
    for line in lines:
        line = line.split('#', 1)[0].strip()
        if not line:
            continue
        p = line.split()
        a = int(p[0], 16) & M
        labels[a] = p[1]
        if len(p) > 2 and p[2] == 'f':
            funcs.add(a)
    return labels, funcs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('image'); ap.add_argument('base'); ap.add_argument('labels')
    ap.add_argument('-t', '--tables'); ap.add_argument('-o', '--out')
    ap.add_argument('--entries', default='')
    a = ap.parse_args()
    img = open(a.image, 'rb').read()
    base = int(a.base, 16)
    end = base + len(img)
    labels, funcs = load_labels(a.labels)
    md = capstone.Cs(capstone.CS_ARCH_M68K, capstone.CS_MODE_BIG_ENDIAN | capstone.CS_MODE_M68K_000)

    def inimg(x):
        return base <= x < end

    def rd16(x):
        o = x - base
        return (img[o] << 8) | img[o + 1]

    def rd32(x):
        return (rd16(x) << 16) | rd16(x + 2)

    insns = {}           # addr -> (size, mnem, ops, targets)
    owner = {}           # byte addr -> insn addr
    work = collections.deque()
    xrefs = collections.defaultdict(set)
    tables = []          # (addr, kind, count, tbase, name)
    unresolved = []

    def seed(x, frm=None, call=False):
        x &= M
        if not inimg(x):
            return
        if frm is not None:
            xrefs[x].add(frm)
        if call:
            funcs.add(x)
        if x not in insns:
            work.append(x)

    for x in funcs.copy():
        seed(x)
    for e in a.entries.split(','):
        if e:
            seed(int(e, 16), call=True)

    if a.tables:
        for line in open(a.tables):
            line = line.split('#', 1)[0].strip()
            if not line:
                continue
            p = line.split()
            t = int(p[0], 16) & M
            kind = p[1]; cnt = int(p[2]) if len(p) > 2 else 0
            tb = int(p[3], 16) & M if len(p) > 3 and p[3] != '-' else t
            name = p[4] if len(p) > 4 else 'tbl_%06x' % t
            tables.append((t, kind, cnt, tb, name))
            labels.setdefault(t, name)
            if kind == 'code':
                seed(t, call=True); continue
            for i in range(cnt):
                if kind == 'w_rel':
                    v = rd16(t + 2 * i); v = v - 0x10000 if v & 0x8000 else v
                    tgt = (tb + v) & M
                elif kind == 'l_abs':
                    tgt = rd32(t + 4 * i) & M
                elif kind == 'jmp':
                    tgt = None
                    seed(t + 6 * i, call=True)
                    continue
                else:
                    raise SystemExit('bad table kind ' + kind)
                labels.setdefault(tgt, '%s_%d' % (name, i))
                seed(tgt, t, call=True)

    tgt_re = re.compile(r'\$([0-9a-f]+)(\.w|\.l|\(pc\))?$')

    def target(ops):
        m = tgt_re.search(ops.strip())
        if not m:
            return None
        v = int(m.group(1), 16)
        if m.group(2) == '.w' and v & 0x8000 and v < 0x10000:
            v |= 0xFFFF0000
        return v & M

    pcidx_re = re.compile(r'\$([0-9a-f]+)\(pc, [ad]\d\.[wl]\)')
    pend = None
    auto_tables = []

    def harvest_l(t, frm):
        n = 0
        while inimg(t + 4 * n + 3) and n < 256 and (t + 4 * n) not in insns:
            v = rd32(t + 4 * n)
            if (v >> 24) not in (0, 0xFF) or not inimg(v & M) or (v & 1):
                break
            n += 1
        for k in range(n):
            tgt = rd32(t + 4 * k) & M
            labels.setdefault(tgt, 'tbl%06x_%d' % (t, k))
            seed(tgt, frm, call=True)
        auto_tables.append((t, 'l', n, frm))
        labels.setdefault(t, 'jtbl_%06x' % t)

    def harvest_w(t, y, frm):
        n = 0; lo = 1 << 30
        while inimg(t + 2 * n + 1) and n < 256 and (t + 2 * n) < lo and (t + 2 * n) not in insns:
            v = rd16(t + 2 * n); v = v - 0x10000 if v & 0x8000 else v
            tgt = (y + v) & M
            if not inimg(tgt) or tgt & 1:
                break
            lo = min(lo, tgt) if tgt > t else lo
            n += 1
        for k in range(n):
            v = rd16(t + 2 * k); v = v - 0x10000 if v & 0x8000 else v
            tgt = (y + v) & M
            labels.setdefault(tgt, 'tbl%06x_%d' % (t, k))
            seed(tgt, frm)
        auto_tables.append((t, 'w', n, frm))
        labels.setdefault(t, 'jtbl_%06x' % t)

    def harvest_bra(t, frm):
        n = 0; z = t
        while inimg(z) and n < 256:
            d = list(md.disasm(img[z - base:z - base + 6], z, count=1))
            if not d or not (d[0].mnemonic.startswith('bra') or d[0].mnemonic == 'jmp'):
                break
            labels.setdefault(z, 'btbl%06x_%d' % (t, n))
            seed(z, frm)
            z += d[0].size; n += 1
        auto_tables.append((t, 'bra', n, frm))

    while work:
        x = work.popleft()
        while inimg(x) and x not in insns:
            if x in owner:  # overlaps another instruction
                unresolved.append((x, 'overlap'))
                break
            o = x - base
            dis = list(md.disasm(img[o:o + 10], x, count=1))
            if not dis:
                unresolved.append((x, 'invalid'))
                break
            i = dis[0]
            mn, ops = i.mnemonic, i.op_str
            if mn.startswith('dc.'):
                unresolved.append((x, 'invalid'))
                break
            m = pcidx_re.search(ops)
            if m and mn.startswith('movea.l'):
                pend = ('l', int(m.group(1), 16) & M)
            elif m and mn.startswith('move.w') and ops.split(',')[-1].strip().startswith('d'):
                pend = ('w', int(m.group(1), 16) & M)
            elif m and mn in ('jmp', 'jsr'):
                y = int(m.group(1), 16) & M
                if pend and pend[0] == 'w':
                    harvest_w(pend[1], y, x)
                else:
                    harvest_bra(y, x)
            elif mn in ('jmp', 'jsr') and pend and pend[0] == 'l' and re.match(r'^\(a\d\)$', ops.strip()):
                harvest_l(pend[1], x)
            if not m and mn not in ('jmp', 'jsr') and not mn.startswith(('add', 'lsl', 'asl')):
                pend = pend if mn.startswith(('movea', 'move.w')) else None
            insns[x] = (i.size, mn, ops)
            for k in range(i.size):
                owner[x + k] = x
            nxt = x + i.size
            if mn in ('rts', 'rte', 'rtr', 'illegal', 'stop'):
                break
            if mn.startswith('bra') or mn == 'jmp':
                t = target(ops) if '(a' not in ops and ',' not in ops.replace('(pc)', '') else None
                if t is not None:
                    seed(t, x)
                else:
                    unresolved.append((x, mn + ' ' + ops))
                break
            if mn.startswith('bsr') or mn == 'jsr':
                t = target(ops) if '(a' not in ops and ',' not in ops else None
                if t is not None:
                    seed(t, x, call=True)
                else:
                    unresolved.append((x, mn + ' ' + ops))
            elif mn.startswith('db') or (mn.startswith('b') and mn[1:3] in (
                    'eq', 'ne', 'cs', 'cc', 'mi', 'pl', 'vs', 'vc', 'hi', 'ls', 'ge', 'lt', 'gt', 'le')):
                t = target(ops)
                if t is not None:
                    seed(t, x)
            x = nxt

    # ---- output ----
    def lab(v):
        return labels.get(v) or (('sub_%06x' % v) if v in funcs else ('loc_%06x' % v) if v in xrefs and v in insns else None)

    def subst(ops):
        def rep(m):
            v = int(m.group(1), 16)
            suf = m.group(2) or ''
            vv = v
            if suf == '.w' and v & 0x8000 and v < 0x10000:
                vv = v | 0xFFFF0000
            n = lab(vv & M)
            return (n + ('' if suf in ('(pc)',) else suf)) if n else m.group(0)
        return re.sub(r'\$([0-9a-f]{3,8})(\.w|\.l|\(pc\))?', rep, ops)

    out = open(a.out, 'w') if a.out else sys.stdout
    x = base
    data_run = []

    def flush():
        if not data_run:
            return
        s = data_run[0]
        for k in range(0, len(data_run), 16):
            chunk = bytes(img[s - base + k: s - base + k + min(16, len(data_run) - k)])
            asc = ''.join(chr(c) if 32 <= c < 127 else '.' for c in chunk)
            out.write('  %06x  dc.b %-48s ; %s\n' % (s + k, chunk.hex(' '), asc))
        data_run.clear()

    while x < end:
        n = lab(x)
        if n and (x in insns or x in labels):
            flush()
            refs = xrefs.get(x)
            out.write('\n%s:%s\n' % (n, ('    ; xrefs ' + ' '.join('%06x' % r for r in sorted(refs)[:8]) + (' ...' if len(refs) > 8 else '')) if refs else ''))
        if x in insns:
            flush()
            sz, mn, ops = insns[x]
            hx = img[x - base:x - base + sz].hex()
            out.write('  %06x  %-20s %-8s %s\n' % (x, hx, mn, subst(ops)))
            x += sz
        else:
            if data_run and (x - data_run[0]) % 16 == 0 and False:
                pass
            data_run.append(x)
            if len(data_run) >= 4096:
                flush()
            x += 1
    flush()
    sys.stderr.write('insns %d funcs %d unresolved %d\n' % (len(insns), len(funcs), len(unresolved)))
    with open((a.out or 'dis') + '.unresolved', 'w') as u:
        for (ux, why) in sorted(set(unresolved)):
            u.write('%06x %s\n' % (ux, why))
    with open((a.out or 'dis') + '.tables', 'w') as fo:
        for t in sorted(auto_tables):
            fo.write('%06x %s %d from %06x\n' % t)
    with open((a.out or 'dis') + '.funcs', 'w') as fo:
        for f in sorted(funcs):
            fo.write('%06x %s %d\n' % (f, lab(f), len(xrefs.get(f, ()))))


if __name__ == '__main__':
    main()
