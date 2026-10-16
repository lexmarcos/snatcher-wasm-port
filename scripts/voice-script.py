#!/usr/bin/env python3
"""Interleave the voice transcripts with the script lines: work/voice/script/SPnn.txt (a working file, game content, gitignored).

  python3 -I scripts/voice-script.py

Per script root (cutscene), in code order: the message lines (from `snt i18n-ctx`) and the voice triggers (from work/voice/voice-triggers.csv)
with the Whisper transcript (work/voice/transcripts/en.json). Used to judge speaker, context and non-speech clips.
"""
import csv, json, os, re, subprocess, collections

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)
en = json.load(open("work/voice/transcripts/en.json"))
ov = json.load(open("work/voice/transcripts/overrides.json")) if os.path.exists("work/voice/transcripts/overrides.json") else {}
trig = collections.defaultdict(list)
for r in csv.DictReader(open("work/voice/voice-triggers.csv")):
    if int(r["id"]) >= 36:
        trig[r["file"]].append(r)
os.makedirs("work/voice/script", exist_ok=True)
for n in range(39):
    sp = f"SP{n:02d}"
    items = collections.defaultdict(list)
    order = []
    out = subprocess.run(["./target/release/snt", "i18n-ctx", "assets/raw/files", sp], capture_output=True, text=True).stdout
    label = None
    for l in out.splitlines():
        m = re.match(r"-- (L_[0-9a-f]+) scene (.*)", l)
        if m:
            label = m.group(1); order.append(label) if label not in order else None; continue
        m = re.match(r"\s*@([0-9a-f]{4}) \[(.*?)\](?: v([0-9a-f]{4}))? (.*?)\s+<\d+p> pc=([0-9a-f]{4})", l)
        if m and label:
            items[label].append((int(m.group(5), 16), "SAY", f"[{m.group(2)}] {m.group(4)}"))
    for r in trig.get(sp, []):
        cid = r["id"]; e = en.get(cid)
        t = ov.get(cid, {}).get("text", e["text"] if e else "?") if e else "?"
        flag = "" if (e and e["text"]) else " (NO SPEECH)"
        items[r["label"]].append((int(r["pc"], 16), "VOICE", f"#{cid} {float(r['seconds']):.1f}s \"{t}\"{flag}  [{r['class']}]"))
        if r["label"] not in order:
            order.append(r["label"])
    with open(f"work/voice/script/{sp}.txt", "w") as f:
        for lab in sorted(order, key=lambda x: int(x[2:], 16)):
            f.write(f"\n== {lab}\n")
            for pc, k, t in sorted(items[lab], key=lambda x: (x[0], x[1])):
                f.write(f"{pc:04x} {k:5s} {t}\n")
print("ok")
