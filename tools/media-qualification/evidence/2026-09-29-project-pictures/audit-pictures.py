"""Verify retained files and archive members without extracting them."""
import hashlib
import json
from pathlib import Path
import sys
import tarfile

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
manifest = json.loads((root/'manifest.json').read_text())
actual = {str(path.relative_to(root)) for path in root.rglob('*') if path.is_file() and path.name != 'manifest.json'}
assert actual == set(manifest)
for name, expected in manifest.items():
    path = root/name
    assert path.stat().st_size == expected['bytes'], name
    with path.open('rb') as source:
        assert hashlib.file_digest(source, 'sha256').hexdigest() == expected['sha256'], name
archives = json.loads((root/'archive-contents.json').read_text())
members = 0
for name, expected in archives.items():
    with tarfile.open(root/name, 'r:gz') as archive:
        inventory = archive.getmembers()
        assert len(inventory) == len(expected)
        for item, row in zip(inventory, expected):
            assert item.isfile() and item.name == row['name'] and item.size == row['bytes']
            assert hashlib.file_digest(archive.extractfile(item), 'sha256').hexdigest() == row['sha256']
            members += 1
summary = json.loads((root/'summary.json').read_text())
print(json.dumps({'files': len(manifest), 'archives': len(archives), 'members': members,
                  'commands': len(summary['commands']),
                  'failed_commands': sum(command['exit_code'] != 0 for command in summary['commands'])}))
