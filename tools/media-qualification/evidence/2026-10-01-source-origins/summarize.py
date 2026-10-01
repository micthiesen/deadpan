import json
from pathlib import Path
import re
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
reports = {}
result = re.compile(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;")
for path in sorted(root.glob("*.json")):
    log_path = path.with_suffix(".log")
    if not log_path.exists():
        continue
    report = json.loads(path.read_text())
    if "command" not in report:
        continue
    sections = {"unit_integration": [0, 0, 0], "documentation": [0, 0, 0]}
    group = "unit_integration"
    for line in log_path.read_text().splitlines():
        if line.lstrip().startswith("Doc-tests "):
            group = "documentation"
        elif line.lstrip().startswith("Running "):
            group = "unit_integration"
        match = result.search(line)
        if match:
            sections[group] = [a + int(b) for a, b in zip(sections[group], match.groups())]
    report["tests"] = {
        key: dict(zip(("passed", "failed", "ignored"), counts))
        for key, counts in sections.items()
    }
    reports[path.stem] = report
print(json.dumps(reports, indent=2) + "\n", end="")
