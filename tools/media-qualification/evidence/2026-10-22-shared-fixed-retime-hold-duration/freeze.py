from pathlib import Path
import hashlib,json,shutil,sys
s=Path('/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22')
name=sys.argv[1] if len(sys.argv)>1 else 'final-source'
paths=set((s/'tracked-code-paths.txt').read_text().splitlines())|set((s/'new-code-paths.txt').read_text().splitlines())
rows={}
for source in sorted(paths):
    p=Path(source)
    if p.suffix!='.rs' or not p.is_file(): continue
    target=s/name/source
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copy2(p,target)
    rows[source]=hashlib.sha256(p.read_bytes()).hexdigest()
(s/(name+'-identities.json')).write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'Rust files frozen in',name)
