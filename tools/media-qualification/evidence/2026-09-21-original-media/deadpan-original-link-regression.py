import json
import os
import pathlib
import shutil
import subprocess

repo = pathlib.Path('/Users/michael/Code/deadpan')
work = pathlib.Path(pathlib.Path('/tmp/deadpan-original-cleanup-regression-current.txt').read_text())
checkout = work / 'before-fix'
path = checkout / 'crates/deadpan-store/src/original_media.rs'
source = (repo / 'crates/deadpan-store/src/original_media.rs').read_text()
old = '    if !same_source_state(inspected, &named) || !same_source_state(inspected, &original) {'
assert source.count(old) == 1
source = source.replace(old, '    if named.dev() != original.dev() || named.ino() != original.ino() {')
path.write_text(source)
command = ['cargo', 'test', '-p', 'deadpan-store', '--lib', 'final_link_confirmation', '--locked']
env = dict(os.environ, CARGO_TARGET_DIR=str(work / 'target'))
with (work / 'linked-before.log').open('w') as log:
    result = subprocess.run(command, cwd=checkout, env=env, stdout=log, stderr=subprocess.STDOUT)
text = (work / 'linked-before.log').read_text()
assert result.returncode != 0 and 'final_link_confirmation_rejects_same_inode_changes_after_hashing ... FAILED' in text, text[-6000:]
(work / 'linked-report.json').write_text(json.dumps({
    'scope': 'Isolated copy with final path check reverted to its old inode-only predicate; shared checkout never changed',
    'command': command, 'before_fix_exit': result.returncode,
    'failure': 'same-inode/same-length content mutation passed the old final path check',
}, indent=2) + '\n')
print(json.dumps({'work': str(work), 'before_fix_exit': result.returncode}))
