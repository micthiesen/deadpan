"""Check the alleged migration chronology with the actual preserved old CLI."""
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-gap-bindings-20260926/legacy-gap-refusal')
binary = Path('/tmp/deadpan-gap-bindings-20260926/old-binary/deadpan-cli')
assert hashlib.sha256(binary.read_bytes()).hexdigest() == '2c3a9501d1d412797ea076f6c8f56c5ad529b1cbd4bba0fdb200e6a364be4600'
scratch.mkdir(exist_ok=False)
project = scratch / 'legacy.deadpan'
project.mkdir()
(project / 'Snapshots').mkdir()
with sqlite3.connect(project / 'project.sqlite') as connection:
    connection.executescript((repo / 'crates/deadpan-store/tests/fixtures/v27-audio-reanchor-history.sql').read_text())
records = []

def run(*arguments, expected=0):
    result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True, text=True)
    records.append(dict(command=[str(binary), *map(str, arguments)], returncode=result.returncode,
                        stdout=result.stdout, stderr=result.stderr))
    (scratch / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
    assert result.returncode == expected, records[-1]
    return json.loads(result.stdout) if result.stdout else json.loads(result.stderr)

def command(name, payload, expected=0):
    before = run('project', 'dump', project, '--json')
    request = dict(protocol=1, project_id=before['project_id'], expected_revision=before['revision_id'],
                   new_revision=name, command=payload)
    path = scratch / f'{name}.json'
    path.write_text(json.dumps(request, indent=2) + '\n')
    result = run('command', project, '--json', path, expected=expected)
    if expected:
        assert run('project', 'dump', project, '--json') == before
        assert 'does not support Repeat gap binding ownership' in json.dumps(result)

doctor = run('doctor')
assert (doctor['document_schema'], doctor['database_schema']) == (21, 27)
for plays in (3, 1):
    if plays == 1:
        command('old-one-play', dict(command='set_repeat', node='old-gap-owner', plays=1,
                gap=dict(duration=3, video=dict(type='background'), audio=dict(type='silence'))))
    state = run('project', 'validate', project)
    name = f'old-pause-after-{plays}-plays'
    command(name, dict(command='insert_time', at=state['duration_frames'],
            hold=dict(duration=1, video=dict(type='background'), audio=dict(type='silence')),
            id=f'pause-{plays}', identities=dict(nodes=[]), timing=dict(allocation=name, ordinal=0)), expected=1)
print(json.dumps({'core': 21, 'database': 27, 'cases': 2, 'configured_plays': [3, 1],
                  'both_refused_without_revision': True, 'commands': str(scratch / 'commands.json')}))
