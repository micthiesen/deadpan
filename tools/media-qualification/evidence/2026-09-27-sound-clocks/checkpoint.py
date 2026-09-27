import hashlib
import json
import subprocess
import tarfile
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-sound-placement-20260927')
out = scratch / 'checkpoint'
out.mkdir(exist_ok=False)


def git(*args):
    return subprocess.check_output(['git', '-c', 'core.fsmonitor=false', *args], cwd=repo)


def sha(data):
    return hashlib.sha256(data).hexdigest()


expected = json.loads((scratch / 'final-source.json').read_text())
assert all(sha((repo / p).read_bytes()) == h for p, h in expected.items())
patch = git('diff', '--binary', 'HEAD')
(out / 'tracked.patch').write_bytes(patch)
files = sorted(p.decode() for p in git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0') if p)
hashes = {p: sha((repo / p).read_bytes()) for p in files}
with tarfile.open(out / 'untracked.tar.gz', 'w:gz') as tar:
    for p in files:
        tar.add(repo / p, arcname=p, recursive=False)
with tarfile.open(out / 'untracked.tar.gz', 'r:gz') as tar:
    assert {p.name: sha(tar.extractfile(p).read()) for p in tar.getmembers()} == hashes
assert hashes == {p: sha((repo / p).read_bytes()) for p in files}
assert git('diff', '--binary', 'HEAD') == patch
assert sorted(p.decode() for p in git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0') if p) == files
design = json.loads((repo / 'docs/design/manifest.json').read_text())
for row in design['images']:
    assert sha((repo / 'docs/design' / row['image']).read_bytes()) == row['sha256']
    assert sha((repo / 'docs/design' / row['prompt']).read_bytes()) == row['prompt_sha256']
evidence = repo / 'tools/media-qualification/evidence/2026-09-27-sound-clocks'
for name, digest in json.loads((evidence / 'sha256.json').read_text()).items():
    assert sha((evidence / name).read_bytes()) == digest
previous = Path('/tmp/deadpan-authored-sounds-20260927/checkpoint')
previous_files = json.loads((previous / 'manifest.json').read_text())['untracked_files']
assert set(previous_files) <= set(files)
record = dict(base_revision=git('rev-parse', 'HEAD').decode().strip(),
    tracked_patch_sha256=sha(patch), untracked_archive_sha256=sha((out / 'untracked.tar.gz').read_bytes()),
    untracked_files=hashes, source_count=len(expected), sources_match_final_gate=True,
    imagegen_boards_and_prompts_verified=len(design['images']), previous_checkpoint=str(previous),
    previous_untracked_paths_removed=[], git_write_access='read-only; no commit or push',
    harness_included=True, evidence_hashes_verified=True, archive_contents_verified=True)
(out / 'manifest.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({k: v for k, v in record.items() if k != 'untracked_files'}))
print('untracked_file_count', len(files))
