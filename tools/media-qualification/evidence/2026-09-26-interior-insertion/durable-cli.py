"""Public headless entrypoint: interior pause, repeated pause, reopen and undo.

Background/Silence checks structural persistence. Separate decoded PCM and
registered VFR service tests qualify the changed media mappings.
"""
import argparse, hashlib, json, sqlite3, subprocess
from pathlib import Path
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
binary=args.binary.resolve()
args.output.mkdir(exist_ok=False)
package=args.output/'interior.deadpan'
log=[]
def run(*arguments, success=True):
    invocation=[str(binary),'--headless',*map(str,arguments)]
    result=subprocess.run(invocation,capture_output=True,text=True,timeout=30)
    log.append(dict(command=invocation,exit_code=result.returncode,stdout=result.stdout,stderr=result.stderr))
    (args.output/'commands.json').write_text(json.dumps(log,indent=2)+'\n')
    assert (result.returncode==0)==success,result.stderr
    return json.loads(result.stdout) if success else result
def snapshot():
    return run('project','dump',package,'--json')
def command(name,payload,success=True):
    before=snapshot()
    request=dict(protocol=1,project_id=before['project_id'],expected_revision=before['revision_id'],new_revision=name,command=payload)
    path=args.output/(name+'.json')
    path.write_text(json.dumps(request,indent=2)+'\n')
    result=run('command',package,'--json',path,success=success)
    assert snapshot()['revision_id']==(name if success else before['revision_id'])
    return result
def navigate(verb):
    before=snapshot()
    run('project',verb,package,'--expected',before['revision_id'])
    after=snapshot()
    assert after['revision_id']!=before['revision_id']
    return after
def intent(document):
    return {k:v for k,v in document.items() if k!='revision_id'}
def pause(name,at):
    return dict(command='insert_time',at=at,hold=dict(recipe,duration=1),id=name,
                identities=dict(nodes=[f'{name}-part-{i}' for i in range(16)]),
                timing=dict(allocation=name,ordinal=0))
doctor=run('doctor')
assert (doctor['document_schema'],doctor['database_schema'])==(25,31)
run('project','create',package,'--fps','30000/1001','--size','16x16')
root=snapshot()['root']
recipe=dict(duration=2,video={'type':'background'},audio={'type':'silence'})
for name,index,frames in [('lead',0,4),('child',1,2)]:
    command('insert-'+name,dict(command='insert',parent=root,index=index,subtree=dict(
        root=name,nodes={name:dict(label=name,kind=dict(type='hold',recipe=dict(recipe,duration=frames)))},overrides={},gap_overrides={})))
command('repeat',dict(command='wrap_repeat',node='child',id='repeat',plays=3,gap=dict(recipe,duration=1)))
before=snapshot()
assert run('project','validate',package)['duration_frames']==12
command('pause-one',pause('pause-one',1))
first=snapshot()
assert run('project','validate',package)['duration_frames']==13
command('pause-two',pause('pause-two',3))
second=snapshot()
assert run('project','validate',package)['duration_frames']==14
assert second['nodes']['repeat']==before['nodes']['repeat']
assert second['nodes']['child']==before['nodes']['child']
for doc in (first,second):
    assert doc['nodes'][root]['kind']['children'][-1]=='repeat'
assert intent(navigate('undo'))==intent(first)
assert intent(navigate('undo'))==intent(before)
assert intent(navigate('redo'))==intent(first)
assert intent(navigate('redo'))==intent(second)
saved=snapshot()
failure=command('repeat-interior-refused',pause('repeat-interior-refused',7),success=False)
assert 'InvalidCommand' in failure.stderr, failure.stderr
assert snapshot()==saved
run('project','validate',package)
with sqlite3.connect(f'file:{package / "project.sqlite"}?mode=ro',uri=True) as db:
    assert db.execute('pragma user_version').fetchone()==(31,)
    revisions=db.execute('select id,document from revisions').fetchall()
    assert len({r[0] for r in revisions})==len(revisions)
    assert all(json.loads(r[1])['schema_version']==25 for r in revisions)
    assert 'repeat-interior-refused' not in {r[0] for r in revisions}
    history=db.execute('select count(*) from history').fetchone()[0]
    assert history==5
report=dict(passed=True,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),entrypoint='deadpan-app --headless',commands=len(log),revisions=len(revisions),history_entries=history,final_revision=saved['revision_id'],scope='Ordinary Hold interior before Repeat, repeated fragment insertion, process reopen on every command, two atomic undo/redo edits, nested Repeat interior refusal')
(args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
