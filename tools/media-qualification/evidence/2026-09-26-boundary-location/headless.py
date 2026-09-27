"""Compare the native app and CLI exact boundary APIs across durable queries.

Synthetic Background/Silence structure only. No GPU, device, or media claim.
"""
import argparse, hashlib, json, sqlite3, subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('app', type=Path)
parser.add_argument('cli', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
args.output.mkdir(exist_ok=False)
app, cli = args.app.resolve(), args.cli.resolve()
package = args.output / 'boundary.deadpan'
records = []

def run(binary, *arguments, success=True):
    command = [str(binary), *(['--headless'] if binary == app else []), *map(str, arguments)]
    result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    records.append(dict(command=command, exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    (args.output / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
    assert (result.returncode == 0) == success, records[-1]
    return json.loads(result.stdout if success else result.stderr)

def snapshot():
    return run(app, 'project', 'dump', package, '--json')

def ratio(n, d=1):
    return dict(numerator=str(n), denominator=str(d))

def hold(frames):
    return dict(label='Pause', kind=dict(type='hold', recipe=dict(
        duration=frames, video=dict(type='background'), audio=dict(type='silence'))))

def retime(child, frames, start, end):
    return dict(label='Retime', kind=dict(type='retime', child=child, duration=frames,
        mapping=dict(start=start, end=end), pitch='preserve'))

def history():
    with sqlite3.connect(f'file:{package / "project.sqlite"}?mode=ro', uri=True) as db:
        return dict(revisions=db.execute('select count(*) from revisions').fetchone()[0],
            history=db.execute('select count(*) from history').fetchone()[0],
            redo=db.execute('select count(*) from redo').fetchone()[0])

run(app, 'project', 'create', package, '--fps', '30000/1001', '--size', '16x16')
empty = snapshot()
nodes = dict(
    scope=dict(label='Group', kind=dict(type='sequence', children=['prefix', 'outer', 'suffix'])),
    prefix=hold(2), suffix=hold(1), outer=retime('repeat', 7, 0, 14),
    repeat=dict(label='Repeat', kind=dict(type='repeat', child='inner',
        iterations=dict(runs=[dict(allocation='imported', first=0, count=2)]),
        gap=hold(2)['kind']['recipe'])),
    inner=retime('leaf', 6, 1, 10), leaf=hold(10),
)
request = dict(protocol=1, project_id=empty['project_id'], expected_revision=empty['revision_id'],
    new_revision='fixture', command=dict(command='insert', parent=empty['root'], index=0,
        subtree=dict(root='scope', nodes=nodes, overrides={}, gap_overrides={})))
input_path = args.output / 'insert.json'
input_path.write_text(json.dumps(request, indent=2) + '\n')
run(app, 'command', package, '--json', input_path)
before = snapshot()
before_history = history()
before_database = hashlib.sha256((package / 'project.sqlite').read_bytes()).hexdigest()
cases = [
    ('fractional', ratio(29, 4), 'right', 'node', 'leaf', ratio(19, 4)),
    ('implicit-gap', ratio(11, 2), 'right', 'gap', 'repeat', ratio(7)),
    ('gap-start-left', ratio(5), 'left', 'node', 'leaf', ratio(10)),
    ('gap-start-right', ratio(5), 'right', 'gap', 'repeat', ratio(6)),
    ('start-outward', ratio(0), 'left', 'project_start', before['root'], ratio(0)),
    ('end-outward', ratio(10), 'right', 'project_end', before['root'], ratio(10)),
    ('end-inward', ratio(10), 'left', 'node', 'suffix', ratio(1)),
]
for name, position, bias, terminal, node, local in cases:
    envelope = dict(protocol=1, request=dict(project_id=before['project_id'], expected_revision='fixture',
        position=position, bias=bias))
    input_path = args.output / f'{name}.json'
    input_path.write_text(json.dumps(envelope, indent=2) + '\n')
    reports = [run(binary, 'locate-boundary', package, '--json', input_path) for binary in (app, cli)]
    assert reports[0] == reports[1], name
    location = reports[0]['location']
    assert location['terminal']['type'] == terminal, location
    assert location['scopes'][-1]['instance']['node'] == node, location
    assert location['scopes'][-1]['position'] == local, location
    for ordinal, scope in enumerate(location['scopes']):
        selection = dict(protocol=1, request=dict(project_id=before['project_id'], expected_revision='fixture',
            role='linked', selector=dict(type='point', target=dict(boundary=dict(
                coordinate=dict(space='occurrence', instance=scope['instance'], position=scope['position']), bias=bias)))))
        selected_path = args.output / f'{name}-owner-{ordinal}.json'
        selected_path.write_text(json.dumps(selection, indent=2) + '\n')
        resolved = run(app, 'resolve-selection', package, '--json', selected_path)
        assert resolved['resolved']['selection']['point']['exact_frame'] == position
    assert snapshot() == before

for name, change, code in [
    ('stale', dict(expected_revision='stale'), 'RevisionConflict'),
    ('outside', dict(position=ratio(21, 2)), 'OutOfRange'),
]:
    query = dict(project_id=before['project_id'], expected_revision='fixture', position=ratio(1), bias='right')
    query.update(change)
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(dict(protocol=1, request=query), indent=2) + '\n')
    for binary in (app, cli):
        assert run(binary, 'locate-boundary', package, '--json', path, success=False)['error']['code'] == code

assert snapshot() == before
assert history() == before_history
assert hashlib.sha256((package / 'project.sqlite').read_bytes()).hexdigest() == before_database
report = dict(passed=True, commands=len(records), cases=len(cases), history=before_history,
    app_sha256=hashlib.sha256(app.read_bytes()).hexdigest(), cli_sha256=hashlib.sha256(cli.read_bytes()).hexdigest(),
    database_sha256=before_database, database_bytes_unchanged=True,
    scope='native app headless / CLI parity, exact scope inverse coordinates, gap/seam/endpoint bias, failure no-write')
(args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
