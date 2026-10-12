import hashlib,json,pathlib,struct,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
old=s/'old-deadpan-cli'; new=repo/'target/release/deadpan-cli'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
previous=repo/'tools/audio-qualification/evidence/2026-10-12-source-projection-reuse'
assert sha(old)==json.loads((previous/'identity.json').read_text())['final_release_binaries']['target/release/deadpan-cli']
r=json.loads((s/'reference.json').read_text())
calls=[]
def call(binary,*args):
 command=[str(binary),*map(str,args)]
 result=subprocess.run(command,capture_output=True,text=True)
 calls.append(dict(command=command,exit=result.returncode,stdout=result.stdout,stderr=result.stderr))
 return result
result=call(old,'verify-export',r['package'],'--movie',r['movie'],'--revision',r['revision'],'--every','1')
report=json.loads(result.stdout)
assert result.returncode!=0 and not report['passed']
assert report['audio'][0]['flags']==['audio_block_snr'] and report['summary']['nonzero_offsets']==0
(s/'old-verify-report.json').write_text(json.dumps(report,indent=2)+'\n')
before=call(new,'project','dump',r['package'],'--json'); assert before.returncode==0
pcm=[]
for start in range(0,r['frames'],256):
 end=min(start+256,r['frames'])
 result=call(new,'inspect-audio',r['package'],'--samples',start,end,'--limited')
 assert result.returncode==0,result.stderr
 samples=json.loads(result.stdout)['audio']['samples']; assert len(samples)==end-start
 pcm.extend(samples)
raw=b''.join(struct.pack('<ff',*pair) for pair in pcm)
assert raw==(s/'reference.f32le').read_bytes()
fixtures={'partial-repeat':dict(package=r['package'],revision=r['revision'],pictures=13,windows=1),'nested-eq':dict(**json.loads((previous/'real-media.json').read_text()),pictures=120,windows=4)}
reports={}
for name,fixture in fixtures.items():
 output=s/('final-'+name); output.mkdir(exist_ok=True)
 result=call(new,'render',fixture['package'],'--output',output,'--name',name+'.mp4','--expected',fixture['revision'])
 assert result.returncode==0,result.stderr
 receipt=json.loads(result.stdout.splitlines()[-1])['status']; assert receipt['outcome']=='published'
 movie=receipt['receipt']['movie']
 result=call(new,'verify-export',fixture['package'],'--movie',movie,'--revision',fixture['revision'],'--every','1')
 report=json.loads(result.stdout)
 assert result.returncode==0 and report['passed'],report['summary']
 assert report['summary']['pictures_checked']==fixture['pictures'] and report['summary']['audio_windows_checked']==fixture['windows']
 assert report['summary']['nonzero_offsets']==0 and report['summary']['offset_verified_windows']==1
 (s/(name+'-verify-report.json')).write_text(json.dumps(report,indent=2)+'\n')
 reports[name]=dict(movie=movie,sha256=sha(pathlib.Path(movie)),summary=report['summary'],receipt=receipt)
after=call(new,'project','dump',r['package'],'--json'); assert after.returncode==0 and after.stdout==before.stdout
(s/'final-result.json').write_text(json.dumps(dict(old_binary_sha256=sha(old),new_binary_sha256=sha(new),pcm_bit_identical=True,pcm_sha256=sha(s/'reference.f32le'),authored_document_unchanged=True,reports=reports,calls=calls),indent=2)+'\n')
print(json.dumps(dict(pcm_bit_identical=True,reports={k:v['summary'] for k,v in reports.items()})))
