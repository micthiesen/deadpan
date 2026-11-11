#!/usr/bin/env python3
"""Sound sanity scan of a rendered acceptance movie (brief item F9).

usage: scan.py MOVIE ORIGINAL SPANS.json [--report OUT.json]

SPANS.json (written by session.py):
  {"fps": [24, 1],
   "cuts": [project frames where the edit changes content],
   "unedited": [{"out_frame": N, "orig_frame": M, "frames": K}, ...]}

Checks, all on 48 kHz decoded PCM from ffmpeg (no numpy: C-speed builtins only):
  clicks    step size at each cut compared with the typical step around it,
            plus the same ratio at random non-cut points as the baseline
  clipping  samples at or beyond full scale
  av        audio-vs-video start offset of the file (ffprobe) and the lag
            of the first unedited span against the Original (correlation)
  level     median 100 ms RMS difference (dB) between movie and Original
            over spans where nothing was authored
Exit status is zero only when every check passes.
"""
import array, json, math, operator, random, subprocess, sys

RATE = 48000


def decode(path):
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-vn", "-map", "0:a:0", "-f", "f32le",
         "-ac", "2", "-ar", str(RATE), "-"],
        check=True, stdout=subprocess.PIPE).stdout
    data = array.array("f")
    data.frombytes(out)
    return data  # interleaved L,R


def boundary(frame, fps):
    # B(f) = round_even(f * 48000 * den / num), exact integer arithmetic.
    num, den = fps
    n, d = frame * RATE * den, num
    q, r = divmod(n, d)
    if 2 * r > d or (2 * r == d and q % 2 == 1):
        q += 1
    return q


def step_stats(ch, i, half=480):
    """|step| at i and the largest |step| in the 10 ms either side (cut excluded).

    A cut that was faded or lands on matching signal steps no more than the
    material around it; a click steps several times more. The largest, not
    the median, because a cut beside digital silence has a zero median."""
    lo, hi = max(1, i - half), min(len(ch) - 1, i + half)
    around = [abs(ch[k] - ch[k - 1]) for k in range(lo, hi) if abs(k - i) > 4]
    natural = max(around) if around else 0.0
    here = max(abs(ch[k] - ch[k - 1]) for k in range(max(1, i - 2), min(len(ch), i + 3)))
    return here, natural


def click_ratio(ch, i):
    here, natural = step_stats(ch, i)
    return here / max(natural, 1e-4), here


def rms_blocks(ch, start, count, block=4800):
    out = []
    for b in range(count):
        seg = ch[start + b * block: start + (b + 1) * block]
        if len(seg) < block:
            break
        out.append(math.sqrt(math.fsum(map(operator.mul, seg, seg)) / block))
    return out


def db(x):
    return -120.0 if x <= 1e-6 else 20 * math.log10(x)


def envelope(ch, start, ms_count):
    step = RATE // 1000
    env = []
    for m in range(ms_count):
        seg = ch[start + m * step: start + (m + 1) * step]
        env.append(math.fsum(map(abs, seg)))
    return env


def best_lag(a, b, window=80):
    """Lag (ms) of b relative to a maximising the mean-removed correlation."""
    best = (None, -2.0)
    ma = sum(a) / len(a)
    a = [x - ma for x in a]
    na = math.sqrt(sum(x * x for x in a)) or 1.0
    for lag in range(-window, window + 1):
        seg = b[window + lag: window + lag + len(a)]
        if len(seg) < len(a):
            continue
        mb = sum(seg) / len(seg)
        sb = [x - mb for x in seg]
        nb = math.sqrt(sum(x * x for x in sb)) or 1.0
        c = sum(map(operator.mul, a, sb)) / (na * nb)
        if c > best[1]:
            best = (lag, c)
    return best


def probe_start(path):
    result = {}
    for kind in ("v", "a"):
        out = subprocess.run(
            ["ffprobe", "-v", "error", "-select_streams", kind + ":0", "-show_entries",
             "stream=start_time,duration", "-of", "json", path],
            check=True, capture_output=True, text=True).stdout
        result[kind] = json.loads(out)["streams"][0]
    return result


def main():
    args = sys.argv[1:]
    report_path = None
    if "--report" in args:
        k = args.index("--report")
        report_path = args[k + 1]
        del args[k:k + 2]
    movie, original, spans_path = args
    spans = json.load(open(spans_path))
    fps = tuple(spans["fps"])
    report = {"movie": movie, "checks": {}}
    ok = True

    pcm = decode(movie)
    ref = decode(original)
    left = pcm[0::2]
    ref_left = ref[0::2]
    total = len(left)
    report["samples"] = total

    # clipping: full-scale samples anywhere in the movie
    block = 48000
    clipped = 0
    peak = 0.0
    for s in range(0, len(pcm), block):
        seg = pcm[s:s + block]
        hi, lo = max(seg), min(seg)
        peak = max(peak, hi, -lo)
        if hi >= 0.9999 or lo <= -0.9999:
            clipped += sum(1 for x in seg if abs(x) >= 0.9999)
    report["checks"]["clipping"] = {"full_scale_samples": clipped, "peak": peak, "pass": clipped == 0}

    # clicks at cuts versus a random baseline
    cuts = [boundary(f, fps) for f in spans["cuts"]]
    cuts = [c for c in cuts if 480 < c < total - 480]
    rows = []
    for c in cuts:
        # a click lands on the sample step at the cut, either channel
        r = max(click_ratio(left, c)[0], click_ratio(pcm[1::2], c)[0])
        rows.append({"sample": c, "ratio": r})
    rng = random.Random(7)
    base = sorted(click_ratio(left, rng.randrange(1000, total - 1000))[0] for _ in range(400))
    base_p99 = base[int(len(base) * 0.99)]
    threshold = max(1.5, base_p99 * 1.5)
    bad = [r for r in rows if r["ratio"] > threshold]
    report["checks"]["clicks"] = {
        "cuts": len(rows), "baseline_p99_ratio": base_p99, "threshold": threshold,
        "worst": sorted(rows, key=lambda r: -r["ratio"])[:5], "flagged": bad,
        "pass": not bad}

    # A/V offset: stream starts plus lag of the first unedited span against the Original
    starts = probe_start(movie)
    av = float(starts["a"].get("start_time", 0)) - float(starts["v"].get("start_time", 0))
    frame_s = fps[1] / fps[0]
    lag_ms = None
    first = next((u for u in spans["unedited"] if u["frames"] >= 24 * 20), None)
    corr = None
    if first:
        o = boundary(first["out_frame"], fps) + RATE * 2
        r0 = boundary(first["orig_frame"], fps) + RATE * 2
        a = envelope(ref_left, r0, 10000)
        b = envelope(left, o - 80 * 48, 10000 + 160)
        lag_ms, corr = best_lag(a, b)
    off = abs(av) + (abs(lag_ms) / 1000 if lag_ms is not None else 0)
    report["checks"]["av_offset"] = {
        "stream_start_offset_s": av, "lag_vs_original_ms": lag_ms, "correlation": corr,
        "frame_s": frame_s, "pass": off <= frame_s and lag_ms is not None}

    # level where nothing was authored
    diffs = []
    for u in spans["unedited"]:
        n = min(u["frames"] * RATE * fps[1] // fps[0] // 4800, 600)
        o = boundary(u["out_frame"], fps)
        r0 = boundary(u["orig_frame"], fps)
        if o + n * 4800 > total or r0 + n * 4800 > len(ref_left):
            n = min(n, (total - o) // 4800, (len(ref_left) - r0) // 4800)
        a = rms_blocks(left, o, n)
        b = rms_blocks(ref_left, r0, n)
        pairs = [(x, y) for x, y in zip(a, b) if y > 1e-3 and x > 1e-3]
        d = sorted(db(x) - db(y) for x, y in pairs)
        if d:
            diffs.append({"out_frame": u["out_frame"], "blocks": len(d), "median_db": d[len(d) // 2],
                          "p5_db": d[len(d) // 20], "p95_db": d[len(d) * 19 // 20]})
    worst = max((abs(x["median_db"]) for x in diffs), default=None)
    report["checks"]["level"] = {"spans": diffs, "worst_median_abs_db": worst,
                                 "pass": worst is not None and worst < 0.5}

    for name, c in report["checks"].items():
        ok &= c["pass"]
        print(f"{name}: {'PASS' if c['pass'] else 'FAIL'}  "
              + json.dumps({k: v for k, v in c.items() if k not in ('pass', 'spans', 'worst', 'flagged')}))
    report["pass"] = ok
    if report_path:
        json.dump(report, open(report_path, "w"), indent=1)
    sys.exit(0 if ok else 1)


main()
