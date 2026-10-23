from pathlib import Path
import subprocess,hashlib,json,sys
s=Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
stage=sys.argv[1]
rows=json.loads((s/(stage+'-source-identities.json')).read_text())
control=json.loads((s/'qualified-control-source-identities.json').read_text())
control_inputs=json.loads((s/'qualified-control-inputs.json').read_text())
inputs=json.loads((s/(stage+'-lock.json')).read_text())
new='crates/deadpan-core/tests/semantic_refresh.rs'
changed={'crates/deadpan-core/src/semantic/planner/selection.rs',new}
assert len(control)==2050 and len(rows)==2051
assert set(rows)==set(control)|{new}
assert {name for name in rows if rows[name]!=control.get(name)}==changed
assert inputs==control_inputs
names=set(subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines())
assert names==set(rows),'Rust source path set changed'
for name,digest in rows.items(): assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
for name,digest in inputs.items(): assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
print('Frozen Rust path set, all SHA256 identities and Cargo.lock unchanged:',stage,len(rows))
