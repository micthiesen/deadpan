from pathlib import Path
import json,subprocess
root=Path(__file__).parent
cli='target/release/deadpan-cli'
package=str(root/'future.deadpan')
def dump():return json.loads(subprocess.check_output([cli,'project','dump',package,'--json']))
def commit(name,command):
 doc=dump()
 request=dict(protocol=1,project_id=doc['project_id'],expected_revision=doc['revision_id'],command=command)
 path=root/(name+'-request.json');path.write_text(json.dumps(request,indent=2)+'\n')
 subprocess.run(['python3',str(root/'run.py'),name,cli,'command',package,'--json',str(path)],check=True)
doc=dump(); source=doc['nodes'][doc['root']]['kind']['children'][0]
commit('fixture-retime',dict(command='wrap_retime',node=source,id='future-stretch',duration=240,pitch='preserve'))
commit('fixture-lead',dict(command='insert',parent=doc['root'],index=0,subtree=dict(root='lead',nodes={'lead':dict(label='Silent lead',kind=dict(type='hold',recipe=dict(duration=240,video=dict(type='background'),audio=dict(type='silence'))))})))
(root/'fixture-document.json').write_text(json.dumps(dump(),indent=2)+'\n')
