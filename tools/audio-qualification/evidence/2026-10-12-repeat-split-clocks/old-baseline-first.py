import hashlib,json,pathlib,shutil,subprocess
s=pathlib.Path(__file__).resolve().parent
old=s/'old-deadpan-cli'
fixture=json.loads((s/'cli-debug-result.json').read_text())
source=pathlib.Path(fixture['package'])
package=s/'old-baseline.deadpan'
shutil.copytree(source,package)
calls=[]
def call(*args,ok=True):
 command=[str(old),*map(str,args)]
 result=subprocess.run(command,capture_output=True,text=True)
 calls.append(dict(command=command,exit=result.returncode,stdout=result.stdout,stderr=result.stderr))
 (s/'old-baseline-calls.json').write_text(json.dumps(calls,indent=2)+'\n')
 if ok: assert result.returncode==0,result.stderr
 return result
for _ in range(3):
 doc=json.loads(call('project','dump',package,'--json').stdout)
 call('project','undo',package,'--expected',doc['revision_id'])
before=call('project','dump',package,'--json').stdout
request=json.loads((source.parent/'request-partial-attached-sound-copies-r11.json').read_text())
request['expected_revision']=json.loads(before)['revision_id']
request['new_revision']='old-baseline-attempt'
request['command']['timing']['allocation']=request['new_revision']
path=s/'old-baseline-request.json'; path.write_text(json.dumps(request,indent=2)+'\n')
result=call('command',package,'--json',path,ok=False)
assert result.returncode!=0
assert 'split sound owners inside Repeat need an occurrence route' in result.stderr,result.stderr
assert call('project','dump',package,'--json').stdout==before
(s/'old-baseline-result.json').write_text(json.dumps(dict(binary_sha256=hashlib.sha256(old.read_bytes()).hexdigest(),package=str(package),refused=True,document_unchanged=True,error=result.stderr),indent=2)+'\n')
print(result.stderr)
