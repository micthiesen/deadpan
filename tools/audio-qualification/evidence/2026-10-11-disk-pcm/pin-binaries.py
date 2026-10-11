from pathlib import Path
import hashlib
import json
import re
import sys

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
name = sys.argv[1]
paths = re.findall(r'Running .*?\(([^\n]+)\)', (root / (name + '.log')).read_text())
if name.startswith('export-'):
    paths.append('target/release/deadpan-cli')
pins = {}
for name_in_log in paths:
    path = Path(name_in_log)
    if not path.is_absolute():
        path = repo / path
    if path.is_file():
        with path.open('rb') as stream:
            pins[str(path)] = hashlib.file_digest(stream, 'sha256').hexdigest()
(root / (name + '-binaries.json')).write_text(json.dumps(pins, indent=2) + '\n')
print({'run': name, 'binaries': len(pins)})
