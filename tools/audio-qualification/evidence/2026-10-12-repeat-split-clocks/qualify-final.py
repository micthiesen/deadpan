import hashlib,json,pathlib,shutil,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
new=repo/'target/release/deadpan-cli'; old=s/'qualified-debug-cli'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
identity=sha(new)
fixture=json.loads((s/'release-export-result.json').read_text())
package=pathlib.Path(fixture['package'])
calls=[]
def call(binary,*args,ok=True):
 command=[str(binary),*map(str,args)]
 result=subprocess.run(command,capture_output=True,text=True)
 calls.append(dict(command=command,exit=result.returncode,stdout=result.stdout,stderr=result.stderr))
 if ok: assert result.returncode==0,result.stderr
 return result
before=call(new,'project','dump',package,'--json').stdout
document=json.loads(before)
revision=document['revision_id']
windows=[]
for start in [512,2114,6918,11723,22934]:
 result=call(new,'inspect-audio',package,'--samples',start,start+256,'--authored-bus')
 samples=json.loads(result.stdout)['audio']['samples']
 assert len(samples)==256 and any(abs(v)>0.001 for f in samples for v in f)
 historical=call(old,'inspect-audio',package,'--samples',start,start+256,'--authored-bus')
 assert json.loads(historical.stdout)['audio']['samples']==samples
 windows.append(dict(start=start,samples=samples))
output=s/'final-export'; output.mkdir(exist_ok=True)
result=call(new,'render',package,'--output',output,'--name','repeat-split.mp4','--expected',revision)
receipt=json.loads(result.stdout.splitlines()[-1])['status']
assert receipt['outcome']=='published'
movie=pathlib.Path(receipt['receipt']['movie'])
result=call(new,'verify-export',package,'--movie',movie,'--revision',revision,'--every','1')
report=json.loads(result.stdout)
assert report['passed'] and report['summary']['pictures_checked']==17
assert report['summary']['nonzero_offsets']==0 and report['summary']['offset_verified_windows']==1
assert call(new,'project','dump',package,'--json').stdout==before
assert sha(new)==identity
(s/'final-result.json').write_text(json.dumps(dict(new_binary_sha256=identity,debug_binary_sha256=sha(old),package=str(package),revision=revision,movie=str(movie),movie_sha256=sha(movie),receipt=receipt,report=report,authored_document_unchanged=True,pcm_bit_identical=True,windows=windows,calls=calls),indent=2)+'\n')
print(json.dumps(report['summary']))
