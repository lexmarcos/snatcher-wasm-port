"""Encode WAVs referenced by assets/audio/pcm.json to Ogg/Opus (parallel ffmpeg) and write
pcm_web.json with .ogg names. Usage: python3 -I audio_encode.py <assets/audio> [bitrate]"""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
root = sys.argv[1]; br = sys.argv[2] if len(sys.argv) > 2 else '32k'
j = json.load(open(os.path.join(root, 'pcm.json')))
jobs = []
def walk(o):
    if isinstance(o, dict):
        for k, v in o.items():
            if k == 'file' and isinstance(v, str) and v.endswith('.wav'):
                o[k] = v[:-4] + '.ogg'; jobs.append(v)
            else:
                walk(v)
walk(j)
def enc(rel):
    src = os.path.join(root, rel); dst = os.path.join(root, 'web', rel[:-4] + '.ogg')
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if os.path.exists(dst) and os.path.getmtime(dst) >= os.path.getmtime(src): return
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', src, '-c:a', 'libopus', '-b:a', br,
                    '-application', 'audio', dst], check=True)
with ThreadPoolExecutor(os.cpu_count()) as ex:
    list(ex.map(enc, jobs))
json.dump(j, open(os.path.join(root, 'web', 'pcm.json'), 'w'), indent=1)
print('encoded', len(jobs))
