#!/usr/bin/env python3
"""FIDELITY2: CONTINUE round trip. Plays a fid_play session (pad script json) in the ORIGINAL up to the save and compares the
backup-RAM file written by the original's save routine (S:0x10592 -> _BURAM fn 4, HLE) with the block the engine's cmd 1E emits (Out::Save).
usage (from re/oracle): .venv/bin/python -I fid_save.py --jump SP01:1b3 --inputs-file out/fid2/a2_queens1.json --frames 5700 [--dump out/fid2/save_oracle.hex]
Prints the oracle file name/size and the byte differences against the engine block (run.rs prints `Save { slot, data }`)."""
import argparse, json, os, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
from machine import Machine
from fid_common import oracle_jump, patch_cdtime
ap = argparse.ArgumentParser()
ap.add_argument('--jump', required=True); ap.add_argument('--inputs-file', required=True); ap.add_argument('--frames', type=int, required=True)
ap.add_argument('--dump', default='')
a = ap.parse_args()
patch_cdtime()
m = Machine(); m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
oracle_jump(m, a.jump, 300)
inp = json.load(open(a.inputs_file)); press = {e[0]: e[1] for e in inp}
for rel in range(1, a.frames + 1):
    m.sub.write(0x786A, bytes(4)); m.inputs = [[1, press.get(rel, 0)]]
    m.step_frame(capture=False)
files = getattr(m, 'bram_files', {})
print('oracle bram files:', {k: len(v) for k, v in files.items()})
ofile = None  # the file written last (dict insertion order is not enough: re-written names keep their position) -> pick by the engine slot below
blk = bytes(m.sub.read(0x9C00, 0x1C0))
# engine
env = dict(os.environ, FIDRNG='1', SYNC=os.path.join(HERE, 'out', 'fid', 'save_sync.json'))
os.makedirs(os.path.dirname(env['SYNC']), exist_ok=True); json.dump(inp, open(env['SYNC'], 'w'))
r = subprocess.run([os.path.join(ROOT, 'target', 'release', 'examples', 'run'), str(a.frames + 400), '100000', '/tmp/fid_save_eng', '', a.jump], cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True)
allm = re.findall(r'Save \{ slot: (\d+), data: \[([0-9, ]*)\] \}', r.stdout)
mm = None
if allm:
    class _M:
        def __init__(s, g): s.g = g
        def group(s, i): return s.g[i - 1]
    mm = _M(allm[-1]); print('engine saves:', [x[0] for x in allm])
if not mm: print('engine: no Save event'); sys.exit(1)
edata = bytes(int(x) for x in mm.group(2).split(','))
print('engine save slot', mm.group(1), 'len', len(edata))
ofile = files.get('SNATCHER_%02d' % int(mm.group(1)))
if ofile is None: print('oracle: no file for the slot'); sys.exit(1)
print('oracle file == $9C00 block at the end of the run:', ofile[:0x1C0] == blk)
if a.dump: open(a.dump, 'w').write(ofile[:0x1C0].hex())
diff = [i for i in range(min(len(edata), 0x1C0)) if edata[i] != ofile[i]]
print('differing bytes:', len(diff), [(hex(i), hex(ofile[i]), hex(edata[i])) for i in diff[:40]])
# files for the CONTINUE round trip (fid_cont.py)
for k, v in files.items(): open(os.path.join(HERE, 'out', 'fid2', 'bram_%s.hex' % k), 'w').write(v.hex())
