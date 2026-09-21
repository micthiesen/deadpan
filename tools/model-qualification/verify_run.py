"""Independent media checks of the Rust host's frozen candidate copy.

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
        provenance = json.loads((run / "worker/outputs/provenance.json").read_text())
        if (not host["clean_exit"] or host["faults"] or host["state"] != "Validating"
                or not host["hash_verified_snapshot"] or host["accepted"]):
            raise ValueError("host did not produce an eligible frozen candidate")
        video = config["request"]["constraints"]["video"]
        candidate = host["candidate"]
        if candidate["video"] != video or candidate["provider"] != config["request"]["provider"]:
            raise ValueError("candidate contract differs from host request")
        snapshot = run / "host/candidate.snapshot.mp4"
        if (snapshot.stat().st_size != candidate["media"]["byte_length"]
                or file_digest(snapshot) != candidate["media"]["sha256"]):
            raise ValueError("frozen candidate changed after host snapshot")
        if provenance["candidate_rgb"]["frames"] != video["frames"]:
            raise ValueError("provenance count differs from authored count")
        result["probe"] = verify_rgb(
            runtime["ffmpeg"], runtime["ffprobe"], snapshot, provenance["candidate_rgb"],
            video["width"], video["height"], video["frame_rate"], lambda: None)
        result.update(passed=True, sha256=candidate["media"]["sha256"],
                      decoded_rgb_sha256=provenance["candidate_rgb"]["rgb_sha256"],
                      frames=video["frames"], video=video,
                      host_report_sha256=file_digest(run / "host/host-report.json"),
                      worker_provenance_sha256=file_digest(run / "worker/outputs/provenance.json"),
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
