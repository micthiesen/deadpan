from pathlib import Path
import hashlib,json,subprocess,sqlite3
root=Path('/tmp/deadpan-nested-sequence-20260926')
binary=root/'deadpan-core25'
work=root/'old-context-refusal'
work.mkdir()
package=work/'nested.deadpan'
records=[]
def run(*args,success=True):
    result=subprocess.run([str(binary),*map(str,args)],text=True,capture_output=True)
    records.append(dict(command=[str(binary),*map(str,args)],returncode=result.returncode,stdout=result.stdout,stderr=result.stderr))
    assert (result.returncode==0)==success,records[-1]
    return json.loads(result.stdout) if result.returncode==0 else result

def dump(): return run('project','dump',package,'--json')
def command(revision,payload,success=True):
    doc=dump()
    path=work/(revision+'.json')
    path.write_text(json.dumps(dict(protocol=1,project_id=doc['project_id'],expected_revision=doc['revision_id'],new_revision=revision,command=payload),indent=2)+'\n')
    return run('command',package,'--json',path,success=success)
def hold(n): return dict(label='Hold',kind=dict(type='hold',recipe=dict(duration=n,video=dict(type='background'),audio=dict(type='silence'))))
def seq(*children): return dict(label='Group',kind=dict(type='sequence',children=list(children)))
run('doctor')
run('project','create',package)
doc=dump()
command('nested-seed',dict(command='insert',parent=doc['root'],index=0,subtree=dict(root='outer',nodes=dict(outer=seq('inner'),inner=seq('first','second'),first=hold(4),second=hold(3)),overrides={},gap_overrides={})))
before=dump()
for at in (2,4):
    command('refused-'+str(at),dict(command='insert_time',at=at,hold=hold(2)['kind']['recipe'],id='pause-'+str(at),identities=dict(nodes=['left','right','owner']),timing=dict(allocation='refused-'+str(at),ordinal=0)),success=False)
assert before==dump()
run('project','validate',package)
report=dict(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),script=str(Path(__file__).resolve()),script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),schema=(25,31),records=records,unchanged_after_refusals=True)
(work/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'summary':str(work/'summary.json'),'refusals': [r['stderr'] for r in records if r['returncode']]}))
