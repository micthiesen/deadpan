import hashlib,json,pathlib,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
old=s/'old-deadpan-cli'
new=repo/'target/release/deadpan-cli'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
identity=json.loads((repo/'tools/audio-qualification/evidence/2026-10-12-copy-mastering-context/identity.json').read_text())
assert sha(old)==identity['binaries']['target/release/deadpan-cli']
import zipfile
with zipfile.ZipFile(repo/'tools/audio-qualification/evidence/2026-10-12-partial-sound-copies/project.zip') as archive: archive.extractall(s/'baseline-final')
package=s/'baseline-final/partial-attached-sound-copies.deadpan'
calls=[]
def call(binary,*args):
    result=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True)
    calls.append(dict(command=[str(binary),*map(str,args)],exit=result.returncode,stdout=result.stdout,stderr=result.stderr))
    return result
before=call(old,'project','dump',package,'--json')
assert before.returncode==0
document=json.loads(before.stdout)
fixture=json.loads((s/'debug-export.json').read_text())
request=json.loads((pathlib.Path(fixture['package']).parent/'request-partial-attached-sound-copies-r10.json').read_text())
request['project_id']=document['project_id']
request['expected_revision']=document['revision_id']
path=s/'baseline-final-request.json'
path.write_text(json.dumps(request,indent=2)+'\n')
rejected=call(old,'command',package,'--json',path)
assert rejected.returncode!=0 and 'sound owner interval changes require an attachment route' in rejected.stderr
unchanged=call(old,'project','dump',package,'--json')
assert unchanged.returncode==0 and unchanged.stdout==before.stdout
accepted=call(new,'command',package,'--json',path)
assert accepted.returncode==0, accepted.stderr
after=call(new,'project','dump',package,'--json')
assert after.returncode==0 and len(json.loads(after.stdout)['beat_sounds'])==3
(s/'baseline-final-result.json').write_text(json.dumps(dict(old_source='743b349d',old_binary_sha256=sha(old),new_binary_sha256=sha(new),old_refused_without_changes=True,new_accepted=True,calls=calls),indent=2)+'\n')
print('Prior qualified binary refuses atomically; current binary accepts the same request and keeps all three sound owners.')
