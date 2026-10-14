"""Encode WAVs referenced by assets/audio/{pcm,fm}.json to Ogg/Opus (parallel ffmpeg) and write
web/{pcm,fm}.json with .ogg names. Usage: python3 -I audio_encode.py <assets/audio>"""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
root = sys.argv[1]
jobs = []
def walk(o, br):
    if isinstance(o, dict):
        for k, v in o.items():
            if k == 'file' and isinstance(v, str) and v.endswith('.wav'):
                o[k] = v[:-4] + '.ogg'; jobs.append((v, br))
            else:
                walk(v, br)
docs = {}
for name, br in (('pcm.json', '32k'), ('fm.json', '96k')):
    p = os.path.join(root, name)
    if os.path.exists(p):
        docs[name] = json.load(open(p)); walk(docs[name], br)
def enc(job):
    rel, br = job
    src = os.path.join(root, rel); dst = os.path.join(root, 'web', rel[:-4] + '.ogg')
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if os.path.exists(dst) and os.path.getmtime(dst) >= os.path.getmtime(src): return
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', src, '-c:a', 'libopus', '-b:a', br,
                    '-application', 'audio', dst], check=True)
with ThreadPoolExecutor(os.cpu_count()) as ex:
    list(ex.map(enc, jobs))
for name, d in docs.items():
    json.dump(d, open(os.path.join(root, 'web', name), 'w'), indent=1)
print('encoded', len(jobs))
