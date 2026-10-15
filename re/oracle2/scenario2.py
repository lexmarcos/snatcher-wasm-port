#!/usr/bin/env python3
"""Run a scenario (same JSON as re/oracle/scenarios/*.json) on oracle2 = original game on the real Sega CD BIOS (Genesis Plus GX).
usage: re/oracle/.venv/bin/python -I re/oracle2/scenario2.py scenarios/intro.json [--out DIR] [--offset N] [--frames N]
Frame numbers of the scenario are relative to t0 (first frame of the game's IP after the BIOS boot, see Machine2.boot); the real BIOS
loads the game much slower than the HLE oracle, so use --offset to shift the whole scenario or the extra entries below.
JSON extensions (ignored by the HLE oracle's scenario.py):
  inputs entry {"until": {"state":1,"script":63,"sp_state":6,"vm_pc":..}, "max": N, "hold": bits}  wait (pad = hold, default 0) until the
     frame record matches every key, at most N frames (default 6000); the script clock is not advanced by waiting entries
  inputs entry {"shot": "name"} / {"save": "name"}: screenshot / save state at this point of the input list (no frame consumed), event aligned
  inputs entry {"dump": "name"}: write VDP regs / CRAM / VSRAM / VRAM md5 to NAME.json
  until keys may also be memory words: "s16:7896": 9 (sub, hex address), "s8:..", "m16:..", "s32:.." (main addresses as in the HLE docs, e.g. m16:FFE061)
  "shots": {"name": frame}  or {"name": {"at": frame}} ; "states": same. With --events the shots are taken at the HLE frame + drift of the
     last 'until' (i.e. the frames are relative to the end of the last wait) so that they land on the same game event."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from o2lib import Machine2
from PIL import Image

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('scenario')
    ap.add_argument('--out', default=os.path.join(HERE, 'out', 'refs'))
    ap.add_argument('--offset', type=int, default=0, help='frames to wait (pad 0) after t0 before the scenario starts')
    ap.add_argument('--frames', type=int, help='override the scenario length')
    ap.add_argument('--load', help='start from a oracle2 save state instead of booting')
    ap.add_argument('--gun', action='store_true')
    ap.add_argument('--ym', default='nuked (ym2612)')
    ap.add_argument('--cd-latency', default=None, help='enabled|disabled')
    ap.add_argument('--match', help='directory of reference PNGs (HLE oracle shots): report, for each, the frame with the fewest differing pixels (normalised colours)')
    ap.add_argument('--match-every', type=int, default=2)
    ap.add_argument('--audio', help='write the audio of the whole run to this WAV file')
    a = ap.parse_args()
    sc = json.load(open(a.scenario))
    os.makedirs(a.out, exist_ok=True)
    os.makedirs(os.path.join(HERE, 'states'), exist_ok=True)
    m = Machine2(trace_path=os.path.join(a.out, 'trace.jsonl'), ym=a.ym, cd_latency=a.cd_latency, gun=a.gun)
    if a.audio: m.record_audio = True
    if a.load:
        m.load_state(a.load); t0 = m.frame
    else:
        t0 = m.boot()
    shots = {}
    for k, v in sc.get('shots', {}).items(): shots.setdefault(v['at'] if isinstance(v, dict) else v, []).append(k)
    states = {}
    for k, v in sc.get('states', {}).items(): states.setdefault(v['at'] if isinstance(v, dict) else v, []).append(k)
    end = (a.frames or sc['frames'])
    entries = [e for e in sc['inputs']]
    cur = None       # [remaining, bits] of a plain entry
    wait = None      # active until-entry
    sf = 0           # scenario clock
    for _ in range(a.offset):
        m.inputs = []; m.step_frame()
    def snap(sfr):
        for n in shots.get(sfr, []):
            Image.fromarray(m.frame_rgb()).save(os.path.join(a.out, n + '.png'))
        for n in states.get(sfr, []):
            m.save_state(os.path.join(HERE, 'states', n + '.state'))
    refs = {}
    best = {}
    if a.match:
        import numpy as np
        for fn in sorted(os.listdir(a.match)):
            if fn.endswith('.png'): refs[fn[:-4]] = np.array(Image.open(os.path.join(a.match, fn)).convert('RGB'))
        best = {k: [10**9, None] for k in refs}
    def match_step():
        import numpy as np
        from o2lib import normalize_levels
        img = None
        for k, ref in refs.items():
            if img is None: img = normalize_levels(m.frame_rgb())
            if img.shape != ref.shape: continue
            d = int((img != ref).any(axis=2).sum())
            if d < best[k][0]: best[k] = [d, m.frame - t0]
    last = {}
    def cond(k, v):
        if ':' in k:
            kind, addr = k.split(':'); addr = int(addr, 16)
            cpu = m.sub if kind[0] == 's' else m.main
            return {'8': cpu.r8, '16': cpu.r16, '32': cpu.r32}[kind[1:]](addr) == v
        return last.get(k) == v
    while sf < end or wait is not None or entries and isinstance(entries[0], dict):
        if wait is None and cur is None and entries:
            e = entries.pop(0)
            if isinstance(e, dict) and 'shot' in e:
                Image.fromarray(m.frame_rgb()).save(os.path.join(a.out, e['shot'] + '.png')); m.log({'t': 'shot', 'name': e['shot'], 'f': m.frame}); continue
            if isinstance(e, dict) and 'dump' in e:
                import hashlib
                d = {'f': m.frame, 'regs': m.vdp_regs(), 'cram': ['%03x' % c for c in m.cram()], 'vsram': m.vsram(), 'vram_md5': hashlib.md5(m.vram()).hexdigest()}
                json.dump(d, open(os.path.join(a.out, e['dump'] + '.json'), 'w')); continue
            if isinstance(e, dict) and 'save' in e:
                m.save_state(os.path.join(HERE, 'states', e['save'] + '.state')); m.log({'t': 'save', 'name': e['save'], 'f': m.frame}); continue
            if isinstance(e, dict): wait = dict(e, left=e.get('max', 6000))
            elif e[0] > 0: cur = list(e)
            continue
        if wait is not None:
            if all(cond(k, v) for k, v in wait['until'].items()) or wait['left'] <= 0:
                m.log({'t': 'until_done', 'f': m.frame, 'ok': wait['left'] > 0, 'until': wait['until']}); wait = None
                continue
            wait['left'] -= 1
            m.inputs = [[1, wait.get('hold', 0)]]
            last = m.step_frame()
            if refs and m.frame % a.match_every == 0: match_step()
            continue
        if cur is not None:
            m.inputs = [[1, cur[1]]]; cur[0] -= 1
            if cur[0] <= 0: cur = None
        else:
            m.inputs = []
        last = m.step_frame()
        sf += 1
        if refs and m.frame % a.match_every == 0: match_step()
        snap(sf)
        if sf % 1000 == 0: print('scenario frame', sf, 'oracle2 frame', m.frame, flush=True)
    if a.audio:
        import numpy as np, wave
        pcm = np.concatenate(m.audio) if m.audio else np.zeros((0, 2), np.int16)
        with wave.open(a.audio, 'wb') as w:
            w.setnchannels(2); w.setsampwidth(2); w.setframerate(int(m.srate)); w.writeframes(pcm.tobytes())
    for k, (d, f) in best.items(): print('match %-16s best %6d px at game frame %s' % (k, d, f))
    print('done: scenario frame', sf, 'oracle2 frame', m.frame, 't0', t0)

main()
