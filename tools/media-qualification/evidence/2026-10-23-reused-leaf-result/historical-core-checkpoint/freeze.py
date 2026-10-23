from pathlib import Path
import subprocess,hashlib,json,shutil,sys
s=Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
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
print(len(rows),'Rust files frozen for',stage)
