"""Run only current app feature tests after the separate base workspace gate."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
artifact_name = sys.argv[1] if len(sys.argv) > 1 else "ui-artifacts"
base_mode = len(sys.argv) > 2 and sys.argv[2] == "base"
artifacts = {}
for line in (root / (artifact_name + ".log")).read_text().splitlines():
    try:
        record = json.loads(line)
    except json.JSONDecodeError:
        continue
    if record.get("reason") != "compiler-artifact" or not record.get("executable"):
        continue
    if Path(record["manifest_path"]).parent.name != "deadpan-app":
        continue
    assert ("ui-harness" in record["features"]) != base_mode, record
    executable = Path(record["executable"])
    key = (record["target"]["name"], record["profile"]["test"])
    artifacts[key] = {
        "target": record["target"]["name"],
        "test": record["profile"]["test"],
        "executable": str(executable),
        "sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "cwd": str(Path(record["manifest_path"]).parent),
        "features": record["features"],
    }
inventory = list(artifacts.values())
tests = [record for record in inventory if record["test"]]
assert len(tests) == 2, inventory
assert any(not record["test"] and record["target"] == "deadpan-app" for record in inventory)
(root / (artifact_name + "-app-inventory.json")).write_text(json.dumps(inventory, indent=2) + "\n")
failed = False
for record in tests:
    print(json.dumps(record), flush=True)
    path = Path(record["executable"])
    assert hashlib.sha256(path.read_bytes()).hexdigest() == record["sha256"]
    result = subprocess.run([str(path)], cwd=record["cwd"], env=os.environ)
    record["exit_code"] = result.returncode
    failed |= result.returncode != 0
(root / (artifact_name + "-app-inventory.json")).write_text(json.dumps(inventory, indent=2) + "\n")
raise SystemExit(1 if failed else 0)
