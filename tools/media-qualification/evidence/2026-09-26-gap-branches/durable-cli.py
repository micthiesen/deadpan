"""Exercise current gap commands through the CLI and durable SQLite history.

Uses only public commands and read-only SQL inspection. No seeded snapshots or
rewritten history. Background/Silence isolates this check from media admission;
the decoded-audio integration suite supplies the independent PCM evidence.
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
args = parser.parse_args()
binary = args.binary.resolve()
args.output.mkdir(exist_ok=False)
package = args.output / 'gap.deadpan'
log = []


def run(*arguments):
    invocation = [str(binary), *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True, timeout=30)
    log.append(dict(command=invocation, exit_code=result.returncode,
                    stdout=result.stdout, stderr=result.stderr))
    (args.output / 'commands.json').write_text(json.dumps(log, indent=2) + '\n')
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def snapshot():
    return run('project', 'dump', package, '--json')


def command(name, payload):
    before = snapshot()
    request = dict(protocol=1, project_id=before['project_id'],
                   expected_revision=before['revision_id'], new_revision=name,
                   command=payload)
    path = args.output / (name + '.json')
    path.write_text(json.dumps(request, indent=2) + '\n')
    result = run('command', package, '--json', path)
    assert snapshot()['revision_id'] == name
    return result


def history(verb):
    before = snapshot()
    run('project', verb, package, '--expected', before['revision_id'])
    after = snapshot()
    assert after['revision_id'] != before['revision_id']
    return after


def intent(document):
    return {key: value for key, value in document.items() if key != 'revision_id'}


doctor = run('doctor')
assert (doctor['document_schema'], doctor['database_schema']) == (23, 29)
run('project', 'create', package, '--fps', '30000/1001', '--size', '16x16')
root = snapshot()['root']
recipe = dict(duration=1, video={'type': 'background'}, audio={'type': 'silence'})
command('insert', dict(command='insert', parent=root, index=0, subtree=dict(
    root='child', nodes={'child': dict(label='Child', kind=dict(type='hold', recipe=recipe))},
    overrides={}, gap_overrides={})))
gap = dict(recipe, duration=2)
command('repeat', dict(command='wrap_repeat', node='child', id='repeat', plays=3, gap=gap))
original = snapshot()
iteration = dict(allocation='repeat', ordinal=0)
command('isolate', dict(command='isolate_gap', node='repeat', iteration=iteration,
                        id='independent', timing=dict(allocation='isolate', ordinal=0)))
isolated = snapshot()
assert isolated['nodes']['independent']['kind']['recipe'] == gap
assert isolated['gap_overrides']['repeat'] == [dict(iteration=iteration, root='independent')]
assert isolated['audio_bindings']['bindings']['independent']['lattice']['gap_after'] == dict(
    type='captured', iteration=iteration)
assert intent(history('undo')) == intent(original)
assert intent(history('redo')) == intent(isolated)
command('lengthen', dict(command='set_hold_duration', node='independent', duration=3))
command('dormant', dict(command='set_repeat', node='repeat', plays=1, gap=gap))
dormant = snapshot()
assert dormant['gap_overrides'] == isolated['gap_overrides']
assert dormant['nodes']['independent']['kind']['recipe']['duration'] == 3
assert run('project', 'validate', package)['duration_frames'] == 1
command('revive', dict(command='set_repeat', node='repeat', plays=3, gap=gap))
assert run('project', 'validate', package)['duration_frames'] == 8
revived = snapshot()
command('clear', dict(command='clear_gap_override', node='repeat', iteration=iteration))
assert 'independent' not in snapshot()['nodes']
assert run('project', 'validate', package)['duration_frames'] == 7
assert intent(history('undo')) == intent(revived)
run('project', 'validate', package)
with sqlite3.connect(f'file:{package / "project.sqlite"}?mode=ro', uri=True) as database:
    assert database.execute('pragma user_version').fetchone() == (29,)
    revisions = database.execute('select id,document from revisions').fetchall()
    assert len({row[0] for row in revisions}) == len(revisions)
    assert all(json.loads(row[1])['schema_version'] == 23 for row in revisions)
    count = database.execute('select count(*) from history').fetchone()[0]
final_revision = snapshot()['revision_id']
report = dict(passed=True, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
              commands=len(log), revisions=len(revisions), history_entries=count,
              final_revision=final_revision,
              scope='Current CLI gap isolation, duration, dormant/revived ownership, clear, durable undo/redo')
(args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
