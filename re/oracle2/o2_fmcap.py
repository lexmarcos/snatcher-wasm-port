#!/usr/bin/env python3
"""FM timbre check: play FM songs / sound effects through the ORIGINAL driver on oracle2 (real BIOS, Nuked YM2612, real RF5C164 drums) and compare
the audio with the engine's pre-rendered file (assets/audio/fm/*.wav, rendered by sncore's YM2612 model) for the same id.
usage: o2_fmcap.py capture OUTDIR ID[,ID..]     -> OUTDIR/o2_<id>.wav (44100 Hz stereo, injected like audio_inject.py from the idle Junker HQ menu)
       o2_fmcap.py compare OUTDIR ID[,ID..]     -> levels (peak/RMS dBFS), onset-aligned envelope correlation, 1/3-octave band difference
ids hex: songs 3f-7a, sfx 01-3e."""
import json, os, struct, sys, wave
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
FM = os.path.join(ROOT, 'assets', 'audio', 'fm')

def inject(m, d0):
    rd = m.main.r8
    if 0x32 <= d0 < 0x3F:
        prio = rd(0xFF6142 + d0 * 8 + 3)
        if prio < rd(0xFF7F22): return False
        m.main.w8(0xFF7F22, prio); m.main.w8(0xFF7F23, rd(0xFF7F20))
    i = rd(0xFF7F20)
    m.main.w8(0xFF7F00 + i, d0)
    m.main.w8(0xFF7F20, (i + 1) & 0x1F if i + 1 != 0x20 else 0)
    return True

def write_wav(path, pcm, rate):
    with wave.open(path, 'wb') as w:
        w.setnchannels(2); w.setsampwidth(2); w.setframerate(rate); w.writeframes(pcm.astype(np.int16).tobytes())

def capture(outdir, ids):
    from o2lib import Machine2
    os.makedirs(outdir, exist_ok=True)
    fm = json.load(open(os.path.join(ROOT, 'assets', 'audio', 'fm.json')))['sounds']
    m = Machine2(); m.load_state(os.path.join(HERE, 'states', 'title.state')); m.record_audio = True
    for _ in range(300): m.step_frame()
    for i in ids:
        for v in (0xF2, 0xF1): inject(m, v)         # F2 stops the music (F1 alone leaves it playing), F1 the rest
        for _ in range(120): m.step_frame()
        m.audio.clear()
        ok = inject(m, i)
        info = fm.get(str(i)) or {}
        secs = min((info.get('seconds') or 4.0) + 1.0, 30.0)
        n = int(secs * 60)
        for _ in range(n): m.step_frame()
        pcm = np.concatenate(m.audio) if m.audio else np.zeros((0, 2), np.int16)
        m.audio.clear()
        write_wav(os.path.join(outdir, 'o2_%02x.wav' % i), pcm, int(m.srate))
        print('%02x accepted=%s frames=%d peak=%d' % (i, ok, n, int(np.abs(pcm).max()) if len(pcm) else 0), flush=True)

def load_wav(p):
    with wave.open(p) as w:
        d = np.frombuffer(w.readframes(w.getnframes()), np.int16).reshape(-1, w.getnchannels()).astype(np.float64)
        return d, w.getframerate()

def bands(x, rate, lo=60, hi=12000):
    # long-term spectrum in 1/3-octave bands (power, dB re full scale), Hann windows of 4096
    n = 4096; w = np.hanning(n); acc = np.zeros(n // 2 + 1)
    for s in range(0, max(len(x) - n, 1), n // 2):
        seg = x[s:s + n]
        if len(seg) < n: seg = np.pad(seg, (0, n - len(seg)))
        acc += np.abs(np.fft.rfft(seg * w)) ** 2
    f = np.fft.rfftfreq(n, 1.0 / rate)
    edges = lo * 2 ** (np.arange(0, np.log2(hi / lo) * 3 + 1) / 3)
    out = []
    for a, b in zip(edges[:-1], edges[1:]):
        k = (f >= a) & (f < b); out.append(10 * np.log10(acc[k].sum() / (32768.0 ** 2 * n) + 1e-20))
    return edges[:-1], np.array(out)

def envelope(x, rate, step=0.01):
    k = int(rate * step); n = len(x) // k
    return np.sqrt((x[:n * k].reshape(n, k) ** 2).mean(axis=1))

def onset(x, rate, thr=0.02):
    pk = np.abs(x).max()
    if pk == 0: return 0
    idx = np.nonzero(np.abs(x) > pk * thr)[0]
    return int(idx[0]) if len(idx) else 0

def compare(outdir, ids):
    rows = []
    for i in ids:
        kind = 'song' if i >= 0x3F else 'sfx'
        eng, er = load_wav(os.path.join(FM, '%s_%02x.wav' % (kind, i)))
        orc, orr = load_wav(os.path.join(outdir, 'o2_%02x.wav' % i))
        em, om = eng.mean(axis=1), orc.mean(axis=1)
        dur = min(len(em) / er, len(om) / orr) - 0.2
        # align on the onset, compare the first `dur` seconds (the original is recorded with 1 s of tail)
        eo, oo = onset(em, er), onset(om, orr)
        ea = em[eo:eo + int(dur * er)]; oa = om[oo:oo + int(dur * orr)]
        def lv(x): return 20 * np.log10(np.abs(x).max() / 32768.0 + 1e-9), 20 * np.log10(np.sqrt((x ** 2).mean()) / 32768.0 + 1e-9)
        pe, re_ = lv(ea); po, ro = lv(oa)
        ee, eo_env = envelope(ea, er), envelope(oa, orr)
        n = min(len(ee), len(eo_env)); c = float(np.corrcoef(ee[:n], eo_env[:n])[0, 1]) if n > 3 else float('nan')
        # drift: best lag (10 ms steps, +-40) of the first and of the last 5 s window
        def lag_of(a, b, w):
            best = (-2, 0)
            for L in range(-40, 41):
                x = a[max(w[0], 0):w[1]]; y = b[max(w[0] + L, 0):w[1] + L]
                k = min(len(x), len(y))
                if k > 20:
                    cc = np.corrcoef(x[:k], y[:k])[0, 1]
                    if cc > best[0]: best = (cc, L)
            return best
        w0 = (0, min(500, n)); w1 = (max(n - 500, 0), n)
        l0, l1 = lag_of(ee, eo_env, w0), lag_of(ee, eo_env, w1)
        fb, Be = bands(ea, er); _, Bo = bands(oa, orr)
        act = (Be > -90) | (Bo > -90)
        d = Bo[act] - Be[act]
        rows.append((i, kind, dur, pe, po, re_, ro, c, float(d.mean()), float(np.abs(d - d.mean()).mean()), (eo / er, oo / orr)))
        print('%s %02x  %5.1fs  peak eng/o2 %6.1f/%6.1f dBFS  rms %6.1f/%6.1f  env corr %.3f  band mean diff %+5.1f dB  shape dev %4.1f dB  onset eng %.3fs o2 %.3fs  lag start %+d0 ms (r %.2f) end %+d0 ms (r %.2f)' % (
            kind, i, dur, pe, po, re_, ro, c, rows[-1][8], rows[-1][9], eo / er, oo / orr, l0[1], l0[0], l1[1], l1[0]), flush=True)
        # per-band table for the report (only with --bands)
        if '--bands' in sys.argv:
            print('   band Hz:   ' + ' '.join('%5d' % f for f in fb[act]))
            print('   eng dB:    ' + ' '.join('%5.0f' % v for v in Be[act]))
            print('   oracle2 dB:' + ' '.join('%5.0f' % v for v in Bo[act]))
    return rows

if __name__ == '__main__':
    cmd, outdir, idl = sys.argv[1:4]
    ids = [int(x, 16) for x in idl.split(',')]
    {'capture': capture, 'compare': compare}[cmd](outdir, ids)
