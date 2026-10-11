from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tarfile

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
baseline = root / 'baseline'
revision = 'afacc963ad5b1044c8be12146987919c94ad366c'
baseline.mkdir()
archive = root / 'baseline.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', revision], cwd=repo, stdout=output, check=True)
with tarfile.open(archive) as tar:
    tar.extractall(baseline, filter='data')
test = 'crates/deadpan-core/tests/retime_edits.rs'
shutil.copy2(repo / test, baseline / test)
target = '/tmp/deadpan-group-mix-20261011/old-target'
with archive.open('rb') as stream:
    archive_hash = hashlib.file_digest(stream, 'sha256').hexdigest()
(root / 'baseline-source.json').write_text(json.dumps({
    'revision': revision, 'archive_sha256': archive_hash,
    'test_overlays': {test: hashlib.sha256((baseline / test).read_bytes()).hexdigest()},
    'target': target,
}, indent=2) + '\n')
result = subprocess.run(['cargo', 'test', '--locked', '-p', 'deadpan-core', '--test',
    'retime_edits', 'disk_prepared_preserve_accepts_four_minutes_of_output_with_undo', '--', '--exact'],
    cwd=baseline, env=dict(os.environ, CARGO_TARGET_DIR=target))
raise SystemExit(result.returncode)
