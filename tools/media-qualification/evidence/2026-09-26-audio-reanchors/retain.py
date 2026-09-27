"""Retain checks and review evidence; binaries and scratch media stay outside Git."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-reanchors-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-audio-reanchors'
gate = json.loads((scratch / 'gate-1/report.json').read_text())
assert len(gate['commands']) == 5, 'complete gate required'
out.mkdir(parents=True, exist_ok=False)
for name in ['focused.py', 'pcm-cli.py', 'gate.py', 'retain.py']:
    shutil.copy2(scratch / name, out / name)
for name in ['focused-1', 'pcm-cli-1', 'pcm-cli-2', 'pcm-cli-3', 'gate-1']:
    shutil.copytree(scratch / name, out / name)
(out / 'old-binary').mkdir()
shutil.copy2(scratch / 'fixture-final/commands.json', out / 'old-binary/commands.json')
provenance = json.loads((repo / 'crates/deadpan-store/tests/fixtures/v26-audio-binding-history.provenance.json').read_text())
assert hashlib.sha256((out / 'old-binary/commands.json').read_bytes()).hexdigest() == provenance['producer_log_sha256']
review = {
    'workflow': 'check', 'base': 'origin/main',
    'subject': 'core21/database27 compact audio reanchor steps, allocation queries, lifecycle and history; earlier dirty increments retained as baseline',
    'reviewers': [
        {'agent': 'reanchor_general_review', 'scope': 'correctness, errors, admission, compactness and tests', 'findings': []},
        {'agent': 'reanchor_migration_review', 'scope': 'closed legacy vocabulary, complete chronological migration and identity retention', 'findings': []},
        {'agent': 'reanchor_timing_review', 'scope': 'exact clocks, raw support, birth scopes and independent decoded-PCM oracle', 'findings': []},
    ],
    'followup': 'Timing reviewer independently verified corrected Preserve source support [100,218) and original full-output comparison',
    'remaining_findings': [],
}
(out / 'review.json').write_text(json.dumps(review, indent=2)+'\n')
verification = {
    'base_revision': subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'committed': False, 'pushed': False, 'git_write_access': 'read-only in this session',
    'environment': {'os': 'macOS 26.5.2', 'build': '25F84', 'architecture': 'arm64', 'rust': '1.97.1 (8bab26f4f 2026-07-14)', 'ffmpeg_prefix': '/tmp/deadpan-media-compatible-xyhilms4/prefix'},
    'schemas': {'core': 21, 'database': 27, 'audio_context': 2},
    'gate': gate,
    'focused_unique_final_cases': {'core': 129, 'store': 91, 'pcm': 6, 'cli': 16},
    'unlogged_initial_compile_failure': {'error': 'E0583 missing module reanchors', 'file': 'crates/deadpan-core/tests/audio_bindings.rs:5', 'correction': 'explicit path attribute to audio_bindings/reanchors.rs', 'warning': 'unused super import in frozen binding tests, removed'},
    'pcm_fixture_corrections': ['Oracle must constrain filter context at the Source host end: ceil(100+128*147/160)=218', 'Full original 384-sample comparison requires two reads within the 256-sample inspection limit'],
    'old_binary': {'sha256': provenance['binary_sha256'], 'core': 20, 'database': 26, 'fixture_revisions': 27, 'fixture_history': 15},
    'gui_review': 'Not repeated: no native UI, startup, lifecycle or picture changes in this increment',
    'new_imagegen': False,
    'remaining_work': ['Authored Repeat-gap binding ownership and birth', 'Complete movement/raw-recipe binding lifecycle', 'General atomic Original-moment splice', 'Native Visual/register workflow', 'Full normative product and release gates'],
}
(out / 'verification.json').write_text(json.dumps(verification, indent=2)+'\n')
manifest = {str(path.relative_to(out)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(out.rglob('*')) if path.is_file()}
(out / 'sha256.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(json.dumps({'directory': str(out), 'files': len(manifest), 'gate': gate}))
