from pathlib import Path
import subprocess,hashlib,json,sys
s=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23');stage=sys.argv[1]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
rows=json.loads((s/(stage+'-source-identities.json')).read_text())
inputs=json.loads((s/(stage+'-lock.json')).read_text())
control=json.loads((s/'qualified-control-source-identities.json').read_text())
delta=json.loads((s/'expected-source-delta.json').read_text())
assert len(control)==2051 and len(rows)==2053
assert set(rows)==set(control)|set(delta['new'])
assert {name for name in rows if rows[name]!=control.get(name)}==set(delta['changed'])
assert inputs==json.loads((s/'qualified-control-inputs.json').read_text())
assert set(subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines())==set(rows)
for name,digest in rows.items(): assert sha(Path(name))==digest,name
for name,digest in inputs.items(): assert sha(Path(name))==digest,name
print('Frozen Rust path set, all SHA256 identities and six inputs unchanged:',stage,len(rows))
