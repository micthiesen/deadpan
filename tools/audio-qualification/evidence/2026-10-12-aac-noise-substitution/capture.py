import hashlib,json,pathlib,struct,subprocess,time
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
r=json.loads((repo/'tools/audio-qualification/evidence/2026-10-12-partial-repeat-sounds/release-export-result.json').read_text())
binary=pathlib.Path('/tmp/deadpan-nested-eq-stages-20261012/old-deadpan-cli')
frames=r['report']['audio'][0]['window'][1]
samples=[]
calls=[]
started=time.monotonic()
for start in range(0,frames,256):
 end=min(start+256,frames)
 command=[str(binary),'inspect-audio',r['package'],'--samples',str(start),str(end),'--limited']
 result=subprocess.run(command,capture_output=True,text=True)
 calls.append(dict(command=command,exit=result.returncode,stderr=result.stderr))
 assert result.returncode==0,result.stderr
 value=json.loads(result.stdout)
 pair=value['audio']['samples']
 assert len(pair)==end-start
 samples.extend(pair)
 if start%2048==0: print(start,flush=True)
raw=b''.join(struct.pack('<ff',*p) for p in samples)
(s/'reference.f32le').write_bytes(raw)
(s/'reference.json').write_text(json.dumps(dict(frames=frames,package=r['package'],revision=r['report']['revision_id'],movie=r['movie'],binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),pcm_sha256=hashlib.sha256(raw).hexdigest(),seconds=time.monotonic()-started,calls=calls),indent=2)+'\n')
print('Captured',frames,'canonical limited stereo samples',flush=True)
