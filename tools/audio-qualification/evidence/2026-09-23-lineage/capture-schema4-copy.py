import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess

scratch = Path('/tmp/deadpan-lineage-20260923')
binary = Path('/tmp/deadpan-schema4-cli')
package = scratch / 'legacy-copy-schema4.deadpan'

def cli(*args):
    result = subprocess.run([str(binary), *map(str, args)], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)

def document():
    return cli('project', 'dump', package, '--json')

def command(revision, value):
    doc = document()
    request = {'protocol': 1, 'project_id': doc['project_id'], 'expected_revision': doc['revision_id'], 'new_revision': revision, 'command': value}
    request_file = scratch / 'schema4-copy-request.json'
    request_file.write_text(json.dumps(request))
    cli('command', package, '--json', request_file)

def navigate(direction):
    cli('project', direction, package, '--expected', document()['revision_id'])

cli('project', 'create', package, '--fps', '30000/1001', '--size', '16x16')
hold = {'label': 'Silence', 'kind': {'type': 'hold', 'recipe': {'duration': 12, 'video': {'type': 'background'}, 'audio': {'type': 'silence'}}}}
command('insert', {'command': 'insert', 'parent': document()['root'], 'index': 0, 'subtree': {'root': 'hold', 'nodes': {'hold': hold}}})
command('wrap', {'command': 'wrap_repeat', 'node': 'hold', 'id': 'repeat', 'plays': 3, 'gap': None})
iteration = {'allocation': 'wrap', 'ordinal': 1}
instance = {'node': 'hold', 'repeats': [{'node': 'repeat', 'iteration': iteration}]}
command('occurrence-copy', {'command': 'edit_occurrence', 'instance': instance, 'edit': {'type': 'rename', 'label': 'Independent play'}, 'identities': {'nodes': ['isolated'], 'marks': []}})
navigate('undo')
navigate('redo')
command('clear', {'command': 'clear_play_override', 'node': 'repeat', 'iteration': iteration})
navigate('undo')
cli('project', 'validate', package)
with sqlite3.connect(package / 'project.sqlite') as live:
    with sqlite3.connect(scratch / 'schema4-copy-backup.sqlite') as backup:
        live.backup(backup)
        assert backup.execute('PRAGMA user_version').fetchone()[0] == 4
        lines = list(backup.iterdump())
digest = hashlib.sha256(binary.read_bytes()).hexdigest()
header = f'''-- Authentic database schema 4 / core schema 4 occurrence-copy history.
-- Captured on 2026-09-23 with the preserved CLI from revision
-- 8b79526ca3ff9bca633aa9e9d795bcc9a0c0c2d1.
-- CLI SHA-256: {digest}.
-- Repeat occurrence isolation, undo/redo, clear override and pending redo.
-- Validated by the old executable and captured through SQLite backup.
-- No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=4;
'''
output = Path('crates/deadpan-store/tests/fixtures/v4-audio-lineage.sql')
output.write_text(header + '\n'.join(lines) + '\n')
print(output)
