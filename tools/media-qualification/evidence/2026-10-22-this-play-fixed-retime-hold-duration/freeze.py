from pathlib import Path
import hashlib,json,shutil,sys
s=Path('/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22')
stage=sys.argv[1]
rows={}
for name in (s/'code-paths.txt').read_text().splitlines():
 p=Path(name); t=s/stage/p
 t.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(p,t)
 rows[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/(stage+'-identities.json')).write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'Rust files frozen in',stage)
