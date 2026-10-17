#!/usr/bin/env python3
"""Find and transcribe the speech on the CD-DA tracks (2..21) with the user's Whisper server (OpenAI-compatible, LAN only).

  python3 scripts/cdda-transcribe.py --prepare            # 16 kHz mono WAV of every track from assets/raw/audio -> work/cdda/wav (needs ffmpeg)
  python3 scripts/cdda-transcribe.py [--only 3,4] [--redo] # Whisper, chunks of 150 s with 10 s overlap, resumable (work/cdda/transcripts/raw/trackNN_cMM.json); then merges
  python3 scripts/cdda-transcribe.py --merge               # only rebuild en.json / en.tsv / summary.txt from the raw files

Server URL and key: work/whisper.env (gitignored) or WHISPER_URL / WHISPER_API_KEY; never printed or committed, audio goes only to that LAN server.
Output (game content, English speech: work/ is gitignored, never commit it):
  en.json     per track: duration, speech seconds, `music_only`, segments {start, end, text, avg_logprob, no_speech_prob, compression_ratio, speech, flag, words}
  en.tsv      flat copy for the Rust tools (`snt cdda-import` / `cdda-check`): track <TAB> start_ms <TAB> end_ms <TAB> speech 0/1 <TAB> flag <TAB> text
  summary.txt which tracks have speech (minutes) and which are music only
Absolute times: chunk offset + segment time; the overlap is resolved by keeping every segment whose middle lies in the part of the chunk it owns.
Whisper invents text on music ("Thank you.", lyrics, repetitions): a segment is speech only when it passes `classify` (no_speech_prob, avg_logprob, compression ratio,
known hallucinations) and a track is `music_only` when fewer than 4 s of speech survive."""
import json, os, re, subprocess, sys, time, uuid, argparse, urllib.request, urllib.error
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORK = os.environ.get("CDDA_WORK", os.path.join(ROOT, "work/cdda"))      # tests point it at a scratch directory
WAV = os.path.join(WORK, "wav")
OUT = os.path.join(WORK, "transcripts")
RAW = os.path.join(OUT, "raw")
CHUNK, OVERLAP = 150.0, 10.0
PROMPT = ("Snatcher intro narration. Gillian Seed, Jean-Jack Gibson, Mika Slayton, Metal Gear, Junker, Neo Kobe City, Snatcher, Benson, Napoleon, Katrina, "
          "Jamie, Harry Benson, Lisa, Random Hajile, Elijah Modnar, Chief Cunningham, Isabella Velvet, Jordan, Moscow, Chernoton, Lucifer-Alpha, Outer Heaven.")


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


def prepare():
    os.makedirs(WAV, exist_ok=True)
    for t in range(2, 22):
        src = os.path.join(ROOT, "assets/raw/audio/track%02d.wav" % t)
        dst = os.path.join(WAV, "track%02d.wav" % t)
        if os.path.exists(src) and not os.path.exists(dst):
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", src, "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", dst], check=True)
    print("wav ready in", WAV)


def server_up(url, key):
    base = url.split("/v1/")[0]
    try:
        rq = urllib.request.Request(base + "/v1/models", headers={"Authorization": "Bearer " + key})
        with urllib.request.urlopen(rq, timeout=10) as r:
            return r.status == 200
    except Exception as ex:
        print("server not reachable:", type(ex).__name__, ex)
        return False


def post(url, key, path, prompt):
    b = uuid.uuid4().hex
    fields = [("model", "large-v3"), ("language", "en"), ("response_format", "verbose_json"), ("word_timestamps", "true"),
              ("timestamp_granularities[]", "word"), ("timestamp_granularities[]", "segment"), ("temperature", "0")]
    if prompt:
        fields.append(("prompt", prompt))
    body = b""
    for k, v in fields:
        body += f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
    body += (f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="{os.path.basename(path)}"\r\n'
             f'Content-Type: audio/wav\r\n\r\n').encode() + open(path, "rb").read() + f"\r\n--{b}--\r\n".encode()
    rq = urllib.request.Request(url, data=body, headers={"Authorization": "Bearer " + key, "Content-Type": "multipart/form-data; boundary=" + b})
    with urllib.request.urlopen(rq, timeout=600) as r:
        return json.loads(r.read())


def chunks(duration):
    out, i, s = [], 0, 0.0
    while True:
        e = min(duration, s + CHUNK)
        out.append((i, s, e))
        if e >= duration - 1e-6:
            return out
        i += 1
        s = e - OVERLAP


def cut(track, s, e, dst):
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-ss", "%.3f" % s, "-t", "%.3f" % (e - s), "-i", os.path.join(WAV, "track%02d.wav" % track),
                    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", dst], check=True)


def one(job, url, key, redo):
    track, i, s, e = job
    f = os.path.join(RAW, "track%02d_c%02d.json" % (track, i))
    if os.path.exists(f) and not redo:
        return job, "skip"
    tmpwav = os.path.join(RAW, "track%02d_c%02d.wav" % (track, i))
    cut(track, s, e, tmpwav)
    last = None
    for attempt in range(5):
        try:
            t0 = time.time()
            j = post(url, key, tmpwav, PROMPT)
            j["_offset"], j["_end"], j["_elapsed"] = s, e, round(time.time() - t0, 2)
            json.dump(j, open(f + ".tmp", "w"))
            os.replace(f + ".tmp", f)
            os.remove(tmpwav)
            return job, "ok %.0fs" % j["_elapsed"]
        except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as ex:
            last = ex
            time.sleep(3 + 5 * attempt)
    return job, f"FAILED {last}"


VOCAL = re.compile(r"^(?:(?:a+h+|o+h+|u+h+|u+g+h+|a+w+|o+o+h+|w+h*o+a+h*|h+m+|m+|h+u+h+|a+r+g+h+|o+f+|e+h+|a+a+h+|la|na|da|ba)[\s.,!?-]*)+$", re.I)
HALLU = re.compile(r"^(?:thank(?:s| you)(?: (?:very much|for watching|for listening))?|bye|you|\.+|music|\[.*\]|\(.*\))[.! ]*$|^thanks for watching|^subtitles? by|^please subscribe", re.I)


def classify(text, nsp, lp, cr):
    t = text.strip()
    if not t:
        return False, "empty"
    if VOCAL.match(t):
        return False, "vocal"
    if HALLU.match(t):
        return False, "hallucination?"
    if (nsp or 0) > 0.5:
        return False, "no_speech_prob"
    if lp is not None and lp < -1.0:
        return False, "low_confidence"
    if cr is not None and cr > 2.4:
        return False, "repetition"
    return True, None


def merge():
    tracks = {}
    for t in range(2, 22):
        files = sorted(f for f in os.listdir(RAW) if f.startswith("track%02d_c" % t) and f.endswith(".json")) if os.path.isdir(RAW) else []
        if not files:
            continue
        segs = []
        js = [json.load(open(os.path.join(RAW, f))) for f in files]
        for n, j in enumerate(js):
            off = j["_offset"]
            lo = off + (OVERLAP / 2 if n > 0 else 0)
            hi = j["_end"] - (OVERLAP / 2 if n < len(js) - 1 else 0)
            for s in j.get("segments", []):
                mid = off + (s["start"] + s["end"]) / 2
                if not (lo <= mid < hi or (n == len(js) - 1 and mid >= lo)):
                    continue
                words = [{"w": w["word"].strip(), "start": round(off + w["start"], 3), "end": round(off + w["end"], 3), "p": w.get("probability")}
                         for w in (s.get("words") or [])]
                if not words:   # server without per-segment words: take the top-level list that falls in the segment
                    words = [{"w": w["word"].strip(), "start": round(off + w["start"], 3), "end": round(off + w["end"], 3), "p": w.get("probability")}
                             for w in (j.get("words") or []) if s["start"] - 0.01 <= w["start"] <= s["end"] + 0.01]
                sp, flag = classify(s["text"], s.get("no_speech_prob"), s.get("avg_logprob"), s.get("compression_ratio"))
                segs.append({"start": round(off + s["start"], 3), "end": round(off + s["end"], 3), "text": s["text"].strip(), "avg_logprob": s.get("avg_logprob"),
                             "no_speech_prob": s.get("no_speech_prob"), "compression_ratio": s.get("compression_ratio"), "speech": sp, "flag": flag, "words": words})
        segs.sort(key=lambda x: x["start"])
        # the same sentence in two chunks (overlap): drop an exact repeat that starts within 2 s of the previous one
        dd = []
        for s in segs:
            if dd and s["text"].lower() == dd[-1]["text"].lower() and abs(s["start"] - dd[-1]["start"]) < 2.0:
                continue
            dd.append(s)
        dur = js[-1]["_end"]
        spsec = sum(s["end"] - s["start"] for s in dd if s["speech"])
        tracks[str(t)] = {"duration": dur, "speech_seconds": round(spsec, 1), "music_only": spsec < 4.0, "segments": dd}
    os.makedirs(OUT, exist_ok=True)
    json.dump(tracks, open(os.path.join(OUT, "en.json"), "w"), indent=1)
    with open(os.path.join(OUT, "en.tsv"), "w") as f:
        for t, e in tracks.items():
            for s in e["segments"]:
                f.write("\t".join([t, str(round(s["start"] * 1000)), str(round(s["end"] * 1000)), "1" if s["speech"] else "0", (s["flag"] or ""), " ".join(s["text"].split())]) + "\n")
    lines = ["track  length   speech   verdict"]
    for t, e in tracks.items():
        n = sum(1 for s in e["segments"] if s["speech"])
        lines.append("%5s %6.1fs %7.1fs   %s (%d segments)" % (t, e["duration"], e["speech_seconds"], "MUSIC ONLY" if e["music_only"] else "speech", n))
    open(os.path.join(OUT, "summary.txt"), "w").write("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prepare", action="store_true")
    ap.add_argument("--merge", action="store_true")
    ap.add_argument("--only", default="")
    ap.add_argument("--redo", action="store_true")
    ap.add_argument("--jobs", type=int, default=1)
    a = ap.parse_args()
    if a.prepare:
        return prepare()
    if a.merge:
        return merge()
    e = env()
    url, key = e.get("WHISPER_URL"), e.get("WHISPER_API_KEY")
    if not url or not key:
        sys.exit("WHISPER_URL / WHISPER_API_KEY missing (work/whisper.env)")
    if not server_up(url, key):
        sys.exit("the Whisper server is not reachable: turn it on and run again (nothing was sent)")
    os.makedirs(RAW, exist_ok=True)
    prepare()
    import wave
    only = {int(x) for x in a.only.split(",") if x}
    jobs = []
    for t in range(2, 22):
        if only and t not in only:
            continue
        with wave.open(os.path.join(WAV, "track%02d.wav" % t)) as w:
            dur = w.getnframes() / w.getframerate()
        for i, s, e2 in chunks(dur):
            jobs.append((t, i, s, e2))
    print(len(jobs), "chunks")
    with ThreadPoolExecutor(a.jobs) as ex:
        for job, st in ex.map(lambda j: one(j, url, key, a.redo), jobs):
            print("track %02d chunk %d [%.0f-%.0f s] %s" % (job[0], job[1], job[2], job[3], st), flush=True)
    merge()


if __name__ == "__main__":
    main()
