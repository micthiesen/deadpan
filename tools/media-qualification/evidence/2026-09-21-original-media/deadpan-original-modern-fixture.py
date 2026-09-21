import hashlib
import json
import pathlib
import sqlite3
import subprocess

repo = pathlib.Path('/Users/michael/Code/deadpan')
work = pathlib.Path(pathlib.Path('/tmp/deadpan-original-baseline-current.txt').read_text())
package = pathlib.Path('/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/deadpan-durable-acceptance-q1rh36r4/accepted/relocated-accepted.deadpan')
command = [str(work / 'deadpan-cli-schema9'), 'project', 'validate', str(package)]
result = subprocess.run(command, check=True, text=True, capture_output=True)
source = sqlite3.connect((package / 'project.sqlite').as_uri() + '?mode=ro', uri=True)
snapshot = sqlite3.connect(':memory:')
source.backup(snapshot)
source.close()
assert snapshot.execute('PRAGMA user_version').fetchone()[0] == 9
assert snapshot.execute("SELECT count(*) FROM generation_bundle_receipts WHERE json_type(bundle, '$.admission')='object'").fetchone()[0] == 1
output = repo / 'crates/deadpan-store/tests/fixtures/v9-history.sql'
output.write_text('-- Genuine schema-9 acceptance qualification project, validated by the c224c2c binary.\n'
    '-- Retains real six-object admission, acceptance, undo/redo and fallback reversion.\n'
    '-- Existing schema-9 state exported through a consistent SQLite backup; no media bytes embedded.\n'
    'PRAGMA application_id=1146113585;\nPRAGMA user_version=9;\n' + '\n'.join(snapshot.iterdump()) + '\n')
(work / 'fixture-generation.json').write_text(json.dumps({
    'validation_revision': 'c224c2c3d2dfd3decad86d279c72684c66962b82',
    'binary_sha256': hashlib.sha256((work / 'deadpan-cli-schema9').read_bytes()).hexdigest(),
    'source_qualification': 'docs/qualification/acceptance-2026-09-21.md',
    'command': command, 'stdout': result.stdout, 'stderr': result.stderr,
    'snapshot_method': 'SQLite backup API into a consistent in-memory database; iterdump of that copy',
    'fixture_sha256': hashlib.sha256(output.read_bytes()).hexdigest()
}, indent=2) + '\n')
print(output)
