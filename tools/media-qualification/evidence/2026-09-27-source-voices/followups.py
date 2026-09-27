"""Retry only observed playback timeout cases with the unchanged workspace binary."""
from pathlib import Path
import hashlib, json, os, re, subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-source-voices-20260927')
gate = json.loads((scratch / 'gate-01/report.json').read_text())
assert len(gate['commands']) == 7
log = (scratch / 'gate-01/2.log').read_text()
matches = re.findall(r'Running unittests src/lib.rs \((target/debug/deps/deadpan_playback-[^)]+)\)', log)
assert len(matches) == 1, matches
binary = repo / matches[0]
before = hashlib.sha256(binary.read_bytes()).hexdigest()
tests = [
    'tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock',
    'tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock',
    'tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm',
]
assert all(f'test {name} ... FAILED' in log for name in tests)
commands = [[binary.name, '--exact', name, '--test-threads=1'] for name in tests]
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ['PATH'], RUST_BACKTRACE='1')
result = subprocess.run(['python3', str(scratch / 'run-checks.py'), str(scratch / 'playback-isolated-01'), json.dumps(commands)], env=environment, check=False)
after = hashlib.sha256(binary.read_bytes()).hexdigest()
(scratch / 'playback-isolated-01/binary.json').write_text(json.dumps(dict(path=str(binary), sha256_before=before, sha256_after=after, unchanged=before == after,
    scope='Only the three workspace timeout cases. Isolated passes do not erase parallel failures or establish their cause.'), indent=2) + '\n')
assert before == after
result.check_returncode()
