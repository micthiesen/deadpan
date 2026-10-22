from pathlib import Path
import subprocess,hashlib,json,shutil
s=Path('/tmp/deadpan-nested-fixed-retime-duration-2026-10-22')
names=subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines()
assert len(names)>1900
rows={}
for name in names:
 p=Path(name); target=s/'baseline-source'/name
 target.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(p,target)
 rows[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/'baseline-source-identities.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'Rust files frozen')
