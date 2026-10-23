from pathlib import Path
import subprocess,hashlib,json,sys
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22')
stage=sys.argv[1]
rows=json.loads((s/(stage+'-source-identities.json')).read_text())
names=set(subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines())
assert names==set(rows),'Rust source path set changed'
for name,digest in rows.items(): assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
for name,digest in json.loads((s/(stage+'-lock.json')).read_text()).items(): assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
print('Frozen Rust path set, all SHA256 identities and Cargo.lock unchanged:',stage,len(rows))
