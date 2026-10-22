from pathlib import Path
import hashlib,json,shutil
scratch=Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
listing=json.loads((scratch/'focused-list.json').read_text())
old=json.loads((scratch/'first-runtime-binaries.json').read_text())['binaries']
rows={}
for name in old:
    path=Path(listing['rust-suites'][name]['binary-path'])
    assert str(path)==old[name]['path'], (name,path,old[name]['path'])
    retained=scratch/'focused-bin'/path.name
    retained.parent.mkdir(exist_ok=True)
    shutil.copy2(path,retained)
    digest=hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest==hashlib.sha256(retained.read_bytes()).hexdigest()
    rows[name]={'path':str(path),'retained':str(retained),'sha256':digest}
(scratch/'focused-binaries.json').write_text(json.dumps(rows,indent=2)+'\n')
print('Five exact current-graph executable paths verified and frozen; all match first-runtime path provenance')
