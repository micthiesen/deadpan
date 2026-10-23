from pathlib import Path
import subprocess,hashlib,json,shutil,sys
s=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
stage=sys.argv[1]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
delta=json.loads((s/'expected-source-delta.json').read_text())
baseline=json.loads((s/'qualified-control-source-identities.json').read_text())
baseline_inputs=json.loads((s/'qualified-control-inputs.json').read_text())
rows={}
for name in subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines():
 p=Path(name); target=s/(stage+'-source')/name
 target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,target);rows[name]=sha(p)
inputs={}
for name in baseline_inputs:
 p=Path(name); target=s/(stage+'-inputs')/name
 target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,target);inputs[name]=sha(p)
assert len(baseline)==2051 and len(rows)==2053
assert set(rows)==set(baseline)|set(delta['new'])
assert {name for name in rows if rows[name]!=baseline.get(name)}==set(delta['changed'])
assert inputs==baseline_inputs
(s/(stage+'-source-identities.json')).write_text(json.dumps(rows,indent=2)+'\n')
(s/(stage+'-lock.json')).write_text(json.dumps(inputs,indent=2)+'\n')
print(len(rows),'Rust files frozen for',stage)
