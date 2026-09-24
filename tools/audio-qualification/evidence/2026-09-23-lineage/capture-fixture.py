import hashlib
import json
import pathlib
import sqlite3
import subprocess

scratch = pathlib.Path('/tmp/deadpan-lineage-20260923')
binary = scratch / 'deadpan-cli-core14'
package = scratch / 'legacy-fixture.deadpan'

def cli(*args):
    result = subprocess.run([str(binary), *map(str, args)], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)

def document():
    with sqlite3.connect(package / 'project.sqlite') as db:
        return json.loads(db.execute('SELECT document FROM revisions WHERE id=(SELECT head_revision FROM state)').fetchone()[0])

def command(revision, value):
    doc = document()
    request = {'protocol': 1, 'project_id': doc['project_id'], 'expected_revision': doc['revision_id'], 'new_revision': revision, 'command': value}
    request_file = scratch / 'fixture-request.json'
    request_file.write_text(json.dumps(request))
    cli('command', package, '--json', request_file)

def navigate(direction):
    cli('project', direction, package, '--expected', document()['revision_id'])

hold = {'label': 'Silence', 'kind': {'type': 'hold', 'recipe': {'duration': 12, 'video': {'type': 'background'}, 'audio': {'type': 'silence'}}}}
command('insert', {'command': 'insert', 'parent': document()['root'], 'index': 0, 'subtree': {'root': 'hold', 'nodes': {'hold': hold}}})
command('split', {'command': 'split', 'node': 'hold', 'at': 5, 'identities': {'nodes': ['left', 'right', 'right-context']}})
navigate('undo')
navigate('redo')
command('wrap', {'command': 'wrap_repeat', 'node': 'right', 'id': 'repeat', 'plays': 3, 'gap': None})
iteration = {'allocation': 'wrap', 'ordinal': 1}
instance = {'node': 'right-context', 'repeats': [{'node': 'repeat', 'iteration': iteration}]}
command('occurrence-rename', {'command': 'edit_occurrence', 'instance': instance, 'edit': {'type': 'rename', 'label': 'Independent play'}, 'identities': {'nodes': ['isolated-right', 'isolated-context'], 'marks': []}})
instance = {'node': 'isolated-right', 'repeats': [{'node': 'repeat', 'iteration': iteration}]}
command('occurrence-split', {'command': 'edit_occurrence', 'instance': instance, 'edit': {'type': 'split', 'at': 3, 'identities': {'nodes': ['isolated-tail', 'isolated-tail-context', 'isolated-sequence']}}, 'identities': {'nodes': [], 'marks': []}})
command('delete', {'command': 'delete', 'node': 'left'})
navigate('undo')
cli('project', 'validate', package)
with sqlite3.connect(package / 'project.sqlite') as live:
    with sqlite3.connect(scratch / 'fixture-backup.sqlite') as backup:
        live.backup(backup)
        assert backup.execute('PRAGMA user_version').fetchone()[0] == 20
        lines = list(backup.iterdump())
header = '''-- Authentic database schema 20 / core schema 14 history.
-- Captured with the preserved CLI from revision 1eb043c on 2026-09-23.
-- Direct Split, undo/redo, Repeat occurrence isolation, occurrence Split,
-- deletion and pending redo. Validated by the old executable and captured
-- through SQLite backup. No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=20;
'''
output = pathlib.Path('crates/deadpan-store/tests/fixtures/v20-history.sql')
output.write_text(header + '\n'.join(lines) + '\n')
(scratch / 'fixture-provenance.json').write_text(json.dumps({'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'fixture_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}, indent=2) + '\n')
print(output)
