import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent
rows = []
for line in (root / "sdr04-artifacts.log").read_text().splitlines():
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if (row.get("reason") == "compiler-artifact"
            and row.get("target", {}).get("name") == "qualify_sdr_export"
            and row.get("target", {}).get("kind") == ["example"]
            and row.get("executable")):
        rows.append(row)
if len(rows) != 1:
    raise RuntimeError(f"expected exactly one recorded SDR example, got {len(rows)}")
artifact = rows[0]
binary = Path(artifact["executable"])
before = hashlib.sha256(binary.read_bytes()).hexdigest()
command = [str(binary), str(root / "metal.json"), str(root / "metal-fixtures")]
receipt = {"cargo_artifact": artifact, "sha256": before, "command": command,
           "cwd": str(Path.cwd()), "timeout_seconds": 90}
path = root / "metal-artifact.json"
with path.open("x") as output:
    json.dump(receipt, output, indent=2)
    output.write("\n")
try:
    child = subprocess.run(command, timeout=90)
    receipt["exit_code"] = child.returncode
except subprocess.TimeoutExpired:
    receipt["timed_out"] = True
    receipt["exit_code"] = 124
receipt["sha256_after"] = hashlib.sha256(binary.read_bytes()).hexdigest()
receipt["binary_unchanged"] = before == receipt["sha256_after"]
path.write_text(json.dumps(receipt, indent=2) + "\n")
if not receipt["binary_unchanged"]:
    raise RuntimeError("qualification artifact changed during execution")
sys.exit(receipt["exit_code"])
