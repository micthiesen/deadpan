"""Retain a successful authored-EQ run and its real emitted file."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import tarfile

parser = argparse.ArgumentParser()
parser.add_argument('run_directory', type=Path)
parser.add_argument('destination', type=Path)
args = parser.parse_args()
source = args.run_directory
out = args.destination
records = json.loads((source/'commands.json').read_text())
assert [r['name'] for r in records] == ['fmt', 'tests', 'clippy', 'whitespace']
assert all(r['exit_code'] == 0 for r in records)
assert json.loads((source/'release-command.json').read_text())['exit_code'] == 0
report = json.loads((source/'export.json').read_text())
assert report['report']['passed'] and not report['report']['failures']
out.mkdir(parents=True, exist_ok=True)
for name in ['run.py', 'release.py', 'retain.py', 'check-evidence.py', 'review.md',
             'commands.json', 'identity.json', 'release-command.json',
             'release-identity.json', 'export.json']:
    shutil.copyfile(source/name, out/name)
for name in ['fmt', 'tests', 'clippy', 'whitespace', 'release', 'initial-tests',
             'schema-update-tests', 'review-tests', 'review-fixed', 'review-clippy']:
    path = source/(name+'.log')
    (out/(name+'.log.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
movie = Path(report['movie'])
shutil.copyfile(movie, out/'equalizer.mp4')
receipts = list(movie.parent.glob('deadpan-render-*.json'))
assert len(receipts) == 1
shutil.copyfile(receipts[0], out/'render-receipt.json')
package = Path(report['package'])
wal = package/'project.sqlite-wal'
assert not wal.exists() or wal.stat().st_size == 0
stream = io.BytesIO()
with tarfile.open(fileobj=stream, mode='w') as archive:
    for path in sorted(package.rglob('*')):
        if path.is_file() and not path.name.endswith(('-shm', '-wal', '.lock')):
            item = archive.gettarinfo(str(path), arcname='equalizer.deadpan/'+str(path.relative_to(package)))
            item.uid = item.gid = item.mtime = 0
            item.uname = item.gname = ''
            with path.open('rb') as data:
                archive.addfile(item, data)
(out/'equalizer.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(),mtime=0))
log = (source/'tests.log').read_text()
result = dict(passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log))),
              release_tests=1, pictures=report['report']['summary']['pictures_checked'],
              audio_blocks=report['report']['summary']['blocks_compared'])
(out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
manifest = {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.iterdir())
            if p.is_file() and p.name != 'manifest.json'}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(result))
