"""Independent media checks of a Rust host's frozen V1 candidate or V2 native copy.

This is qualification evidence, not app acceptance, a safety/quality classifier,
or durable project artifact promotion.
"""

import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from worker_media import file_digest, verify_rgb


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_directory", type=Path)
    args = parser.parse_args()
    run = args.run_directory
    started = time.monotonic()
    result = {"accepted": False, "scope": "generated-file timing/color/RGB/hash checks only"}
    failure = None
    try:
        host = json.loads((run / "host/host-report.json").read_text())
        config = json.loads((run / "host-config.json").read_text())
        runtime = json.loads((run / "runtime.json").read_text())
        if (not host["clean_exit"] or host["faults"] or host["state"] != "Validating"
                or host["accepted"]):
            raise ValueError("host did not produce an eligible frozen candidate")
        modern = config["request"]["protocol"] == 2
        if modern:
            if not host.get("hash_verified_bundle"):
                raise ValueError("host did not snapshot the native bundle")
            candidate = host["bundle"]
            native = config["request"]["plan"]["native"]
            video = {"frames": native["frame_count"], "frame_rate": native["frame_rate"],
                     "width": native["width"], "height": native["height"]}
            media = candidate["native"]
            snapshot = run / "host/native.snapshot.mp4"
            provenance_path = run / "host/provenance.snapshot.json"
            if (provenance_path.stat().st_size != candidate["provenance"]["byte_length"]
                    or file_digest(provenance_path) != candidate["provenance"]["sha256"]):
                raise ValueError("frozen provenance changed after host snapshot")
            rgb_key = "native_rgb"
        else:
            if not host.get("hash_verified_snapshot"):
                raise ValueError("host did not snapshot the sampled candidate")
            video = config["request"]["constraints"]["video"]
            candidate = host["candidate"]
            media = candidate["media"]
            snapshot = run / "host/candidate.snapshot.mp4"
            provenance_path = run / "worker/outputs/provenance.json"
            rgb_key = "candidate_rgb"
        provenance = json.loads(provenance_path.read_text())
        if candidate["video"] != video or candidate["provider"] != config["request"]["provider"]:
            raise ValueError("candidate contract differs from host request")
        if (snapshot.stat().st_size != media["byte_length"]
                or file_digest(snapshot) != media["sha256"]):
            raise ValueError("frozen candidate changed after host snapshot")
        if provenance[rgb_key]["frames"] != video["frames"]:
            raise ValueError("provenance count differs from requested video count")
        result["probe"] = verify_rgb(
            runtime["ffmpeg"], runtime["ffprobe"], snapshot, provenance[rgb_key],
            video["width"], video["height"], video["frame_rate"], lambda: None)
        result.update(passed=True, protocol=config["request"]["protocol"], sha256=media["sha256"],
                      decoded_rgb_sha256=provenance[rgb_key]["rgb_sha256"],
                      frames=video["frames"], video=video,
                      host_report_sha256=file_digest(run / "host/host-report.json"),
                      worker_provenance_sha256=file_digest(provenance_path),
                      adapter_sources_sha256=provenance.get("adapter_sources_sha256"))
    except Exception as error:
        failure = error
        result.update(passed=False, error=str(error))
    result["verification_seconds"] = time.monotonic() - started
    with (run / "host/media-verification.json").open("x") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps(result, indent=2))
    if failure:
        raise failure


if __name__ == "__main__":
    main()
