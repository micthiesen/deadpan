import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path(os.environ.get("DEADPAN_CHECK_OUTPUT", str(Path(__file__).parent)))
name, *command = sys.argv[1:]
environment = dict(os.environ, DEADPAN_FFMPEG_PREFIX="/tmp/deadpan-ui-ffmpeg/prefix")
report = {"command": command, "started": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "base_commit": subprocess.check_output(["git", "-c", "core.fsmonitor=false", "rev-parse", "HEAD"], text=True).strip(),
          "diff_sha256": hashlib.sha256(subprocess.check_output(["git", "-c", "core.fsmonitor=false", "diff"])).hexdigest()}
paths = subprocess.check_output(["git", "-c", "core.fsmonitor=false", "ls-files", "--cached", "--others", "--exclude-standard", "-z"]).decode().split("\0")
sources = {name: hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in sorted(set(paths))
           if name and Path(name).is_file() and (Path(name).suffix in (".rs", ".toml", ".lock", ".cpp", ".h", ".hpp", ".c", ".m", ".py") or name.startswith((".cargo/", "crates/", "native/")))}
source_wire = json.dumps(sources, sort_keys=True, indent=2) + "\n"
report["source_manifest_sha256"] = hashlib.sha256(source_wire.encode()).hexdigest()
(root / ("source-" + report["source_manifest_sha256"] + ".json")).write_text(source_wire)
start = time.monotonic()
with (root / (name + ".log")).open("w") as output:
    child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, env=environment)
    report["pid"] = child.pid
    (root / (name + ".json")).write_text(json.dumps(report, indent=2) + "\n")
    report["exit_code"] = child.wait()
report["seconds"] = time.monotonic() - start
(root / (name + ".json")).write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
print("".join((root / (name + ".log")).read_text().splitlines(keepends=True)[-35:]))
sys.exit(report["exit_code"])
