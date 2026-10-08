#!/usr/bin/env python3
"""Qualify eight real Extension results through acceptance and offline export.

This deliberately mutates the source fixtures by accepting, undoing, and redoing
their Ready attempt. Run only after qualify.py has finished all eight cases.
Every invocation requires a fresh output directory and untouched before.json
authored baselines. It never changes HOME, PATH, TMPDIR, or model installation.
"""

import argparse
import copy
import datetime
import errno
import hashlib
import json
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import time
import traceback
from pathlib import Path


ROOT = Path(__file__).resolve().parent
EXPECTED_CASES = [
    {"case": f"{frames}f-{direction}", "frames": frames,
     "generated_frames": generated, "direction": direction, "motion": motion}
    for frames, generated, motion in [
        (12, 8, "still"), (24, 24, "still"),
        (48, 48, "subtle"), (72, 72, "moderate")]
    for direction in ["from_left", "from_right"]
]
EXPECTED_PROVIDER = {
    "pack_id": "ltx-2.3-q4-extension", "pack_version": "1",
    "runtime_id": "ltx-mlx", "runtime_version": "0.15.8+deadpan-extension1",
    "seed": 42107,
}
RATE = {"numerator": 24, "denominator": 1}
FILE_PROBE = """import errno,json,sys
try:
    with open(sys.argv[1], 'rb') as stream:
        count = len(stream.read(1))
    print(json.dumps({'readable': True, 'bytes_read': count}))
except OSError as error:
    print(json.dumps({'readable': False, 'errno': error.errno, 'error': str(error)}))
"""
NETWORK_PROBE = """import json,socket,sys
try:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as stream:
        stream.settimeout(3)
        stream.connect(('127.0.0.1', int(sys.argv[1])))
    print(json.dumps({'connected': True}))
except OSError as error:
    print(json.dumps({'connected': False, 'errno': error.errno, 'error': str(error)}))
"""


class QualificationFailure(RuntimeError):
    def __init__(self, message, kind="evidence_assertion_failed"):
        super().__init__(message)
        self.kind = kind


def require(condition, message, kind="evidence_assertion_failed"):
    if not condition:
        raise QualificationFailure(message, kind)


def failure_kind(error):
    if isinstance(error, QualificationFailure):
        return error.kind
    if isinstance(error, (KeyError, IndexError, TypeError, ValueError)):
        return "script_or_response_shape_error"
    if isinstance(error, OSError):
        return "environment_or_evidence_io_error"
    if isinstance(error, KeyboardInterrupt):
        return "interrupted"
    return "script_error"


def utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load(path):
    require(path.is_file() and path.stat().st_size <= 64 * 1024 * 1024,
            f"missing or excessive JSON file: {path}")
    return json.loads(path.read_text())


def save_new(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")


def last_json(path):
    remaining = path.read_text().lstrip()
    decoder = json.JSONDecoder()
    values = []
    while remaining:
        value, end = decoder.raw_decode(remaining)
        values.append(value)
        remaining = remaining[end:].lstrip()
    require(bool(values), f"empty JSON output: {path}")
    return values[-1]


class Runner:
    def __init__(self, directory, cli, timeout):
        self.directory = directory
        self.cli = cli
        self.timeout = timeout
        self.steps = []
        self.phase = "preflight"
        self.cleanup_uncertain = False

    def execute(self, label, command, profile=None):
        self.phase = label
        command = [str(part) for part in command]
        if profile is not None:
            command = ["sandbox-exec", "-f", str(profile), *command]
        stdout = self.directory / f"{label}.stdout"
        stderr = self.directory / f"{label}.stderr"
        record = {"label": label, "command": command, "started": utc_now(),
                  "sandbox_profile": str(profile) if profile else None}
        started = time.monotonic()
        process = None
        try:
            with stdout.open("x") as out, stderr.open("x") as err:
                process = subprocess.Popen(command, cwd=self.directory,
                                           stdin=subprocess.DEVNULL,
                                           stdout=out, stderr=err)
                try:
                    process.wait(timeout=self.timeout)
                except subprocess.TimeoutExpired:
                    record["timed_out"] = True
                    # Ask the CLI to perform its owned-worker teardown. Never
                    # describe a timed-out command as qualified, even if it exits.
                    self.cleanup_uncertain = True
                    process.terminate()
                    try:
                        process.wait(timeout=60)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=30)
                        record["leader_killed"] = True
                    raise QualificationFailure(
                        f"{label} timed out; worker cleanup needs inspection", "command_timeout")
            record["exit_code"] = process.returncode
            require(process.returncode == 0,
                    f"{label} exited {process.returncode}; inspect {stderr}", "cli_command_failed")
            return last_json(stdout)
        except BaseException as error:
            record["error"] = f"{type(error).__name__}: {error}"
            record["failure_kind"] = failure_kind(error)
            if process is not None:
                if process.poll() is None:
                    self.cleanup_uncertain = True
                    process.terminate()
                    try:
                        process.wait(timeout=60)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=30)
                        record["leader_killed"] = True
                record["exit_code"] = process.poll()
            raise
        finally:
            record["seconds"] = round(time.monotonic() - started, 6)
            record["finished"] = utc_now()
            self.steps.append(record)
            save_new(self.directory / f"{label}.execution.json", record)
            print(json.dumps(record), flush=True)

    def command(self, label, *arguments, profile=None):
        return self.execute(label, [self.cli, *arguments], profile)


def without_revision(document):
    value = copy.deepcopy(document)
    del value["revision_id"]
    return value


def baseline_facts(case, before):
    nodes = before["nodes"]
    root = nodes[before["root"]]["kind"]
    require(root["type"] == "sequence" and len(root["children"]) == 2,
            "fixture must have one source and one Hold at the root")
    source_ids = [node for node in root["children"] if node != "extension"]
    require(len(source_ids) == 1, "fixture has no unique source")
    source = nodes[source_ids[0]]["kind"]
    require(source["type"] == "source" and source["source"]["duration"] == 9
            and source["source"]["audio"] is None,
            "fixture must have nine silent Original pictures")
    expected_order = ([source_ids[0], "extension"] if case["direction"] == "from_left"
                      else ["extension", source_ids[0]])
    require(root["children"] == expected_order, "unexpected fixture edge")
    hold = nodes["extension"]["kind"]
    require(hold["type"] == "hold", "extension target is not a Hold")
    recipe = hold["recipe"]
    require(recipe["duration"] == case["frames"]
            and recipe["audio"] == {"type": "silence"}
            and recipe["video"]["type"] == "freeze", "unexpected fallback recipe")
    require(before["presentation_basis"]["frame_rate"] == RATE
            and before["presentation_basis"]["width"] == 768
            and before["presentation_basis"]["height"] == 320,
            "fixture must retain its native 768x320 / 24 fps basis")
    return recipe


def generation_facts(case, result, generated):
    require(result["request"] == generated["request_id"]
            and result["attempt"] == generated["attempt_id"], "result identity differs")
    require(generated["state"] == "Ready" and generated["ready"] is not None
            and generated["failure"] is None and generated["record_error"] is None,
            "generation did not record a clean Ready result")
    require(generated["hold_id"] == "extension" and generated["seed"] == 42107,
            "unexpected Hold or seed")
    require(generated["plan"]["operation"] == "extension", "not an Extension plan")
    sampling = generated["plan"]["plan"]["sampling"]
    for key, expected in {
        "direction": case["direction"], "output_frame_count": case["frames"],
        "generated_frame_count": case["generated_frames"], "context_frame_count": 9,
        "project_rate": RATE, "native_rate": RATE,
    }.items():
        require(sampling[key] == expected, f"unexpected sampling.{key}")


def accepted_facts(case, before, accepted, generated, package):
    recipe = accepted["nodes"]["extension"]["kind"]["recipe"]
    video = recipe["video"]
    require(video["type"] == "generated", "accept did not select generated pictures")
    selected = video["accepted"]
    artifact = selected["artifact"]
    fallback = before["nodes"]["extension"]["kind"]["recipe"]["video"]
    require(selected["fallback"] == fallback, "accept changed the fallback")
    require(recipe["duration"] == case["frames"], "accept changed authored duration")
    require(artifact["sampling"] == {
        "operation": "extension", "sampling": generated["plan"]["plan"]["sampling"]},
        "accepted sampling differs from the Ready attempt")
    for authored, ready in [("native_object", "native"), ("sampled_object", "sampled"),
                            ("provenance", "provenance")]:
        require(artifact[authored] == generated["ready"][ready],
                f"accepted {authored} differs from the Ready attempt")
    added = {artifact["native_asset"], artifact["sampled_asset"]}
    require(len(added) == 2 and added.isdisjoint(before["assets"])
            and set(accepted["assets"]) - set(before["assets"]) == added,
            "accept must add exactly its two generated assets")
    for asset_key, object_key, frames in [
        ("native_asset", "native_object", 9 + case["generated_frames"]),
        ("sampled_asset", "sampled_object", case["frames"]),
    ]:
        asset = accepted["assets"][artifact[asset_key]]
        obj = artifact[object_key]
        require(asset["content_hash"] == "blake3:" + obj["content"]["digest"]
                and asset["frame_count"] == frames and asset["audio"] is None,
                f"unexpected admitted {asset_key}")
    normalized = copy.deepcopy(accepted)
    normalized["nodes"]["extension"]["kind"]["recipe"]["video"] = fallback
    for asset in added:
        del normalized["assets"][asset]
    require(without_revision(normalized) == without_revision(before),
            "accept changed structure, audio, assets, or other authored state outside its provider")
    require(accepted["revision_id"] != before["revision_id"], "accept made no revision")

    obj = artifact["provenance"]
    require(obj["content"]["algorithm"] == "blake3"
            and re.fullmatch(r"[0-9a-f]{64}", obj["content"]["digest"]),
            "invalid retained provenance object identity")
    provenance_path = package / "Media" / "Generated" / ("blake3-" + obj["content"]["digest"])
    require(provenance_path.stat().st_size == obj["byte_length"], "provenance length differs")
    # Full project validate independently verifies the stored BLAKE3 identities.
    # This SHA-256 records the exact additional evidence inspected by this script.
    envelope = load(provenance_path)
    binding = envelope["binding"]
    require(envelope["validation_profile"] == "deadpan-ffv1-extension-1"
            and envelope["selected_provider"]["operation"] == "extension",
            "unexpected retained qualification profile")
    require(binding["identity"] == {
        "request_id": generated["request_id"], "attempt_id": generated["attempt_id"]},
        "retained request identity differs")
    require(binding["project_id"] == before["project_id"]
            and binding["revision_id"] == before["revision_id"]
            and binding["plan"] == generated["plan"]["plan"], "retained binding differs")
    require(binding["target"] == {"hold_id": "extension",
                                  "request_version": generated["request_version"]},
            "retained target differs")
    require(binding["provider"] == EXPECTED_PROVIDER
            and envelope["selected_provider"]["selection"] == EXPECTED_PROVIDER,
            "retained host-selected production provider differs")
    constraints = binding["constraints"]
    require(constraints["motion"] == case["motion"]
            and constraints["conditioning"] == "extend_" + case["direction"]
            and constraints["video"] == {"frames": case["frames"], "frame_rate": RATE,
                                         "width": 768, "height": 320},
            "retained controls or output contract differ")
    require(envelope["native"] == artifact["native_object"]
            and envelope["sampled"] == artifact["sampled_object"], "retained objects differ")
    return {"provider": binding["provider"], "artifact": artifact,
            "provenance_sha256": digest(provenance_path)}


def probe_file(root):
    require(root.is_dir(), f"denied root is absent: {root}")
    preferred = [root / "project.sqlite", root / "runtime.json",
                 root / ".active" / "ltx-2.3-q4-extension.json"]
    for candidate in preferred:
        if candidate.is_file() and candidate.resolve().is_relative_to(root):
            return candidate.resolve()
    visited = 0
    for directory, subdirs, files in os.walk(root, followlinks=False):
        subdirs.sort()
        for name in sorted(files):
            visited += 1
            require(visited <= 10000, f"could not find bounded denial probe in {root}")
            candidate = Path(directory, name)
            if candidate.is_file() and candidate.resolve().is_relative_to(root):
                return candidate.resolve()
    raise RuntimeError(f"no readable-file candidate under denied root: {root}")


def sandbox_checks(runner, denied):
    profile = runner.directory / "offline.sb"
    with profile.open("x") as stream:
        stream.write("(version 1)\n(allow default)\n")
        stream.write("(deny network-outbound (remote ip))\n")
        for path in denied.values():
            stream.write("(deny file-read* (subpath " + json.dumps(str(path)) + "))\n")
    checks = {}
    for label, path in denied.items():
        candidate = probe_file(path)
        command = [sys.executable, "-c", FILE_PROBE, candidate]
        positive = runner.execute(f"probe-{label}-readable", command)
        require(positive.get("readable") is True, f"positive file probe failed: {candidate}")
        negative = runner.execute(f"probe-{label}-denied", command, profile)
        require(negative.get("readable") is False
                and negative.get("errno") in [errno.EPERM, errno.EACCES],
                f"sandbox did not prove read denial: {candidate}")
        checks[label] = {"root": str(path), "file": str(candidate),
                         "positive": positive, "negative": negative}
    # A real listening endpoint avoids treating ECONNREFUSED as network denial.
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(2)
        listener.settimeout(5)
        command = [sys.executable, "-c", NETWORK_PROBE, listener.getsockname()[1]]
        positive = runner.execute("probe-ip-connectable", command)
        require(positive.get("connected") is True, "positive IP probe failed")
        connection, _ = listener.accept()
        connection.close()
        negative = runner.execute("probe-ip-denied", command, profile)
        require(negative.get("connected") is False
                and negative.get("errno") in [errno.EPERM, errno.EACCES],
                "sandbox did not prove outbound IP denial")
        checks["outbound-ip"] = {"positive": positive, "negative": negative,
                                 "probe_family": "IPv4", "policy": "all remote ip"}
    save_new(runner.directory / "denial-checks.json", checks)
    return profile, checks


def sample_points(case):
    total = 9 + case["frames"]
    start = 9 if case["direction"] == "from_left" else 0
    end = start + case["frames"]
    frames = sorted({frame for frame in [0, 1, total - 2, total - 1,
                    start - 1, start, start + 1, start + case["frames"] // 2,
                    end - 2, end - 1, end, end + 1] if 0 <= frame < total})
    # Absolute frame boundaries, never accumulated rounded durations.
    sample = lambda frame: frame * 48000 * RATE["denominator"] // RATE["numerator"]
    count = sample(total)
    centers = [0, count, sample(start), sample(end),
               sample(start + case["frames"] // 2)]
    windows = sorted({(max(0, center - 1024), min(count, center + 1024))
                      for center in centers})
    return total, start, end, frames, windows


def verify_report(report, before, revision, total, frames, start, end):
    require(report["passed"] and not report["failures"], "export comparison failed")
    require(report["project_id"] == before["project_id"]
            and report["revision_id"] == revision and report["range"] == [0, total]
            and report["frame_rate"] == [24, 1] and report["raster"] == [768, 320],
            "export comparison has unexpected scope")
    require(report["movie"]["video_frames"] == total
            and report["movie"]["expected_video_frames"] == total,
            "movie duration differs from authored frames")
    require([item["output_frame"] for item in report["pictures"]] == frames,
            "picture comparison omitted requested samples")
    for picture in report["pictures"]:
        expected = "generated" if start <= picture["output_frame"] < end else "original"
        require(picture["provenance"]["kind"] == expected,
                "picture comparison used an unexpected provider")


def qualify_case(case, result, runner, shared_denied):
    source = (ROOT / case["case"] / "project.deadpan").resolve(strict=True)
    before_path = ROOT / case["case"] / "before.json"
    generated_path = ROOT / case["case"] / "generated.json"
    before, generated = load(before_path), load(generated_path)
    runner.phase = "check-source-and-ready-evidence"
    baseline_facts(case, before)
    generation_facts(case, result, generated)
    evidence = {"case": case, "source": str(source), "started": utc_now(),
                "before_sha256": digest(before_path), "generated_sha256": digest(generated_path)}
    denied = {"source-project": source, **shared_denied}
    require(all(not runner.directory.is_relative_to(path) and
                not runner.cli.is_relative_to(path) and
                not Path(sys.executable).resolve().is_relative_to(path)
                for path in denied.values()), "sandbox would deny its output or executable")
    profile, probes = sandbox_checks(runner, denied)
    evidence["denials"] = probes
    require(runner.command("baseline", "project", "dump", source, "--json") == before,
            "source authored state differs from before.json; refusing acceptance")
    runner.command("accept", "accept-hold", source, "--request", generated["request_id"],
                   "--attempt", generated["attempt_id"])
    accepted = runner.command("accepted-cold", "project", "dump", source, "--json")
    runner.command("accepted-validate", "project", "validate", source)
    runner.phase = "check-accepted-provider"
    evidence["accepted"] = accepted_facts(case, before, accepted, generated, source)
    runner.command("undo", "project", "undo", source, "--expected", accepted["revision_id"])
    undone = runner.command("undone-cold", "project", "dump", source, "--json")
    require(without_revision(undone) == without_revision(before),
            "undo did not restore the complete original authored baseline")
    runner.command("redo", "project", "redo", source, "--expected", undone["revision_id"])
    redone = runner.command("redone-cold", "project", "dump", source, "--json")
    require(without_revision(redone) == without_revision(accepted),
            "redo did not restore the exact accepted authored state")
    runner.command("redone-validate", "project", "validate", source)
    portable = runner.directory / "portable.deadpan"
    evidence["portable_copy"] = runner.command("copy-portable", "project", "copy-portable",
                                               source, portable)
    copied = runner.command("portable-cold", "project", "dump", portable, "--json", profile=profile)
    require(copied == redone, "portable copy changed the accepted document")
    runner.command("portable-validate", "project", "validate", portable, profile=profile)
    exports = runner.directory / "Exports"
    exports.mkdir()
    event = runner.command("render", "render", portable, "--output", exports,
                           "--name", "accepted.mp4", "--expected", redone["revision_id"],
                           profile=profile)
    status = event["status"]
    require(event["event"] == "finished" and status["stage"] == "finished"
            and status["attempt_state"] == "verified" and status["outcome"] == "published"
            and status["captured_revision"] == redone["revision_id"]
            and status["cleanup_confirmed"] and status["observed_movie_commit"],
            "ordinary render did not complete verified publication and cleanup")
    receipt = status["receipt"]
    require(receipt["contains_generated_pictures"] is True, "render lost generated pictures")
    for key in ["movie", "report"]:
        path = Path(receipt[key]).resolve(strict=True)
        require(path.is_relative_to(exports) and path.stat().st_size == receipt[key + "_bytes"]
                and digest(path) == receipt[key + "_sha256"], f"published {key} receipt differs")
    total, start, end, frames, windows = sample_points(case)
    verify_args = ["verify-export", portable, "--movie", receipt["movie"],
                   "--revision", redone["revision_id"], "--frames", ",".join(map(str, frames))]
    pictures = runner.command("verify-pictures", *verify_args, "--no-audio", profile=profile)
    verify_report(pictures, before, redone["revision_id"], total, frames, start, end)
    audio_evidence = {"status": "unsupported", "reason": "emitted movie has no audio track"}
    if pictures["movie"]["audio_track"] is not None:
        audio = runner.command("verify-silent-audio", *verify_args, "--samples",
                               ",".join(f"{a}:{b}" for a, b in windows), profile=profile)
        verify_report(audio, before, redone["revision_id"], total, frames, start, end)
        require([item["window"] for item in audio["audio"]] == [list(pair) for pair in windows],
                "audio comparison omitted requested windows")
        require(all(item["passed"] and item["kind"] == "silent"
                    and item["offset_status"] == "not_applicable"
                    and item["measured_offset_samples"] is None
                    and item["uncovered_samples"] == 0 for item in audio["audio"]),
                "silent export windows failed or claimed a measured silent offset")
        audio_evidence = {"status": "passed", "windows": windows,
                          "timing_observation": "not applicable to silence"}
    require(runner.command("portable-after", "project", "dump", portable, "--json",
                           profile=profile) == redone, "portable render changed authored state")
    require(runner.command("source-after", "project", "dump", source, "--json") == redone,
            "source changed after redo or portable export")
    evidence.update({"passed": True, "finished": utc_now(), "receipt": receipt,
                     "before_revision": before["revision_id"],
                     "accepted_revision": accepted["revision_id"],
                     "undo_revision": undone["revision_id"],
                     "redo_revision": redone["revision_id"], "picture_frames": frames,
                     "silent_audio": audio_evidence, "portable": str(portable)})
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "accepted-verification")
    parser.add_argument("--timeout-seconds", type=int, default=900)
    args = parser.parse_args()
    cli = args.cli.resolve(strict=True)
    output = args.output.resolve()
    require(cli.is_file() and os.access(cli, os.X_OK), "CLI is not executable")
    require(args.timeout_seconds > 0, "timeout must be positive")
    require(cli.parent.name == "MacOS" and cli.parent.parent.name == "Contents",
            "--cli must name a bundled Contents/MacOS/deadpan-cli")
    require(shutil.which("sandbox-exec") is not None, "sandbox-exec unavailable")
    output.mkdir(parents=True, exist_ok=False)
    summary = {"started": utc_now(), "passed": False, "complete_batch": False,
               "cli": str(cli), "cli_sha256": digest(cli),
               "script": str(Path(__file__).resolve()), "script_sha256": digest(Path(__file__)),
               "platform": platform.platform(), "python": sys.version,
               "environment": "inherited without HOME, PATH or TMPDIR replacement",
               "expected_provider": EXPECTED_PROVIDER, "cases": []}
    try:
        cases, results = load(ROOT / "cases.json"), load(ROOT / "results.json")
        require(cases == EXPECTED_CASES, "cases.json differs from the exact eight-case matrix")
        require(isinstance(results, list) and len(results) == len(cases)
                and {item["case"] for item in results} == {item["case"] for item in cases},
                "results.json is incomplete or has duplicated/unknown cases")
        summary["complete_batch"] = True
        summary["cases_sha256"] = digest(ROOT / "cases.json")
        summary["results_sha256"] = digest(ROOT / "results.json")
        results_by_case = {item["case"]: item for item in results}
        for case in cases:
            require(all(results_by_case[case["case"]].get(key) == value
                        for key, value in case.items()),
                    f"generation result metadata differs: {case['case']}", "precondition_failed")
        generation_binary = ROOT / "executed-binary.json"
        if generation_binary.exists():
            summary["generation_binary"] = load(generation_binary)
        shared_denied = {
            "ai-runtime": (cli.parent.parent / "Resources" / "ai-runtime").resolve(strict=True),
            "default-model-root": (Path.home() / "Library" / "Application Support" /
                                   "Deadpan" / "Models").resolve(strict=True),
            "development-model-cache": (Path.home() / "Library" / "Caches" /
                                        "Deadpan" / "ltx-qualification").resolve(strict=True),
        }
        summary["shared_denied_paths"] = {key: str(value) for key, value in shared_denied.items()}
        for case in cases:
            result = results_by_case[case["case"]]
            if result.get("ready") is not True:
                summary["cases"].append({"case": case, "passed": False,
                                         "status": "generation_failed", "generation_result": result})
                continue
            directory = output / case["case"]
            directory.mkdir()
            runner = Runner(directory, cli, args.timeout_seconds)
            started = time.monotonic()
            try:
                evidence = qualify_case(case, result, runner, shared_denied)
            except Exception as error:
                evidence = {"case": case, "passed": False, "phase": runner.phase,
                            "error": f"{type(error).__name__}: {error}",
                            "failure_kind": failure_kind(error),
                            "traceback": traceback.format_exc(),
                            "source_may_remain_accepted": True,
                            "cleanup_uncertain": runner.cleanup_uncertain}
            evidence["steps"] = runner.steps
            evidence["seconds"] = round(time.monotonic() - started, 6)
            save_new(directory / "summary.json", evidence)
            summary["cases"].append(evidence)
            if runner.cleanup_uncertain:
                raise RuntimeError("Stopped after timeout; inspect owned CLI/worker cleanup before resuming")
        summary["passed"] = len(summary["cases"]) == 8 and all(
            case["passed"] for case in summary["cases"])
        summary["qualified_count"] = sum(case["passed"] for case in summary["cases"])
    except BaseException as error:
        summary["error"] = f"{type(error).__name__}: {error}"
        summary["failure_kind"] = failure_kind(error)
        summary["traceback"] = traceback.format_exc()
    finally:
        summary["finished"] = utc_now()
        summary["cli_sha256_after"] = digest(cli)
        if summary["cli_sha256_after"] != summary["cli_sha256"]:
            summary["passed"] = False
            summary["binary_changed"] = True
        save_new(output / "summary.json", summary)
        print(json.dumps({"summary": str(output / "summary.json"),
                          "passed": summary["passed"],
                          "qualified_count": summary.get("qualified_count", 0)}), flush=True)
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
