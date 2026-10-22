from pathlib import Path
import hashlib,json,shutil
scratch=Path('/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22')
hashes=json.loads((scratch/'final-source-identities.json').read_text())
for source,digest in hashes.items():
    assert hashlib.sha256(Path(source).read_bytes()).hexdigest()==digest,source
rows={}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
    source=Path('target/release')/name
    target=scratch/'bin'/name
    target.parent.mkdir(exist_ok=True)
    shutil.copy2(source,target)
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    assert hashlib.sha256(target.read_bytes()).hexdigest()==digest
    rows[name]={'source':str(source),'executed':str(target),'sha256':digest}
(scratch/'binaries.json').write_text(json.dumps(rows,indent=2)+'\n')
print(f'{len(hashes)} frozen sources verified; four release executables copied and hashed')
