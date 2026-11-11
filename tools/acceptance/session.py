#!/usr/bin/env python3
"""Headless acceptance session on real footage (brief item F2).

usage: python3 -I session.py --cli DIR --video FILE --work DIR [--reuse PKG] [--model GGML] [--phrase TEXT]

DIR holds the release deadpan-cli plus deadpan-transcribe and
deadpan-media-worker. WORK is a new private directory for the project, the
rendered movie and the reports. Every step is recorded as pass or fail with
the command and message; a failing step does not stop the session, because a
product failure on real footage is the result this session exists to find.
Nothing here edits the project except through deadpan-cli commands.
"""
import argparse, hashlib, json, os, subprocess, sys, time

ARGS = None
RESULTS = []
FPS = (24, 1)


def cli(*args, stdin_json=None, check=True):
    cmd = [os.path.join(ARGS.cli, "deadpan-cli"), *map(str, args)]
    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    took = time.time() - started
    out = proc.stdout.strip()
    try:
        data = json.loads(out) if out else None
    except json.JSONDecodeError:
        data = out  # JSON-lines commands
    if proc.returncode != 0 and check:
        err = proc.stderr.strip() or out
        raise Failure(" ".join(cmd[1:])[:300], err[:1200], took)
    return data, proc, took


class Failure(Exception):
    def __init__(self, command, message, took=0.0):
        super().__init__(message)
        self.command, self.message, self.took = command, message, took


def step(name):
    def wrap(fn):
        started = time.time()
        try:
            detail = fn()
            RESULTS.append({"step": name, "pass": True, "detail": detail,
                            "seconds": round(time.time() - started, 1)})
            print(f"PASS {name}: {json.dumps(detail)[:200]}")
        except Failure as failure:
            RESULTS.append({"step": name, "pass": False, "command": failure.command,
                            "message": failure.message, "seconds": round(time.time() - started, 1)})
            print(f"FAIL {name}: {failure.command}\n     {failure.message[:400]}")
        except Exception as error:  # script bug or unexpected product shape
            RESULTS.append({"step": name, "pass": False, "command": "(script)",
                            "message": f"{type(error).__name__}: {error}",
                            "seconds": round(time.time() - started, 1)})
            print(f"FAIL {name}: {type(error).__name__}: {error}")
        return fn
    return wrap


# ------------------------------------------------------------ project state

def dump():
    return cli("project", "dump", PROJECT, "--json")[0]


def duration(doc, node):
    n = doc["nodes"][node]["kind"]
    t = n["type"]
    if t == "source":
        return n["source"]["duration"]
    if t == "hold":
        return n["recipe"]["duration"]
    if t == "sequence":
        return sum(duration(doc, c) for c in n["children"])
    if t == "retime":
        return n.get("duration") or n["retime"]["duration"]
    if t == "repeat":
        return repeat_duration(doc, node, n)
    raise Failure("dump", f"unknown node kind {t}")


def repeat_duration(doc, node, n):
    # Report-only helper; the plan is authoritative. Uses the stored total
    # when the document carries one.
    for key in ("duration", "total"):
        if key in n:
            return n[key]
    raise Failure("dump", "repeat duration not in the document")


def root_layout(doc):
    """[(child, start, frames)] for the root sequence's direct children."""
    out, at = [], 0
    for child in doc["nodes"][doc["root"]]["kind"]["children"]:
        d = project_duration_of(doc, child)
        out.append((child, at, d))
        at += d
    return out


def project_duration_of(doc, child):
    # `inspect-plan` would be exact; the structural walk is enough for placing
    # cursors, and the final length is checked against the rendered movie.
    return duration(doc, child)


def envelope(doc, operation, bank=None):
    if bank is None:
        bank = cli("macro", "inspect", PROJECT)[0]["bank_version"]
    return {"protocol": 1, "project_id": doc["project_id"], "expected_revision": doc["revision_id"],
            "expected_bank_version": bank, "operation": operation}


def apply(instructions, cursor, selected_child=None, visual=None, dry=False):
    doc = dump()
    op = {"type": "apply", "program": {"instructions": instructions}, "parent": doc["root"],
          "cursor": cursor, "selected_child": selected_child, "visual_selection": visual}
    path = os.path.join(WORK, "request.json")
    json.dump(envelope(doc, op), open(path, "w"))
    args = ["macro", PROJECT, "--json", path] + (["--dry-run"] if dry else [])
    return cli(*args)[0]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


# ------------------------------------------------------------------- session

CTX = {"cursor": 0, "selected_child": None}
SHIFT = 0            # edit frames added so far before any later original frame
REGIONS = []         # (orig_start, orig_end, delta) of authored changes, original frames
CUTS = []            # edit frames where content changes


NO_BEAT = object()  # "no selected child": the instruction resolves it from the cursor


def commit(instructions, cursor=None, selected=NO_BEAT, visual=None):
    """One committed `apply`; returns the saved context for the next step."""
    out = apply(instructions, CTX["cursor"] if cursor is None else cursor,
                None if selected is NO_BEAT else selected, visual)
    if not out.get("committed"):
        raise Failure("macro apply", "not committed: " + json.dumps(out)[:400])
    CTX.update(cursor=out["context"]["cursor"], selected_child=out["context"]["selected_child"])
    return out


def child_at(frame):
    """The root's direct child that covers an Edit frame (the beat a cursor sits in)."""
    out = cli("inspect-plan", PROJECT, "--frame", frame)[0]
    chain = out["sample"]["framing"]
    return chain[-2]["instance"]["node"] if len(chain) >= 2 else None


def at(orig_frame):
    """Edit frame of an Original frame that no authored change precedes in time order."""
    return orig_frame + SHIFT


def main():
    global ARGS, WORK, PROJECT, SHIFT
    p = argparse.ArgumentParser()
    p.add_argument("--cli", required=True)
    p.add_argument("--video", required=True)
    p.add_argument("--work", required=True)
    p.add_argument("--reuse", help="clone this package (already created and transcribed) instead")
    p.add_argument("--model", default=os.path.expanduser(
        "~/Library/Application Support/Deadpan/Models/whisper-base-en/1/ggml-base.en.bin"))
    p.add_argument("--phrase", default="main engine")
    p.add_argument("--no-render", action="store_true", help="skip render and verify-export (run them apart)")
    ARGS = p.parse_args()
    WORK = ARGS.work
    os.makedirs(WORK, exist_ok=False)
    PROJECT = os.path.join(WORK, "acceptance.deadpan")
    state = {"phrase": ARGS.phrase}

    if ARGS.reuse:
        subprocess.run(["cp", "-cR", ARGS.reuse, PROJECT], check=True)
        RESULTS.append({"step": "create and transcribe", "pass": True, "detail": "cloned " + ARGS.reuse})
    else:
        @step("create project from the video")
        def _():
            out, _, took = cli("project", "create-original", PROJECT, ARGS.video)
            return {"frames": out["created"]["duration_frames"], "seconds": round(took)}

        @step("transcribe")
        def _():
            out, _, took = cli("transcribe", PROJECT, "--model", ARGS.model, "--sha256", sha256(ARGS.model))
            return {"seconds": round(took), "words": out["words"], "approximate": out["approximate_words"]}

    doc0 = dump()
    original_frames = duration(doc0, doc0["root"])
    asset = next(iter(doc0["assets"]))
    state["original_frames"] = original_frames
    CTX["selected_child"] = None

    @step("search a word")
    def _():
        out = cli("transcript", PROJECT, "--search", ARGS.phrase)[0]
        m = out["transcripts"][0]["matches"][0]
        # frames from exact sample bounds: floor the start, ceil the end
        s = int(m["original_sample_start"]["numerator"]) * FPS[0] // 48000
        e = -(-int(m["original_sample_end"]["numerator"]) * FPS[0] // 48000)
        state["word"] = (s, e)
        return {"phrase": ARGS.phrase, "frames": [s, e], "matches": len(out["transcripts"][0]["matches"])}

    @step("repeat the word 3 plays with 3 dB gain and 1/16 zoom step")
    def _():
        global SHIFT
        s, e = state["word"]
        out = commit([{"type": "repeat", "selector": {"type": "visual_selection"}, "plays": 3,
                        "escalation": {"gain_step": 3000,
                                       "zoom": {"step": {"numerator": "1", "denominator": "16"}}}}],
                     cursor=s, visual={"type": "time", "anchor": e, "head": s, "extending": False})
        delta = out["edit"]["duration_delta"]
        REGIONS.append((s, e, delta))
        CUTS.extend([s, s + (e - s), s + 2 * (e - s), e + delta])
        SHIFT += delta
        return {"delta": delta, "repeat": CTX["selected_child"]}

    pause_orig = 4800  # 200 s
    @step("insert a 1.5 s pause")
    def _():
        global SHIFT
        out = commit([{"type": "insert_pause", "length": {"unit": "milliseconds", "milliseconds": 1500}}],
                     cursor=at(pause_orig))
        delta = out["edit"]["duration_delta"]
        state["pause"] = {"node": CTX["selected_child"], "cursor": CTX["cursor"], "frames": delta}
        REGIONS.append((pause_orig, pause_orig, delta))
        CUTS.extend([at(pause_orig), at(pause_orig) + delta])
        SHIFT += delta
        return state["pause"]

    @step("creep on the pause (1x to 1.5x)")
    def _():
        commit([{"type": "zoom", "kind": {"type": "creep", "from": None,
                                          "to": {"numerator": "3", "denominator": "2"},
                                          "curve": "smoothstep"}, "center": "keep"}],
               cursor=state["pause"]["cursor"], selected=state["pause"]["node"])
        return "creep applied"

    cut_orig = 7200  # 300 s
    @step("cut away to another moment (Original 600 s, 2 s)")
    def _():
        doc = dump()
        bank = cli("macro", "inspect", PROJECT)[0]["bank_version"]
        op = {"type": "yank_original", "register": "a", "ordinals": {"start": 14400, "end": 14448}}
        path = os.path.join(WORK, "yank.json")
        json.dump(envelope(doc, op, bank), open(path, "w"))
        cli("macro", PROJECT, "--json", path)
        # isolate the host beat, then show the copied moment over it
        for frame in (cut_orig, cut_orig + 48):
            commit([{"type": "split"}], cursor=at(frame), selected=child_at(at(frame)))
            CUTS.append(at(frame))
        commit([{"type": "set_cutaway", "register": "a"}], cursor=at(cut_orig + 1), selected=child_at(at(cut_orig + 1)))
        return "cutaway set"

    speed_orig = 9600  # 400 s
    @step("change speed of a 5 s passage to 2x")
    def _():
        global SHIFT
        for frame in (speed_orig, speed_orig + 120):
            commit([{"type": "split"}], cursor=at(frame), selected=child_at(at(frame)))
        out = commit([{"type": "retime", "speed": {"numerator": "2", "denominator": "1"},
                       "pitch": "preserve", "wrap": False}],
                     cursor=at(speed_orig + 1), selected=child_at(at(speed_orig + 1)))
        delta = out["edit"]["duration_delta"]
        REGIONS.append((speed_orig, speed_orig + 120, delta))
        CUTS.extend([at(speed_orig), at(speed_orig + 120) + delta])
        SHIFT += delta
        return {"delta": delta}

    rev_orig = 12000  # 500 s
    @step("reverse the 1 s before a cursor")
    def _():
        global SHIFT
        out = commit([{"type": "insert_reverse", "length": {"unit": "milliseconds", "milliseconds": 1000}}],
                     cursor=at(rev_orig))
        delta = out["edit"]["duration_delta"]
        REGIONS.append((rev_orig, rev_orig, delta))
        CUTS.extend([at(rev_orig), at(rev_orig) + delta])
        SHIFT += delta
        return {"delta": delta}

    sound_orig = 14400  # 600 s
    @step("place a sound effect")
    def _():
        sting = os.path.join(WORK, "sting.wav")
        cli("sound", "--write-sting", sting)
        out = cli("project", "retain-original", PROJECT, sting)[0]
        digest = out["retained_original"]["record"]["object"]["content"]["digest"]
        doc = dump()
        req = {"protocol": 1, "registration": {
            "expected_revision": doc["revision_id"], "new_revision": "sting-registered",
            "original": {"algorithm": "blake3", "digest": digest}, "new_asset_id": "sting",
            "label": "sting", "insertion": None}, "streams": {"type": "audio_only", "stream": 0, "interpretation": "stereo_left_right"}}
        path = os.path.join(WORK, "register.json")
        json.dump(req, open(path, "w"))
        cli("project", "register-source", PROJECT, "--request-json", path)
        doc = dump()
        placed = os.path.join(WORK, "sound.json")
        frame = at(sound_orig)
        json.dump({"protocol": 1, "expected_revision": doc["revision_id"],
                   "edit": {"type": "place", "asset": "sting", "at": {"frame": frame}}}, open(placed, "w"))
        out = cli("sound", PROJECT, "--json", placed)[0]
        REGIONS.append((sound_orig, sound_orig + 48, 0))
        return {"frame": frame, "result": json.dumps(out)[:160]}

    @step("render")
    def _():
        if ARGS.no_render:
            return "skipped (--no-render)"
        out_dir = os.path.join(WORK, "Exports")
        os.makedirs(out_dir)
        out, proc, took = cli("render", PROJECT, "--output", out_dir, "--name", "acceptance.mp4")
        state["render_seconds"] = round(took)
        state["movie"] = os.path.join(out_dir, "acceptance.mp4")
        return {"seconds": round(took), "output": str(out)[:200]}

    @step("verify-export")
    def _():
        if ARGS.no_render:
            return "skipped (--no-render)"
        report = os.path.join(WORK, "verify-export.json")
        out, proc, took = cli("verify-export", PROJECT, "--movie", state["movie"], "--every", "600",
                              "--samples", "0:480000", "--report", report)
        return {"seconds": round(took), "report": report}

    # spans for the audio scan: gaps between authored regions, original -> edit frames
    spans, shift, cursor = [], 0, 0
    # regions were recorded in authoring order; order them by original frame and recompute shifts
    ordered = sorted(REGIONS)
    out_shift = 0
    for start, end, delta in ordered:
        if start > cursor:
            spans.append({"orig_frame": cursor, "out_frame": cursor + out_shift, "frames": start - cursor})
        out_shift += delta
        cursor = end
    spans.append({"orig_frame": cursor, "out_frame": cursor + out_shift,
                  "frames": original_frames - cursor})
    # a region's own cut frames, mapped through the shifts before it
    json.dump({"fps": list(FPS), "cuts": sorted(set(CUTS)), "unedited": spans},
              open(os.path.join(WORK, "spans.json"), "w"), indent=1)
    json.dump(RESULTS, open(os.path.join(WORK, "results.json"), "w"), indent=1)
    json.dump(state, open(os.path.join(WORK, "state.json"), "w"), default=str)
    print("movie:", state.get("movie"))
    sys.exit(0 if all(r["pass"] for r in RESULTS) else 1)


if __name__ == "__main__":
    main()
