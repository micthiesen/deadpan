import hashlib,json,pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
report=json.loads((s/'real-media.json').read_text())
old=s/'old-deadpan-cli'
new=repo/('target/debug/deadpan-cli' if '--debug' in sys.argv else 'target/release/deadpan-cli')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
identity=json.loads((repo/'tools/audio-qualification/evidence/2026-10-12-partial-repeat-sounds/identity.json').read_text())
assert sha(old)==identity['final_release_binaries']['target/release/deadpan-cli']
calls=[]
def call(binary,*args):
 result=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True)
 calls.append(dict(command=[str(binary),*map(str,args)],exit=result.returncode,stdout=result.stdout,stderr=result.stderr))
 return result
package=report['package']
before=call(old,'project','dump',package,'--json')
assert before.returncode==0,before.stderr
args=['inspect-audio',package,'--samples','512','768','--authored-bus']
rejected=call(old,*args)
assert rejected.returncode==0,rejected.stderr
unchanged=call(old,'project','dump',package,'--json')
assert unchanged.returncode==0 and unchanged.stdout==before.stdout
accepted=call(new,*args)
assert accepted.returncode==0,accepted.stderr
samples=json.loads(accepted.stdout)['audio']['samples']
assert samples==json.loads(rejected.stdout)['audio']['samples']
assert len(samples)==256 and any(abs(v)>0.001 for f in samples for v in f)
name='baseline-debug-result.json' if '--debug' in sys.argv else 'baseline-result.json'
(s/name).write_text(json.dumps(dict(old_source='0c445fc3',old_binary_sha256=sha(old),new_binary_sha256=sha(new),old_and_new_accepted=True,authored_document_unchanged=True,pcm_bit_identical=True,calls=calls),indent=2)+'\n')
print('Old and new binaries prepare bit-identical 256-frame inspection windows; full Render is a separate comparison.')
