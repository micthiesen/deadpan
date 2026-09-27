from pathlib import Path
import hashlib
import json
import sqlite3
import subprocess

base = Path(__file__).parent
binary = Path('/tmp/deadpan-framing-20260924/native-gui/Deadpan Camera Review.app/Contents/MacOS/deadpan-app')
assert hashlib.sha256(binary.read_bytes()).hexdigest() == '4df29f2d65ba76d2cdcf88f4c361a38d416a224270aeae26b3d297052896036e'
project = base / 'fixture.deadpan'
log = []

def run(*args):
    command = [str(binary), '--headless', *map(str, args)]
    result = subprocess.run(command, capture_output=True, text=True)
    log.append(dict(command=command, returncode=result.returncode, stdout=result.stdout, stderr=result.stderr))
    (base / 'commands.json').write_text(json.dumps(log, indent=2) + '\n')
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)

def snapshot():
    return run('project', 'dump', project, '--json')

def command(revision, payload):
    document = snapshot()
    request = dict(protocol=1, project_id=document['project_id'], expected_revision=document['revision_id'], new_revision=revision, command=payload)
    path = base / (revision + '.json')
    path.write_text(json.dumps(request, indent=2) + '\n')
    return run('command', project, '--json', path)

def ratio(n, d=1):
    return dict(numerator=str(n), denominator=str(d))

def pose(scale):
    return dict(center_x=ratio(1, 2), center_y=ratio(1, 2), scale=scale)

run('project', 'create', project, '--fps', '30000/1001', '--size', '16x16')
command('pause', dict(command='insert_time', at=0, hold=dict(duration=8, video=dict(type='background'), audio=dict(type='silence')), id='hold', identities=dict(nodes=[]), timing=dict(allocation='pause', ordinal=0)))
command('frame', dict(command='set_framing', node='hold', framing=dict(value=dict(type='envelope', envelope=dict(initial=pose(ratio(1)), segments=[dict(end=ratio(1), pose=pose(ratio(27, 20)), curve=dict(type='smoothstep'))])))))
command('splice', dict(command='insert_time', at=3, hold=dict(duration=2, video=dict(type='background'), audio=dict(type='silence')), id='inserted', identities=dict(nodes=[f'splice-copy-{i}' for i in range(8)]), timing=dict(allocation='splice', ordinal=0)))
command('rename', dict(command='rename', node='inserted', label='Old framed pause'))
for verb in ('undo', 'redo', 'undo'):
    run('project', verb, project, '--expected', snapshot()['revision_id'])
run('project', 'validate', project)
final = snapshot()
(base / 'final.json').write_text(json.dumps(final, indent=2) + '\n')
with sqlite3.connect(project / 'project.sqlite') as database:
    assert database.execute('pragma user_version').fetchone() == (24,)
    assert all(json.loads(row[0])['schema_version'] == 18 for row in database.execute('select document from revisions'))
    application_id = database.execute('pragma application_id').fetchone()[0]
    sql = f'PRAGMA application_id={application_id};\nPRAGMA user_version=24;\n' + '\n'.join(database.iterdump()) + '\n'
    (base / 'v24-composition-history.sql').write_text(sql)
    counts = {table: database.execute(f'select count(*) from {table}').fetchone()[0] for table in ('revisions', 'history')}
report = dict(sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts, final_revision=final['revision_id'])
(base / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
