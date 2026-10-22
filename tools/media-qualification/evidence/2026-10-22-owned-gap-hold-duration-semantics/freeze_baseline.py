from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21')
rows={}
for name in (s/'baseline-code-paths.txt').read_text().splitlines():
 p=Path(name)
 t=s/'baseline-source'/p
 t.parent.mkdir(parents=True,exist_ok=True)
 shutil.copy2(p,t)
 rows[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/'baseline-source-identities.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'Rust sources frozen')
