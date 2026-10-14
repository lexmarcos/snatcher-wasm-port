#!/usr/bin/env python3
"""AUDIO-VERIFY: record all reference logs of the ORIGINAL FM driver into assets/audio/ref/.

usage (from re/oracle, needs the oracle venv + core/build, see docs/re/oracle.md):
    .venv/bin/python -I audio_ref.py [outdir] [--quick]

Writes (format: audio_ym.py docstring; replayed by core/src/audio/verify.rs):
  intro_ym.txt     title screen -> options -> intro -> Junker HQ (music 3F, 38, 4F for ~2450
                   frames, menu sfx 1D) = the real game flow, scenarios/intro.json, 6600 frames
  inject600.txt    every music id 3F-7A (600 frames each) + one sfx between them, then the
                   remaining sfx (audio_inject.py 600)
  inject3000.txt   every music id for 3000 frames (50 s: past the first loop of most songs)
  inject_vol.txt   like inject600 with 1500 frames per song and the volume word M:$FF7F5E
                   swept through ramps/pseudo-random values (--vol)
  sfx_only.txt     every FM sfx 01-3E alone (no music), 150 frames apart
--quick only records intro_ym.txt and inject600.txt (~2 min). Everything runs in parallel.
The logs are gitignored (they describe the original code's behaviour, not shipped).
"""
import os, subprocess, sys
here = os.path.dirname(os.path.abspath(__file__))
args = [a for a in sys.argv[1:] if not a.startswith('--')]
out = os.path.abspath(args[0]) if args else os.path.join(here, '..', '..', 'assets', 'audio', 'ref')
os.makedirs(out, exist_ok=True)
py = sys.executable
jobs = [
    [py, '-I', 'audio_ym.py', 'scenarios/intro.json', '6600', os.path.join(out, 'intro_ym.txt')],
    [py, '-I', 'audio_inject.py', os.path.join(out, 'inject600.txt'), '600', '1500'],
]
if '--quick' not in sys.argv:
    jobs += [
        [py, '-I', 'audio_inject.py', os.path.join(out, 'inject3000.txt'), '3000', '1500'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'inject_vol.txt'), '1500', '1500', '--vol'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'sfx_only.txt'), '300', '1500', '--sfxonly'],
    ]
procs = [subprocess.Popen(j, cwd=here) for j in jobs]
rc = [p.wait() for p in procs]
print('exit codes', rc)
sys.exit(1 if any(rc) else 0)
