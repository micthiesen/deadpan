from pathlib import Path
import subprocess,json,sqlite3,hashlib
base=Path(__file__).parent
binary=Path('/tmp/deadpan-master-20260924/native-gui/Deadpan Limited Review.app/Contents/MacOS/deadpan-app')
assert hashlib.sha256(binary.read_bytes()).hexdigest()=='f68cb731edbc6ae1c3e7e4ce416dac7aa158156d61beba80b12d059bcbc3e5b9'
project=base/'fixture.deadpan'
log=[]
def run(*args):
    command=[str(binary),'--headless',*map(str,args)]
    result=subprocess.run(command,capture_output=True,text=True)
    log.append(dict(command=command,returncode=result.returncode,stdout=result.stdout,stderr=result.stderr))
    (base/'commands.json').write_text(json.dumps(log,indent=2)+'\n')
    if result.returncode:raise RuntimeError(result.stderr)
    return json.loads(result.stdout)
def snapshot():return run('project','dump',project,'--json')
def command(revision,payload):
    doc=snapshot(); request=dict(protocol=1,project_id=doc['project_id'],expected_revision=doc['revision_id'],new_revision=revision,command=payload)
    path=base/(revision+'.json');path.write_text(json.dumps(request,indent=2)+'\n')
    return run('command',project,'--json',path)
for rev,at,length,node in [('pause-one',0,8,'hold'),('pause-two',3,2,'inserted')]:
    doc=snapshot()
    command(rev,dict(command='insert_time',at=at,hold=dict(duration=length,video=dict(type='background'),audio=dict(type='silence')),id=node,identities=dict(nodes=[f'{rev}-copy-{i}' for i in range(len(doc['nodes'])+4)]),timing=dict(allocation=rev,ordinal=0)))
command('rename',dict(command='rename',node='inserted',label='Old binary pause'))
for verb in ['undo','redo','undo']:
    run('project',verb,project,'--expected',snapshot()['revision_id'])
run('project','validate',project)
final=snapshot();(base/'final.json').write_text(json.dumps(final,indent=2)+'\n')
with sqlite3.connect(project/'project.sqlite') as db:
    assert db.execute('pragma user_version').fetchone()==(23,)
    assert all(json.loads(row[0])['schema_version']==17 for row in db.execute('select document from revisions'))
    sql='\n'.join(db.iterdump())+'\n'
    # iterdump omits these header pragmas; preserve observed values explicitly.
    sql=f"PRAGMA application_id={db.execute('pragma application_id').fetchone()[0]};\nPRAGMA user_version=23;\n"+sql
    (base/'v23-framing-history.sql').write_text(sql)
print(dict(sql_sha256=hashlib.sha256(sql.encode()).hexdigest(),revisions=7,final_revision=final['revision_id']))
