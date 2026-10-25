from pathlib import Path
import subprocess,json,hashlib
d=Path('/tmp/deadpan-word-refinement-2026-10-25/negative-fixture')
exe='/tmp/deadpan-follow-creep-2026-10-24/target/debug/deadpan-cli'
(d/'binary-sha256.txt').write_text(hashlib.sha256(Path(exe).read_bytes()).hexdigest()+'\n')
def call(tag,args):
 (d/(tag+'.argv.json')).write_text(json.dumps([exe,*args]))
 p=subprocess.run([exe,*args],capture_output=True)
 (d/(tag+'.stdout')).write_bytes(p.stdout);(d/(tag+'.stderr')).write_bytes(p.stderr);(d/(tag+'.exit')).write_text(str(p.returncode))
 print(tag,p.returncode,p.stderr.decode()[:1000])
 return json.loads(p.stdout) if p.returncode==0 else None
package=d/'registered.deadpan'
call('create',['project','create',str(package),'--fps','30000/1001','--size','320x180'])
r=call('retain',['project','retain-original',str(package),str(d/'negative-opus.mp4')])
print(str(r)[:500])
if r:
 req={'protocol':1,'registration':{'expected_revision':'initial','new_revision':'registered','original':r['retained_original']['record']['object']['content'],'new_asset_id':'speech','label':'Speech','insertion':{'parent':'root','index':0,'node':'speech-source','label':'Speech'}},'streams':{'type':'audio_only','stream':0}}
 (d/'request.json').write_text(json.dumps(req))
 call('register',['project','register-source',str(package),'--request-json',str(d/'request.json')])
