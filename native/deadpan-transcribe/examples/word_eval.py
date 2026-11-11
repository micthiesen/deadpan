#!/usr/bin/env python3
"""Word-start/end error of Deadpan's recognizer against LibriSpeech word truth.

Truth: Montreal Forced Aligner alignments of LibriSpeech test-clean (Lugosch et
al., zenodo 2619474). Usage:

  word_eval.py prep   ROOT [N]    pick N utterances, write 16 kHz f32le + truth
  word_eval.py run    ROOT MODEL  run the word_eval example, store token JSON
  word_eval.py score  ROOT [mode] score the stored run (mode: worker = shipped conversion; heuristic = old whisper times)

WORD_EVAL_SEED (default 7) picks the sample. ROOT is ~/Library/Developer/Deadpan/acceptance/speech (outside the repository).
"""
import difflib, io, time, urllib.request, glob, json, os, random, re, statistics, subprocess, sys, zipfile


def norm(w):
    return re.sub(r"[^a-z0-9']", "", w.lower())


def load_truth(zip_path, wanted):
    """{utt: [(word, start_s, end_s)]} from the TextGrid words tier."""
    out = {}
    with zipfile.ZipFile(zip_path) as z:
        for name in z.namelist():
            utt = os.path.basename(name)[:-9]
            if utt not in wanted or not name.endswith(".TextGrid"):
                continue
            text = z.read(name).decode()
            tier = text.split('name = "words"')[1]
            tier = tier.split("item [")[0]
            out[utt] = [(w, float(a), float(b)) for a, b, w in re.findall(
                r'xmin = ([\d.]+)\s+xmax = ([\d.]+)\s+text = "([^"]*)"', tier) if w.strip()]
    return out


URL = "https://zenodo.org/api/records/2619474/files/librispeech_alignments.zip/content"
UA = {"User-Agent": "OpenAI File Downloader, XaiImageApiFetch/1.0"}


class RemoteZip(io.BufferedIOBase):
    """Read-only file over HTTP range requests (the alignment zip is 623 MB)."""

    BLOCK = 1 << 18

    def __init__(self, size):
        self.size, self.pos, self.cache = size, 0, {}

    def seekable(self):
        return True

    def tell(self):
        return self.pos

    def seek(self, offset, whence=0):
        self.pos = offset if whence == 0 else self.pos + offset if whence == 1 else self.size + offset
        return self.pos

    def _block(self, i):
        if i not in self.cache:
            a, b = i * self.BLOCK, min(self.size, (i + 1) * self.BLOCK) - 1
            req = urllib.request.Request(URL, headers={**UA, "Range": f"bytes={a}-{b}"})
            for attempt in range(12):  # zenodo answers 429 under bursts
                try:
                    self.cache[i] = urllib.request.urlopen(req).read()
                    break
                except Exception:
                    time.sleep(5 * (attempt + 1))
            else:
                raise SystemExit("zenodo download failed")
        return self.cache[i]

    def read(self, k=-1):
        k = self.size - self.pos if k < 0 else min(k, self.size - self.pos)
        out = b""
        while k > 0:
            block = self._block(self.pos // self.BLOCK)
            piece = block[self.pos % self.BLOCK:][:k]
            out += piece
            self.pos += len(piece)
            k -= len(piece)
        return out


def fetch_truth(names, target):
    zin = zipfile.ZipFile(RemoteZip(623023332))
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as out:
        for name in names:
            out.writestr(name, zin.read(name))


SEED = int(os.environ.get("WORD_EVAL_SEED", "7"))


def media(root, per=50):
    """Shipped-path inputs: the sample as long MP4s (black picture, AAC audio)
    for `deadpan-cli project create-original` + `transcribe`; truth per file."""
    d = evaldir(root, SEED)
    truth = json.load(open(f"{d}/truth.json"))
    utts = sorted(truth)
    out = f"{d}/media"
    os.makedirs(out, exist_ok=True)
    for n in range(0, len(utts), per):
        group, offset, words, pcm = utts[n:n + per], 0.0, [], b""
        for utt in group:
            raw = open(f"{d}/{utt}.f32", "rb").read()
            words += [(w, a + offset, b + offset) for w, a, b in truth[utt]]
            pcm += raw + b"\0" * 4 * 6400
            offset += len(raw) / 4 / 16000 + 0.4
        name = f"part{n // per}"
        open(f"{out}/{name}.f32", "wb").write(pcm)
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", "16000", "-ac", "1",
                        "-i", f"{out}/{name}.f32", "-f", "lavfi", "-i", f"color=black:s=320x240:r=30:d={offset:.2f}",
                        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-shortest",
                        f"{out}/{name}.mp4"], check=True)
        json.dump(words, open(f"{out}/{name}.truth.json", "w"))
        os.remove(f"{out}/{name}.f32")
    print(len(utts), "utterances in", -(-len(utts) // per), "files")


def bundle(root, cli):
    """Score the shipped path: `cli` is a bundle's Contents/MacOS/deadpan-cli
    with the packs installed; runs create-original + transcribe + transcript."""
    d = evaldir(root, SEED) + "/media"
    scratch = f"/tmp/deadpan-finish/words/bundle{SEED}"
    subprocess.run(["rm", "-rf", scratch], check=True)
    os.makedirs(scratch)
    se, ee, total, matched = [], [], 0, 0
    for mp4 in sorted(glob.glob(f"{d}/part*.mp4")):
        name = os.path.basename(mp4)[:-4]
        project = f"{scratch}/{name}.deadpan"
        subprocess.run([cli, "project", "create-original", project, mp4], check=True, capture_output=True)
        out = subprocess.run([cli, "transcribe", project], check=True, capture_output=True, text=True).stdout
        info = json.loads(out)
        print(name, "words", info["words"], "word_times", json.dumps(info["word_times"]))
        listing = json.loads(subprocess.run([cli, "transcript", project], check=True, capture_output=True, text=True).stdout)
        hyp = [(norm(w["text"]), w["start_cs"] / 100, w["end_cs"] / 100)
               for w in listing["transcripts"][0]["transcript"]["words"]]
        ref = [(norm(w), a, b) for w, a, b in json.load(open(f"{d}/{name}.truth.json"))]
        total += len(ref)
        m = difflib.SequenceMatcher(None, [h[0] for h in hyp], [x[0] for x in ref], autojunk=False)
        for a, b, size in m.get_matching_blocks():
            for k in range(size):
                se.append(hyp[a + k][1] - ref[b + k][1])
                ee.append(hyp[a + k][2] - ref[b + k][2])
                matched += 1
    print(f"bundled transcribe: truth words={total} matched={matched}")
    stats("start", se)
    stats("end  ", ee)


def evaldir(root, seed):
    return f"{root}/eval" if seed == 7 else f"{root}/eval{seed}"


def prep(root, n, seed=SEED):
    flacs = sorted(glob.glob(f"{root}/corpus/LibriSpeech/test-clean/*/*/*.flac"))
    random.Random(seed).shuffle(flacs)
    # spread over speakers: at most 6 utterances each
    picked, per = [], {}
    for f in flacs:
        spk = f.split("/")[-3]
        if per.get(spk, 0) < 6:
            per[spk] = per.get(spk, 0) + 1
            picked.append(f)
        if len(picked) == n:
            break
    d = evaldir(root, seed)
    os.makedirs(d, exist_ok=True)
    utts = {os.path.basename(f)[:-5]: f for f in picked}
    grids = f"{d}/textgrids.zip"
    if not os.path.exists(grids):
        fetch_truth(["test-clean/" + "/".join(f.split("/")[-3:])[:-5] + ".TextGrid" for f in picked], grids)
    truth = load_truth(grids, set(utts))
    keep = {}
    for utt, f in utts.items():
        if utt not in truth:
            continue
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", f, "-ac", "1", "-ar", "16000",
                        "-f", "f32le", f"{d}/{utt}.f32"], check=True)
        keep[utt] = truth[utt]
    k = int(os.environ.get("WORD_EVAL_CONCAT", "1"))
    if k > 1:  # long clips: k same-speaker utterances 0.4 s apart, to exercise windows
        import struct
        by_speaker = {}
        for utt in sorted(keep):
            by_speaker.setdefault(utt.split("-")[0], []).append(utt)
        joined = {}
        for spk, group in by_speaker.items():
            for g in range(0, len(group) - k + 1, k):
                parts, offset, words, pcm = group[g:g + k], 0.0, [], b""
                for utt in parts:
                    raw = open(f"{d}/{utt}.f32", "rb").read()
                    words += [(w, a + offset, b + offset) for w, a, b in keep[utt]]
                    pcm += raw + struct.pack("<f", 0.0) * 6400
                    offset += len(raw) / 4 / 16000 + 0.4
                    os.remove(f"{d}/{utt}.f32")
                name = parts[0] + f"+{k}"
                open(f"{d}/{name}.f32", "wb").write(pcm)
                joined[name] = words
        for f in glob.glob(f"{d}/*.f32"):
            if os.path.basename(f)[:-4] not in joined:
                os.remove(f)
        keep = joined
    json.dump(keep, open(f"{d}/truth.json", "w"))
    print(len(keep), "utterances")


def run(root, model):
    binary = os.environ.get("WORD_EVAL_BIN", "target/lane-words/release/examples/word_eval")
    files = sorted(glob.glob(f"{evaldir(root, SEED)}/*.f32"))
    with open(f"{evaldir(root, SEED)}/run.jsonl", "w") as out:
        subprocess.run([binary, model, *files], stdout=out, check=True)


def prevtok(s, tok):
    toks = [t for t in s["tokens"] if not t["text"].startswith("[_") and t["text"].strip()]
    i = toks.index(tok)
    return toks[i - 1] if i else None


def words_from(segments, mode):
    """Group tokens into words like Transcript::from_segments; mode picks times."""
    words = []
    for s in segments:
        toks = [t for t in s["tokens"] if not t["text"].startswith("[_") and t["text"].strip()]
        for i, t in enumerate(toks):
            punct = not any(c.isalnum() for c in t["text"].strip())
            if punct and words and not (t["text"].startswith(" ")):
                continue
            if t["text"].startswith(" ") or not words or words[-1]["seg"] != id(s):
                if punct:
                    continue
                words.append({"text": t["text"].strip(), "seg": id(s), "toks": [t], "next": toks[i + 1] if i + 1 < len(toks) else None, "s": s})
            else:
                words[-1]["toks"].append(t)
                words[-1]["next"] = toks[i + 1] if i + 1 < len(toks) else None
                words[-1]["text"] += t["text"].strip()
    out = []
    for w in words:
        first, last, nxt, s = w["toks"][0], w["toks"][-1], w["next"], w["s"]
        if mode == "heuristic":
            start, end = first["t0"], last["t1"]
        else:  # dtw variants
            start = first["dtw"]
            if mode == "dtw":
                end = nxt["dtw"] if nxt else s["t1"]
            elif mode == "dtw_prev":  # start = previous token's DTW time, end = last token's
                prev = prevtok(s, first)
                start = prev["dtw"] if prev else s["t0"]
                end = last["dtw"]
            elif mode == "dtw_prev_h":
                prev = prevtok(s, first)
                start = prev["dtw"] if prev else s["t0"]
                end = min(last["t1"], last["dtw"]) if False else last["dtw"]
            elif mode == "dtw_h":  # end: heuristic, but never past the next start
                end = min(last["t1"], nxt["dtw"]) if nxt else last["t1"]
            else:
                raise SystemExit("unknown mode " + mode)
        out.append((norm(w["text"]), start / 100, max(end, start) / 100))
    return out


def stats(label, errs):
    a = sorted(abs(e) for e in errs)
    if not a:
        return
    p = lambda q: a[min(len(a) - 1, int(q * len(a)))]
    print(f"{label}: n={len(a)} median_abs={1000*statistics.median(a):.0f}ms "
          f"signed_median={1000*statistics.median(errs):+.0f}ms p90={1000*p(.9):.0f}ms "
          f"within60={100*sum(x <= .06 for x in a)/len(a):.0f}% within100={100*sum(x <= .1 for x in a)/len(a):.0f}%")


def score(root, mode):
    truth = json.load(open(f"{evaldir(root, SEED)}/truth.json"))
    se, ee, total, matched, secs, audio = [], [], 0, 0, 0.0, 0.0
    refined, ai, aa = {}, 0.0, 0.0
    path = f"{evaldir(root, SEED)}/refined.jsonl"
    if mode == "aligner":
        for line in open(path):
            q = json.loads(line)
            refined[q["file"]] = q
            ai += q["seconds"]
            aa += q["audio_seconds"]
    for line in open(f"{evaldir(root, SEED)}/run.jsonl"):
        r = json.loads(line)
        utt = os.path.basename(r["file"])[:-4]
        secs += r["seconds"]
        audio += os.path.getsize(r["file"]) / 4 / 16000
        if mode == "aligner":  # words from refined.jsonl (word_refine_eval)
            hyp = [(norm(t), s / 100, e / 100) for t, s, e in refined[r["file"]]["words"]]
        elif mode == "worker":  # Transcript::from_segments over the worker's output
            hyp = [(norm(t), s / 100, e / 100) for t, s, e in r["words"]]
        else:
            hyp = words_from(r["segments"], mode)
        ref = [(norm(w), s, e) for w, s, e in truth[utt]]
        total += len(ref)
        m = difflib.SequenceMatcher(None, [h[0] for h in hyp], [x[0] for x in ref], autojunk=False)
        for a, b, size in m.get_matching_blocks():
            for k in range(size):
                se.append(hyp[a + k][1] - ref[b + k][1])
                ee.append(hyp[a + k][2] - ref[b + k][2])
                matched += 1
    print(f"mode={mode} truth words={total} matched={matched} recognizer {secs:.1f}s for {audio:.0f}s audio")
    stats("start", se)
    stats("end  ", ee)
    if mode == "aligner":
        print(f"aligner {ai:.1f}s for {aa/60:.1f} min of audio = {ai/(aa/60):.1f}s/min")


if __name__ == "__main__":
    cmd, root = sys.argv[1], os.path.expanduser(sys.argv[2])
    if cmd == "prep":
        prep(root, int(sys.argv[3]) if len(sys.argv) > 3 else 200)
    elif cmd == "bundle":
        bundle(root, sys.argv[3])
    elif cmd == "media":
        media(root)
    elif cmd == "run":
        run(root, sys.argv[3])
    else:
        score(root, sys.argv[3] if len(sys.argv) > 3 else "heuristic")
