"""Retry observed playback timeout cases with the same workspace binary."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-routed-voices-20260927')
gates = [p for p in sorted(scratch.glob('gate-*'))
         if (p / 'report.json').exists()
         and len(json.loads((p / 'report.json').read_text())['commands']) == 7]
assert gates
log = (gates[-1] / '2.log').read_text()
matches = re.findall(r'Running unittests src/lib.rs \((target/debug/deps/deadpan_playback-[^)]+)\)', log)
assert len(matches) == 1, matches
known = [
    'tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock',
    'tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock',
    'tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm',
    'tests::seek_reuses_private_pcm_but_a_new_session_must_reopen_sources',
]
tests = [name for name in known if f'test {name} ... FAILED' in log]
if not tests:
    print('No observed known playback timeout to retry')
    raise SystemExit(0)
binary = repo / matches[0]
before = hashlib.sha256(binary.read_bytes()).hexdigest()
commands = [[binary.name, '--exact', name, '--test-threads=1'] for name in tests]
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ['PATH'], RUST_BACKTRACE='1')
result = subprocess.run(['python3', str(scratch / 'run-checks.py'), str(scratch / 'playback-isolated-01'), json.dumps(commands)], env=environment, check=False)
after = hashlib.sha256(binary.read_bytes()).hexdigest()
(scratch / 'playback-isolated-01/binary.json').write_text(json.dumps(dict(path=str(binary), sha256_before=before, sha256_after=after, unchanged=before == after,
    scope='Only observed playback timeout cases. The seek-cache test is an additional failure in this run; isolated passes do not erase parallel failures or establish their cause.'), indent=2) + '\n')
assert before == after
result.check_returncode()
