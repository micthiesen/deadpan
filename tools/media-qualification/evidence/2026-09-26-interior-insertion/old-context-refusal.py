from pathlib import Path
import hashlib,json,sqlite3,subprocess
root=Path('/tmp/deadpan-interior-splice-20260926')
work=root/'old-context-refusal-02'
work.mkdir()
project=work/'probe.deadpan'
project.mkdir()
(project/'Snapshots').mkdir()
binary=root/'core24-deadpan-cli'
assert hashlib.sha256(binary.read_bytes()).hexdigest()=='ba8c2c9dd860d0ae8540df6aaa3759d5f0b61e9bbadc87798fbbe2cabd35522a'
with sqlite3.connect(project/'project.sqlite') as db:
    db.executescript(Path('crates/deadpan-store/tests/fixtures/v30-interior-insert-history.sql').read_text())
log=[]
def run(*args,expected=0):
    result=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True)
    log.append(dict(args=list(map(str,args)),returncode=result.returncode,stdout=result.stdout,stderr=result.stderr))
    (work/'commands.json').write_text(json.dumps(log,indent=2)+'\n')
    assert result.returncode==expected,log[-1]
    return json.loads(result.stdout) if result.stdout else None

def command(name,payload,expected=0):
    doc=run('project','dump',project,'--json')
    path=work/(name+'.json')
    path.write_text(json.dumps(dict(protocol=1,project_id=doc['project_id'],expected_revision=doc['revision_id'],new_revision=name,command=payload),indent=2)+'\n')
    return run('command',project,'--json',path,expected=expected)

def contents():
    with sqlite3.connect(project/'project.sqlite') as db:
        return '\n'.join(db.iterdump())

def reject(name):
    before=contents()
    command(name,dict(command='insert_time',at=2,hold=dict(duration=2,video=dict(type='background'),audio=dict(type='silence')),id=name+'-hold',identities=dict(nodes=[name+'-left',name+'-right',name+'-source']),timing=dict(allocation=name,ordinal=0)),expected=1)
    assert before==contents()

reject('old-hold-interior')
doc=run('project','dump',project,'--json')
source=next(node for node in doc['nodes'].values() if node['kind']['type']=='source')
command('old-source-prefix',dict(command='insert',parent=doc['root'],index=0,subtree=dict(root='probe-source',nodes={'probe-source':source},overrides={})))
reject('old-source-interior')
run('project','validate',project)
print('Preserved core24 CLI rejected Source and Hold interior insertions before composites; database content unchanged on each refusal.')
