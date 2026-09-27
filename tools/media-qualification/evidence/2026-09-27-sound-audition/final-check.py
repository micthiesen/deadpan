"""Seal final source after the workspace gate and recheck the late UI label's formatting."""
from pathlib import Path
import hashlib, json, os, subprocess, time

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-authored-sounds-20260927')
gate = scratch / 'gate-01'
out = scratch / 'final-source-check'
report = json.loads((gate / 'report.json').read_text())
assert len(report['commands']) == 7
assert report['changed_sources'] == ['crates/deadpan-app/src/preview/playback.rs']
# The only mid-run edit was the selected-sound label. Workspace tests/build and
# both feature lint/tests ran after that edit; a second full gate adds no coverage.
assert all(report['commands'][i]['exit_code'] == 0 for i in [0, 1, 3, 4, 5, 6])
expected = json.loads((gate / 'source-after.json').read_text())

def seal():
    paths = subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=repo).decode().split('\0')
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in sorted(set(paths)) if p and (p.startswith(('crates/', 'native/')) or p in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml')) and (repo / p).is_file()}

before = seal()
assert before == expected
out.mkdir(exist_ok=False)
(out / 'source-before.json').write_text(json.dumps(before, indent=2) + '\n')
command = ['cargo', 'fmt', '--all', '--', '--check']
started = time.monotonic()
with (out / '0.log').open('w') as log:
    result = subprocess.run(command, cwd=repo, env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'), stdout=log, stderr=subprocess.STDOUT)
after = seal()
(out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
result_report = dict(commands=[dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-started, log='0.log')], source_count=len(after), source_unchanged=before==after, changed_sources=[p for p in sorted(set(before)|set(after)) if before.get(p)!=after.get(p)], inherited_gate='gate-01', scope='Final format recheck and source seal; workspace and feature results remain in gate-01, including any test failures')
(out / 'report.json').write_text(json.dumps(result_report, indent=2) + '\n')
print(json.dumps(result_report))
assert result.returncode == 0 and before == after
