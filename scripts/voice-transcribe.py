#!/usr/bin/env python3
"""Transcribe the exported voice clips with the user's Whisper server (OpenAI-compatible, LAN only).

  snt voice-export assets/raw/files work/voice/wav
  python3 -I scripts/voice-transcribe.py [--jobs 3] [--only 450,451] [--redo]
  python3 -I scripts/voice-transcribe.py --alt          # second pass without the names prompt (raw_noprompt/), kept as `alt` in en.json
  python3 -I scripts/voice-transcribe.py --merge        # only rebuild work/voice/transcripts/en.json

Reads the server URL and key from work/whisper.env (gitignored) or the environment variables WHISPER_URL / WHISPER_API_KEY.
One file per clip in work/voice/transcripts/raw/NNNN.json (a rerun skips what is done), merged into transcripts/en.json.
The output is game content (English speech): work/ is gitignored, never commit it.
"""
import json, os, sys, time, uuid, threading, urllib.request, urllib.error
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WAV = os.path.join(ROOT, "work/voice/wav")
OUT = os.path.join(ROOT, "work/voice/transcripts")
RAW = os.path.join(OUT, "raw")
ALT = os.path.join(OUT, "raw_noprompt")  # second opinion: the same clip without the names prompt (--alt)
PROMPT = ("Gillian Seed, Jean-Jack Gibson, Mika Slayton, Metal Gear, Junker, Neo Kobe, Snatcher, Benson, Napoleon, Katrina, "
          "Jamie, Harry Benson, Lisa, Random Hajile, Elijah Modnar, Chief Cunningham, Isabella Velvet, Jordan.")


def env():
    e = dict(os.environ)
    p = os.path.join(ROOT, "work/whisper.env")
    if os.path.exists(p):
        for l in open(p):
            l = l.strip()
            if l and not l.startswith("#") and "=" in l:
                k, v = l.split("=", 1)
                e.setdefault(k.strip(), v.strip())
    return e


def post(url, key, path, prompt):
    b = uuid.uuid4().hex
    fields = [("model", "large-v3"), ("language", "en"), ("response_format", "verbose_json"), ("word_timestamps", "true"),
              ("temperature", "0")]
    if prompt:
        fields.append(("prompt", prompt))
    body = b""
    for k, v in fields:
        body += f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
    body += (f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="{os.path.basename(path)}"\r\n'
             f'Content-Type: audio/wav\r\n\r\n').encode() + open(path, "rb").read() + f"\r\n--{b}--\r\n".encode()
    rq = urllib.request.Request(url, data=body, headers={"Authorization": "Bearer " + key,
                                                         "Content-Type": "multipart/form-data; boundary=" + b})
    with urllib.request.urlopen(rq, timeout=180) as r:
        return json.loads(r.read())


def one(cid, url, key, redo, alt=False):
    f = os.path.join(ALT if alt else RAW, f"{cid}.json")
    if os.path.exists(f) and not redo:
        return cid, "skip"
    wav = os.path.join(WAV, f"v{cid:04d}.wav")
    last = None
    for attempt in range(5):
        try:
            t0 = time.time()
            j = post(url, key, wav, None if alt else PROMPT)
            j["_elapsed"] = round(time.time() - t0, 3)
            if not alt and not j.get("text", "").strip():  # the prompt can make Whisper answer nothing: ask once more without it
                j2 = post(url, key, wav, None)
                if j2.get("text", "").strip():
                    j2["_elapsed"] = j["_elapsed"]
                    j2["_noprompt"] = True
                    j = j2
            tmp = f + ".tmp"
            json.dump(j, open(tmp, "w"))
            os.replace(tmp, f)
            return cid, "ok"
        except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as ex:
            last = ex
            time.sleep(2 + 3 * attempt)
    return cid, f"FAILED {last}"


import re
VOCAL = re.compile(r"^(?:(?:a+h+|o+h+|u+h+|u+g+h+|a+w+|o+o+h+|w+h*o+a+h*|h+m+|m+|h+u+h+|a+r+g+h+|o+f+|e+h+|a+a+h+)[\s.,!?-]*)+$", re.I)
HALLU = re.compile(r"^(?:thank(?:s| you)(?: (?:very much|for watching|for listening))?|bye)[.! ]*$|^thanks for watching", re.I)


def classify(text, seconds, nsp, lp):
    """(speech, flag): a clip gets a subtitle only when speech is true."""
    t = text.strip()
    if not t:
        return False, "empty"
    if VOCAL.match(t):
        return False, "vocal"
    if HALLU.match(t):
        return False, "hallucination?"
    if (nsp or 0) > 0.3:
        return False, "no_speech_prob"
    if (lp if lp is not None else 0) < -0.8:
        return False, "low_confidence"
    return True, None


def merge():
    idx = json.load(open(os.path.join(WAV, "index.json")))
    out = {}
    for cid, meta in idx.items():
        f = os.path.join(RAW, f"{cid}.json")
        if not os.path.exists(f):
            continue
        j = json.load(open(f))
        segs = []
        words = []
        for s in j.get("segments", []):
            segs.append({"start": s["start"], "end": s["end"], "text": s["text"].strip(), "avg_logprob": s.get("avg_logprob"),
                         "no_speech_prob": s.get("no_speech_prob"), "compression_ratio": s.get("compression_ratio")})
            for w in s.get("words", []) or []:
                words.append({"w": w["word"].strip(), "start": w["start"], "end": w["end"], "p": w.get("probability")})
        fa = os.path.join(ALT, f"{cid}.json")
        alt = json.load(open(fa)) if os.path.exists(fa) else None
        out[cid] = {"alt": alt["text"].strip() if alt else None, "seconds": meta["seconds"], "text": j.get("text", "").strip(), "segments": segs, "words": words,
                    "avg_logprob": min([s["avg_logprob"] for s in segs if s["avg_logprob"] is not None], default=None),
                    "no_speech_prob": max([s["no_speech_prob"] for s in segs if s["no_speech_prob"] is not None], default=None),
                    "elapsed": j.get("_elapsed"), "noprompt": bool(j.get("_noprompt"))}
    ovp = os.path.join(OUT, "overrides.json")
    ov = json.load(open(ovp)) if os.path.exists(ovp) else {}
    for cid, e in out.items():
        e["speech"], e["flag"] = classify(e["text"], e["seconds"], e["no_speech_prob"], e["avg_logprob"])
        o = ov.get(cid)
        if o:  # manual decision (work/voice/transcripts/overrides.json): {"text": corrected English, "speech": bool, "why": ""}
            e["whisper_text"] = e["text"]
            e["text"] = o.get("text", e["text"])
            e["speech"] = o.get("speech", e["speech"])
            e["flag"] = "override: " + o.get("why", "") if o.get("why") else "override"
    json.dump(out, open(os.path.join(OUT, "en.json"), "w"), indent=1, sort_keys=False)
    # flat copy for the Rust tools (snt voice-import / voice-check): id <TAB> seconds <TAB> speech 0/1 <TAB> flag <TAB> text
    with open(os.path.join(OUT, "en.tsv"), "w") as f:
        for cid, e in sorted(out.items(), key=lambda x: int(x[0])):
            f.write("\t".join([cid, "%.3f" % e["seconds"], "1" if e["speech"] else "0", (e["flag"] or "").replace("\t", " "),
                               " ".join(e["text"].split())]) + "\n")
    return len(out), len(idx)


def main():
    a = sys.argv[1:]
    os.makedirs(RAW, exist_ok=True)
    if "--merge" in a:
        print("merged %d of %d" % merge())
        return
    jobs = int(a[a.index("--jobs") + 1]) if "--jobs" in a else 3
    only = {int(x) for x in a[a.index("--only") + 1].split(",")} if "--only" in a else None
    redo = "--redo" in a
    alt = "--alt" in a
    if alt:
        os.makedirs(ALT, exist_ok=True)
    e = env()
    url, key = e["WHISPER_URL"], e["WHISPER_API_KEY"]
    ids = sorted(int(k) for k in json.load(open(os.path.join(WAV, "index.json"))))
    if only:
        ids = [i for i in ids if i in only]
    t0 = time.time()
    done = 0
    lock = threading.Lock()
    fails = []
    with ThreadPoolExecutor(jobs) as ex:
        for cid, st in ex.map(lambda i: one(i, url, key, redo, alt), ids):
            with lock:
                done += 1
                if st.startswith("FAILED"):
                    fails.append(cid)
                    print(cid, st, flush=True)
                elif done % 50 == 0:
                    print(f"{done}/{len(ids)} {time.time()-t0:.0f}s", flush=True)
    print("done", len(ids), "failed", fails, f"{time.time()-t0:.0f}s")
    print("merged %d of %d" % merge())


main()
