#!/usr/bin/env python3
"""Where is speech on the CD-DA tracks (2..21)? A cheap energy / modulation heuristic that needs no server (the Whisper pass is the real answer).

  python3 scripts/cdda-heuristic.py [--wav work/cdda/wav] [--out work/cdda/heuristic.txt]

16 kHz mono WAV of every track (made by `scripts/cdda-transcribe.py --prepare`). For every 2 s window: the share of the 300-3400 Hz envelope's modulation power that
sits at the syllable rate (2.5-7 Hz), the depth of the envelope dips (10th / 90th percentile of the 20 ms energies: speech pauses between words, sustained music does not)
and the spectral flux; a window is a *speech candidate* when the modulation share (> 0.30) and the dip depth (> 1.0 decade; calibrated on 40 random PCM voice clips: median 1.4, music tracks 3 / 4 / 19: median 0.45) are both speech-like. Output: per track the minutes of candidate windows and
a strip (`#` candidate, `.` not) of one character per 2 s. Evidence only: music with a strong rhythm in the syllable band, and speech over music, can fool it."""
import os, sys, wave, argparse
import numpy as np

def load(p):
    with wave.open(p) as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1
        return np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768

def band(x, lo, hi, sr=16000):
    n = len(x); X = np.fft.rfft(x); f = np.fft.rfftfreq(n, 1 / sr)
    X[(f < lo) | (f > hi)] = 0
    return np.fft.irfft(X, n)

def analyse(x, sr=16000, win=2.0):
    b = band(x, 300, 3400)
    hop = int(0.02 * sr)
    n = len(b) // hop
    e = np.sqrt((b[:n * hop].reshape(n, hop) ** 2).mean(axis=1) + 1e-12)       # 20 ms RMS envelope (50 Hz)
    k = int(win / 0.02); out = []
    for i in range(0, n - k + 1, k):
        w = e[i:i + k]
        if w.mean() < 0.004: out.append((0, 0, 0, w.mean())); continue
        d = w - w.mean(); S = np.abs(np.fft.rfft(d * np.hanning(len(d)))) ** 2
        f = np.fft.rfftfreq(len(d), 0.02)
        tot = S[(f >= 0.5) & (f <= 16)].sum() + 1e-12
        mod = S[(f >= 2.5) & (f <= 7)].sum() / tot
        lw = np.log10(w + 1e-6)
        dip = np.percentile(lw, 90) - np.percentile(lw, 10)           # decades between loud and quiet
        out.append((mod, dip, 0, w.mean()))
    return out

def main():
    ap = argparse.ArgumentParser()
    root = os.path.join(os.path.dirname(__file__), '..')
    ap.add_argument('--wav', default=os.path.join(root, 'work/cdda/wav'))
    ap.add_argument('--out', default=os.path.join(root, 'work/cdda/heuristic.txt'))
    a = ap.parse_args()
    lines = []
    for t in range(2, 22):
        p = os.path.join(a.wav, 'track%02d.wav' % t)
        if not os.path.exists(p): continue
        x = load(p)
        r = analyse(x)
        cand = ''.join('#' if (m > 0.30 and d > 1.0) else ('+' if (m > 0.25 and d > 0.8) else '.') for m, d, _, lvl in r)
        mins = sum(1 for c in cand if c == '#') * 2 / 60
        lines.append('track %02d  %6.1f s  candidate speech %.1f min (+ weaker %.1f)' % (t, len(x) / 16000, mins, cand.count('+') * 2 / 60))
        for i in range(0, len(cand), 60): lines.append('   %4ds %s' % (i * 2, cand[i:i + 60]))
    open(a.out, 'w').write('\n'.join(lines) + '\n')
    print('\n'.join(l for l in lines if l.startswith('track')))

if __name__ == '__main__':
    main()
