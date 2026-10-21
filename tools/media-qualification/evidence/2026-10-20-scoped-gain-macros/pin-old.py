from pathlib import Path
import hashlib,json,stat,shutil
root=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-scoped-gain-2026-10-20')
assert (scratch/'old-witness.exit').read_text().strip()=='100'
log=(scratch/'old-witness.log').read_text()
assert 'single-play macro authority supports only Zoom' in log,log[-3000:]
assert '1 test run: 0 passed, 1 failed' in log,log[-3000:]
files=[p for p in (root/'target/debug/deps').glob('deadpan_core-*') if p.is_file() and p.stat().st_mode & stat.S_IXUSR]
files.sort(key=lambda p:p.stat().st_mtime,reverse=True)
binary=files[0]
assert binary.stat().st_mtime >= (scratch/'old-witness.sh').stat().st_mtime
assert not files[1:] or binary.stat().st_mtime > files[1].stat().st_mtime
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=['Cargo.lock','crates/deadpan-core/src/semantic/planner.rs','crates/deadpan-core/src/semantic/planner/pause.rs','crates/deadpan-core/src/semantic/planner/tests/scoped_framing.rs','crates/deadpan-core/src/semantic/planner/tests/scoped_gain.rs']
ids={'base':'c111e8bf','sha256':{p:sha(root/p) for p in paths},'binary':str(binary.relative_to(root)),'binary_sha256':sha(binary),'selection':'newest executable deadpan_core-* built by sole Cargo owner for the --lib run'}
assert ids['sha256'][paths[1]]=='b518578d6b9cea7f4351f6a7ecc34ee3badaf6efc39ba817d0b71e4df6d373c8'
assert ids['sha256'][paths[2]]=='7d88d645e4a417f90884852831530c305a78668df4760829f30c832acdaab0fc'
assert ids['sha256'][paths[4]]==sha(scratch/'old-witness/scoped_gain.rs')
(scratch/'old-identities.json').write_text(json.dumps(ids,indent=2)+'\n')
shutil.copy2(binary,scratch/'old-core-test')
print(json.dumps(ids,indent=2))
