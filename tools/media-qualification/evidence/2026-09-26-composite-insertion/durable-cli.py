"""Exercise current composite InsertTime through public CLI and durable history.

Uses Background/Silence so this checks authoring and storage, not media output.
Decoded PCM and native indexed-picture tests supply independent media evidence.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('--app-headless', action='store_true')
args = parser.parse_args()
binary = args.binary.resolve()
args.output.mkdir(exist_ok=False)
package = args.output / 'composite.deadpan'
log = []


def run(*arguments, success=True):
    invocation = [str(binary), *(['--headless'] if args.app_headless else []), *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True, timeout=30)
    log.append(dict(command=invocation, exit_code=result.returncode,
                    stdout=result.stdout, stderr=result.stderr))
    (args.output / 'commands.json').write_text(json.dumps(log, indent=2) + '\n')
    assert (result.returncode == 0) == success, result.stderr
    return json.loads(result.stdout) if success else result


def snapshot():
    return run('project', 'dump', package, '--json')


def command(name, payload, success=True):
    before = snapshot()
    request = dict(protocol=1, project_id=before['project_id'],
                   expected_revision=before['revision_id'], new_revision=name,
                   command=payload)
    path = args.output / (name + '.json')
    path.write_text(json.dumps(request, indent=2) + '\n')
    result = run('command', package, '--json', path, success=success)
    assert snapshot()['revision_id'] == (name if success else before['revision_id'])
    return result


def navigate(verb):
    before = snapshot()
    run('project', verb, package, '--expected', before['revision_id'])
    after = snapshot()
    assert after['revision_id'] != before['revision_id']
    return after


def intent(document):
    return {key: value for key, value in document.items() if key != 'revision_id'}


def pause(name, at):
    return dict(command='insert_time', at=at, hold=dict(recipe, duration=1),
                id=name, identities=dict(nodes=[]),
                timing=dict(allocation=name, ordinal=0))


doctor = run('doctor')
assert (doctor['document_schema'], doctor['database_schema']) == (24, 30)
run('project', 'create', package, '--fps', '30000/1001', '--size', '16x16')
root = snapshot()['root']
recipe = dict(duration=2, video={'type': 'background'}, audio={'type': 'silence'})
command('insert', dict(command='insert', parent=root, index=0, subtree=dict(
    root='child', nodes={'child': dict(label='Child', kind=dict(type='hold', recipe=recipe))},
    overrides={}, gap_overrides={})))
command('repeat', dict(command='wrap_repeat', node='child', id='repeat', plays=3,
                       gap=dict(recipe, duration=1)))
command('isolate', dict(command='isolate_gap', node='repeat',
                        iteration=dict(allocation='repeat', ordinal=0),
                        id='independent', timing=dict(allocation='isolate', ordinal=0)))
command('split', dict(command='split', node='repeat', at=1,
                      identities=dict(nodes=[f'fragment-{i}' for i in range(16)])))
before = snapshot()
assert run('project', 'validate', package)['duration_frames'] == 8
command('pause-one', pause('pause-one', 1))
first = snapshot()
assert len(first['nodes']) == len(before['nodes']) + 1
command('pause-two', pause('pause-two', 2))
second = snapshot()
assert run('project', 'validate', package)['duration_frames'] == 10
assert len(second['nodes']) == len(before['nodes']) + 2
for key, value in before['nodes'].items():
    if key != root:
        assert second['nodes'][key] == value
assert second['gap_overrides'] == before['gap_overrides']
assert any(len(binding.get('reanchors', [])) == 2
           for binding in second['audio_bindings']['bindings'].values())
assert intent(navigate('undo')) == intent(first)
assert intent(navigate('undo')) == intent(before)
assert intent(navigate('redo')) == intent(first)
assert intent(navigate('redo')) == intent(second)
saved = snapshot()
failure = command('interior-refused', pause('interior-refused', 4), success=False)
assert 'existing root Sequence seam' in failure.stderr
assert snapshot() == saved
run('project', 'validate', package)
with sqlite3.connect(f'file:{package / "project.sqlite"}?mode=ro', uri=True) as database:
    assert database.execute('pragma user_version').fetchone() == (30,)
    revisions = database.execute('select id,document from revisions').fetchall()
    assert len({row[0] for row in revisions}) == len(revisions)
    assert all(json.loads(row[1])['schema_version'] == 24 for row in revisions)
    assert 'interior-refused' not in {row[0] for row in revisions}
    history = database.execute('select count(*) from history').fetchone()[0]
report = dict(passed=True, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
              entrypoint='deadpan-app --headless' if args.app_headless else 'deadpan-cli',
              commands=len(log), revisions=len(revisions), history_entries=history,
              final_revision=saved['revision_id'],
              scope='Public-command split Repeat with isolated gap, two root-seam pauses, durable undo/redo, atomic interior refusal')
(args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
