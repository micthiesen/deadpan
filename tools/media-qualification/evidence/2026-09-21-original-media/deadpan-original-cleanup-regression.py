import json
import os
import pathlib
import shutil
import subprocess
import tempfile

repo = pathlib.Path('/Users/michael/Code/deadpan')
work = pathlib.Path(tempfile.mkdtemp(prefix='deadpan-original-cleanup-regression-'))
pathlib.Path('/tmp/deadpan-original-cleanup-regression-current.txt').write_text(str(work))
checkout = work / 'before-fix'
checkout.mkdir()
for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml']:
    if (repo / name).is_file():
        shutil.copyfile(repo / name, checkout / name)
for name in ['crates', 'native', '.cargo']:
    if (repo / name).is_dir():
        shutil.copytree(repo / name, checkout / name)
path = checkout / 'crates/deadpan-store/src/object_storage.rs'
source = path.read_text()
start = source.index('            // Capture the new namespace entry before opening it,')
end = source.index('            let cloned = openat(', start)
guard_start = source.index('            let mut pending = PendingObject {', start)
guard_end = source.index('            self.inner.validate_contained', guard_start)
guard = source[guard_start:guard_end]
source = source[:start] + source[end:]
start = source.index('            let cloned = openat(', source.index('    fn try_clone_file_with('))
end = source.index('            let cloned = File::from(cloned);', start)
section = source[start:end]
for line in ['                || i128::from(metadata.st_dev) != pending.device\n',
             '                || i128::from(metadata.st_ino) != pending.inode\n']:
    assert line in section
    section = section.replace(line, '')
source = source[:start] + section + guard + source[end:]
path.write_text(source)
command = ['cargo', 'test', '-p', 'deadpan-store', '--lib', 'clone_validation_failure', '--locked']
env = dict(os.environ, CARGO_TARGET_DIR=str(work / 'target'))
with (work / 'before.log').open('w') as log:
    result = subprocess.run(command, cwd=checkout, env=env, stdout=log, stderr=subprocess.STDOUT)
text = (work / 'before.log').read_text()
assert result.returncode != 0 and 'assertion failed: originals_pending_entries(package.path()).is_empty()' in text, text[-6000:]
(work / 'report.json').write_text(json.dumps({
    'scope': 'Isolated source copy with only cleanup-guard fix reversed; shared checkout never changed',
    'command': command, 'before_fix_exit': result.returncode,
    'failure': 'assertion failed: originals_pending_entries(package.path()).is_empty()',
    'regression': 'Actual APFS clone gains a second link before validation; rejected pending name must be removed while other name is preserved',
}, indent=2) + '\n')
print(json.dumps({'work': str(work), 'before_fix_exit': result.returncode}))
