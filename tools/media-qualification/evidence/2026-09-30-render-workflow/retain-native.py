import gzip
import hashlib
import json
import sqlite3
import tarfile
from pathlib import Path

root = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
destination = repo/'tools/media-qualification/evidence/2026-09-30-render-workflow'
destination.mkdir(parents=True, exist_ok=True)
report = json.loads((root/'native-report.json').read_text())
decoded = json.loads((root/'decoded-workflow/report.json').read_text())
assert report['status'] == 'passed' and decoded['result'] == 'passed workflow publications'
assert all(c['passed'] for c in decoded['final_file_admission'])

def digest(path):
    with path.open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()

receipts = {}
def visit(value):
    if isinstance(value, dict):
        if all(key in value for key in ('movie','report','movie_sha256','report_sha256','movie_bytes','report_bytes')):
            for kind in ('movie', 'report'):
                path = Path(value[kind])
                assert path.stat().st_size == value[kind+'_bytes']
                actual = digest(path)
                assert actual == value[kind+'_sha256']
                receipts[str(path)] = {'bytes':path.stat().st_size, 'sha256':actual}
        for item in value.values():
            visit(item)
    elif isinstance(value, list):
        for item in value:
            visit(item)
visit(report)
assert len(receipts) == 8

snapshots = root/'verified-databases'
snapshots.mkdir()
for name in ('source', 'generated'):
    path = root/'native-inputs'/(name+'.deadpan')/'project.sqlite'
    with sqlite3.connect(f'file:{path}?mode=ro', uri=True) as source:
        with sqlite3.connect(snapshots/(name+'.sqlite')) as target:
            source.backup(target)

files = sorted(p for folder in ('native-output','decoded-workflow','native-inputs','verified-databases')
               for p in (root/folder).rglob('*') if p.is_file())
assert all(not p.is_symlink() for p in files)
inventory = {str(p.relative_to(root)):{'bytes':p.stat().st_size,'sha256':digest(p)} for p in files}
archive = destination/'native-artifacts.tar.gz'
with archive.open('wb') as raw:
    with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w|') as bundle:
            for path in files:
                bundle.add(path, arcname=str(path.relative_to(root)), recursive=False)
audit = {'status':'passed','receipt_files':receipts,'archive':{'path':str(archive.relative_to(repo)),
    'sha256':digest(archive),'bytes':archive.stat().st_size,'file_count':len(inventory),
    'uncompressed_file_bytes':sum(x['bytes'] for x in inventory.values())},'files':inventory}
(destination/'native-artifact-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
for source,name in ((root/'native-report.json','native-report.json.gz'),
                    (root/'decoded-workflow/report.json','decoded-report.json.gz')):
    (destination/name).write_bytes(gzip.compress(source.read_bytes(),mtime=0))
print(json.dumps({'status':audit['status'],'archive':audit['archive'],'receipt_files':len(receipts)}))
