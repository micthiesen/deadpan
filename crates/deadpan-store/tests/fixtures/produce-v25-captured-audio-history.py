from pathlib import Path
import hashlib
import json
import sqlite3
import subprocess

base = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = base / 'deadpan-cli-core19'
binary_sha = '849b0d0b1129f488e90dbeb391c6be9633769aa88a91dbc250bca08d5520d2e3'
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
project = base / 'source-selection-fixture-2.deadpan'
project.mkdir()
(project / 'Snapshots').mkdir()
input_fixture = repo / 'crates/deadpan-store/tests/fixtures/v24-composition-history.sql'
with sqlite3.connect(project / 'project.sqlite') as database:
    database.executescript(input_fixture.read_text())
log = []

def run(*args):
    command = [str(binary), *map(str, args)]
    result = subprocess.run(command, capture_output=True, text=True)
    log.append(dict(command=command, returncode=result.returncode, stdout=result.stdout, stderr=result.stderr))
    (base / 'source-selection-commands.json').write_text(json.dumps(log, indent=2) + '\n')
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)

def snapshot():
    return run('project', 'dump', project, '--json')

def command(revision, payload):
    doc = snapshot()
    request = dict(protocol=1, project_id=doc['project_id'], expected_revision=doc['revision_id'], new_revision=revision, command=payload)
    path = base / (revision + '.json')
    path.write_text(json.dumps(request, indent=2) + '\n')
    return run('command', project, '--json', path)

def ratio(n, d=1):
    return dict(numerator=str(n), denominator=str(d))

run('doctor')
run('project', 'migrate', project)
run('project', 'redo', project, '--expected', snapshot()['revision_id'])
context = dict(canvases=[dict(width=16, height=16, fit='fill', layers=[dict(center_x=ratio(1,2), center_y=ratio(1,2), scale=ratio(3,2)), None])])
command('capture', dict(command='set_hold_picture_context', node='inserted', context=context))
span = dict(start=dict(ticks=0,time_base=dict(numerator=1,denominator=48000)), end=dict(ticks=48000,time_base=dict(numerator=1,denominator=48000)))
command('audio-asset', dict(command='add_asset', id='audio', asset=dict(label='Retained original audio fixture',content_hash='a'*64,video=None,audio=span,still_image=False,frame_count=None)))
doc = snapshot()
root = doc['root']
index = len(doc['nodes'][root]['kind']['children'])
source = dict(duration=30,video=dict(type='blank'),audio=dict(asset='audio',span=span),link='independent',audio_offset=-137,audio_mapping=dict(type='placement',start=ratio(-1,3),frames=ratio(30000,1001)),video_mapping=dict(type='fit_beat'))
command('audio-source', dict(command='insert', parent=root,index=index,subtree=dict(root='source',nodes=dict(source=dict(label='Original region',kind=dict(type='source',source=source))),overrides={})))
command('audio-placement',dict(command='set_source_audio_mapping',node='source',mapping=dict(type='placement',start=ratio(-2,3),frames=ratio(30000,1001)),offset=-73))
gap=dict(duration=2,video=dict(type='background'),audio=dict(type='silence'),picture_context=context)
command('captured-repeat',dict(command='wrap_repeat',node='inserted',id='repeat',plays=2,gap=gap))
# No repeated occurrence is materialized for this root-level edit, but it exercises
# the old occurrence command's captured-context vocabulary.
command('occurrence-capture',dict(command='edit_occurrence',instance=dict(node='source',repeats=[]),edit=dict(type='set_source_audio_mapping',mapping=dict(type='duration',frames=ratio(30000,1001)),offset=-31),identities=dict(nodes=[],marks=[])))
command('rename-captured',dict(command='rename',node='source',label='Retained pending rename'))
for verb in ('undo','redo','undo'):
    run('project',verb,project,'--expected',snapshot()['revision_id'])
run('project','validate',project)
final=snapshot()
with sqlite3.connect(project/'project.sqlite') as db:
    assert db.execute('pragma user_version').fetchone()==(25,)
    assert all(json.loads(row[0])['schema_version']==19 for row in db.execute('select document from revisions'))
    appid=db.execute('pragma application_id').fetchone()[0]
    sql=f'PRAGMA application_id={appid};\nPRAGMA user_version=25;\n'+'\n'.join(db.iterdump())+'\n'
    (base/'v25-captured-audio-history.sql').write_text(sql)
    counts={table:db.execute(f'select count(*) from {table}').fetchone()[0] for table in ('revisions','history')}
report=dict(binary_sha256=binary_sha,input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),sql_sha256=hashlib.sha256(sql.encode()).hexdigest(),counts=counts,final_revision=final['revision_id'],core_schema=19,database_schema=25)
(base/'source-selection-report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
