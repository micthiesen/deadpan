from pathlib import Path
import subprocess,hashlib,json,shutil,sys
s=Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
stage=sys.argv[1]
names=subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines()
rows={}
for name in names:
 p=Path(name); target=s/(stage+'-source')/name
 target.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(p,target)
 rows[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/(stage+'-source-identities.json')).write_text(json.dumps(rows,indent=2)+'\n')
inputs={}
for name in ['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md']:
 p=Path(name); target=s/(stage+'-inputs')/name
 target.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(p,target)
 inputs[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/(stage+'-lock.json')).write_text(json.dumps(inputs,indent=2)+'\n')
baseline=json.loads((s/'qualified-control-source-identities.json').read_text())
baseline_inputs=json.loads((s/'qualified-control-inputs.json').read_text())
expected_new='crates/deadpan-core/tests/semantic_refresh.rs'
expected_changed={'crates/deadpan-core/src/semantic/planner/selection.rs',expected_new}
assert len(baseline)==2050 and len(rows)==2051
assert set(rows)==set(baseline)|{expected_new}
assert {name for name in rows if rows[name]!=baseline.get(name)}==expected_changed
assert inputs==baseline_inputs
print(len(rows),'Rust files frozen for',stage)
