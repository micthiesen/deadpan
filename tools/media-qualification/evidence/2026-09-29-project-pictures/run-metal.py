"""Execute only the Cargo-recorded project-picture example, with attribution."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent
rows = []
for line in (root / "artifacts.log").read_text().splitlines():
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if (row.get("reason") == "compiler-artifact"
            and row.get("target", {}).get("name") == "qualify_project_picture"
            and row.get("target", {}).get("kind") == ["example"]
            and row.get("executable")):
        rows.append(row)
if len(rows) != 1:
    raise RuntimeError(f"expected one exact project example, got {len(rows)}")
artifact = rows[0]
binary = Path(artifact["executable"])
before = hashlib.sha256(binary.read_bytes()).hexdigest()
command = [str(binary), str(root / "project-metal.json"), str(root / "project-metal-fixtures")]
receipt = {"cargo_artifact": artifact, "sha256": before, "command": command,
           "cwd": str(Path.cwd()), "timeout_seconds": 120}
path = root / "project-metal-artifact.json"
with path.open("x") as output:
    json.dump(receipt, output, indent=2)
    output.write("\n")
try:
    child = subprocess.run(command, timeout=120)
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
