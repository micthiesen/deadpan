"""Recover a missing documentation baseline from the verified prior checkpoint.

Only a disposable directory is patched; the shared checkout is never restored.
"""
from pathlib import Path
import hashlib, json, shutil, subprocess

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
prior=Path('/tmp/deadpan-nested-sequence-20260926/checkpoint')
manifest=json.loads((prior/'manifest.json').read_text())
patch=prior/'tracked.patch'
assert hashlib.sha256(patch.read_bytes()).hexdigest()==manifest['tracked_patch_sha256']
copy=scratch/'readme-copy'
copy.mkdir(exist_ok=False)
original=subprocess.check_output(['git','-c','core.fsmonitor=false','show',manifest['base_revision']+':README.md'],cwd=repo)
(copy/'README.md').write_bytes(original)
subprocess.run(['git','-c','core.fsmonitor=false','apply','--include=README.md',str(patch)],cwd=copy,check=True)
shutil.copy2(copy/'README.md',scratch/'before/README.md')
(scratch/'readme-baseline.json').write_text(json.dumps(dict(
    checkpoint=str(prior),patch_sha256=manifest['tracked_patch_sha256'],
    readme_sha256=hashlib.sha256((copy/'README.md').read_bytes()).hexdigest()),indent=2)+'\n')
