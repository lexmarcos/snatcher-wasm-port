#!/usr/bin/env python3
"""Speaker evidence for the voice subtitles: which portrait is talking while a clip plays (docs/voice-subtitles.md section 4).

  ./target/release/examples/strip_probe survey 3000 $(roots) > work/voice/strip-survey.txt      # roots: SPnn:pc (work/voice/voice-scenes.csv)
  python3 -I scripts/voice-speakers.py [work/voice/strip-survey.txt] [i18n/pt-BR/voice.json]

The game animates the mouth of the portrait that talks (the object next to the frame is not paused and its animation index moves). `strip_probe survey`
logs, per clip and moment, the portraits in the strip (column x, `*` = mouth not paused, `m` = animation moving, a hash of the top rows of the art).
Portrait art -> speaker number is learned from the clips whose speaker is set in voice.json (votes); then the script lists
  * PROPOSE: clips without a speaker where exactly one portrait is talking and its art is mapped (>= 2 clips, >= 75 %),
  * CHECK:   clips whose voice.json speaker differs from the talking portrait's speaker (a mislabelled clip, or a portrait that is not the speaker).
Nothing is changed: review the lines (the English text is printed when work/voice/transcripts/en.tsv exists) and put the speaker in a patch for
`snt voice-import`. When unsure, show no name.
"""
import collections, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)
survey = sys.argv[1] if len(sys.argv) > 1 else "work/voice/strip-survey.txt"
vpath = sys.argv[2] if len(sys.argv) > 2 else "i18n/pt-BR/voice.json"
voice = json.load(open(vpath))["clips"]
en = {}
if os.path.exists("work/voice/transcripts/en.tsv"):
    for l in open("work/voice/transcripts/en.tsv"):
        f = l.rstrip("\n").split("\t")
        if len(f) >= 5:
            en[f[0]] = f[4]

# per clip: talking portraits (art hashes) at the moments after the portraits settled (>= 25 frames)
talk = collections.defaultdict(list)
for l in open(survey):
    m = re.match(r"(\S+) id=(\d+) f=\d+ (.*)", l)
    if not m:
        continue
    cid = m.group(2)
    for d, s in re.findall(r"\+(\d+):\[([^\]]*)\]", m.group(3)):
        if int(d) < 25:
            continue
        t = []
        for x in s.split(","):
            if not x:
                continue
            a, h = x.split(":")
            if "*" in a and "m" in a:
                t.append(h)
        talk[cid].append(t)


def talking(cid):
    """the art hashes of the one portrait that talks in every sample of the clip (an animated display shows several), or None"""
    ts = talk.get(cid)
    if not ts or any(len(t) != 1 for t in ts):
        return None
    return {t[0] for t in ts}


votes = collections.defaultdict(collections.Counter)
for cid, v in voice.items():
    hs = talking(cid)
    if hs and "s" in v:
        for h in hs:
            votes[h][v["s"]] += 1
mapping = {}
for h, c in votes.items():
    s, n = c.most_common(1)[0]
    if sum(c.values()) >= 2 and n / sum(c.values()) >= 0.75:
        mapping[h] = s
print("portrait art -> speaker (votes):")
for h, c in sorted(votes.items(), key=lambda x: -sum(x[1].values())):
    print(f"  {h}  {dict(c)}  {'-> ' + mapping[h] if h in mapping else '(not mapped)'}")
print()
for cid in sorted(voice, key=int):
    v = voice[cid]
    hs = talking(cid)
    if not hs or not all(h in mapping for h in hs) or len({mapping[h] for h in hs}) != 1:
        continue
    sp = mapping[next(iter(hs))]
    if "s" not in v:
        print(f"PROPOSE {cid}\t{sp}\t{en.get(cid, '')[:90]}")
    elif v["s"] != sp:
        print(f"CHECK   {cid}\tjson {v['s']} / portrait {sp}\t{en.get(cid, '')[:90]}")
