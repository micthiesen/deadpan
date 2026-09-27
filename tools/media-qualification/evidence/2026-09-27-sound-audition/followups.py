"""Retry the observed playback timeouts in isolation with the unchanged test binary."""
from pathlib import Path
import hashlib, json, subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-authored-sounds-20260927')
gate = json.loads((scratch / 'gate-01/report.json').read_text())
assert len(gate['commands']) == 7
binary = repo / 'target/debug/deps/deadpan_playback-34f16a24a1d54369'
before = hashlib.sha256(binary.read_bytes()).hexdigest()
tests = [
    'tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock',
    'tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock',
    'tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm',
]
commands = [[str(binary), '--exact', name, '--test-threads=1'] for name in tests]
result = subprocess.run(['python3', str(scratch / 'run-checks.py'), str(scratch / 'playback-isolated-01'), json.dumps(commands)], check=False)
after = hashlib.sha256(binary.read_bytes()).hexdigest()
(scratch / 'playback-isolated-01/binary.json').write_text(json.dumps(dict(path=str(binary), sha256_before=before, sha256_after=after, unchanged=before==after, scope='Only the three workspace timeout cases; an isolated pass does not erase the parallel failures'), indent=2) + '\n')
assert before == after
result.check_returncode()
subprocess.run(['python3', str(scratch / 'ui-checks.py')], check=True)
