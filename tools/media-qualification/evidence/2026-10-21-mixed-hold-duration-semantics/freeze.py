from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-mixed-hold-duration-next-2026-10-21')
paths=set()
for name in ['tracked-code-paths.txt','new-code-paths.txt']:
 paths.update((s/name).read_text().splitlines())
hashes={}
for name in sorted(paths):
 p=Path(name)
 if p.suffix!='.rs' or not p.is_file(): continue
 dst=s/'final-source'/name
 dst.parent.mkdir(parents=True,exist_ok=True)
 shutil.copy2(p,dst)
 hashes[name]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/'source-identities.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(len(hashes),'frozen Rust files')
