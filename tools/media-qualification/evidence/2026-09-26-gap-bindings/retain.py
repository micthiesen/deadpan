"""Retain Repeat-gap verification without bundling binaries or scratch media."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-gap-bindings-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-gap-bindings'
gate_name = sys.argv[1]
gate = json.loads((scratch / gate_name / 'report.json').read_text())
assert len(gate['commands']) == 5, 'complete gate required'
review = json.loads((scratch / 'review.json').read_text())
assert len(review['reviewers']) == 3
backend_changes = [path for path in gate['changed_sources']
                   if not (path.startswith('crates/deadpan-app/') or path == 'Cargo.lock')]
assert not backend_changes, backend_changes
out.mkdir(parents=True, exist_ok=False)
for name in ['pcm-cli.py', 'gate.py', 'retain.py', 'review.json', 'legacy-gap-refusal.py', 'post-format.py', 'playback-rerun.py', 'playback-exact.py']:
    shutil.copy2(scratch / name, out / name)
for name in ['plan-1', 'pcm-cli-1', 'gate-1', 'gate-2', 'gate-3', 'gate-4', gate_name, 'post-format', 'playback-rerun', 'playback-exact']:
    shutil.copytree(scratch / name, out / name)
for directory, names in {
    'core': ['build-initial.log', 'focused-1.log', 'focused-final.log', 'focused-rerun.log'],
    'migration': ['core-final.log', 'store-gap-initial.log', 'store-final.log', 'store-final-2.log', 'corrections.md'],
    'pcm': ['run-1.log', 'birth-existing-binary.log', 'birth-backtrace.log'],
}.items():
    (out / directory).mkdir()
    for name in names:
        shutil.copy2(scratch / directory / name, out / directory / name)
(out / 'old-binary').mkdir()
shutil.copy2(scratch / 'migration/producer-2/commands.json', out / 'old-binary/commands.json')
shutil.copy2(scratch / 'legacy-gap-refusal/commands.json', out / 'old-binary/gap-refusal-commands.json')
provenance = json.loads((repo / 'crates/deadpan-store/tests/fixtures/v27-audio-reanchor-history.provenance.json').read_text())
assert hashlib.sha256((out / 'old-binary/commands.json').read_bytes()).hexdigest() == provenance['producer_log_sha256']
verification = {
    'base_revision': subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'committed': False, 'pushed': False, 'git_write_access': 'read-only in this session',
    'environment': {'os': 'macOS 26.5.2', 'build': '25F84', 'architecture': 'arm64', 'rust': '1.97.1 (8bab26f4f 2026-07-14)', 'ffmpeg_prefix': '/tmp/deadpan-media-compatible-xyhilms4/prefix'},
    'schemas': {'core': 22, 'database': 28, 'audio_context': 2},
    'gate_directory': gate_name,
    'gate': gate,
    'subsequent_formatting': json.loads((scratch / 'post-format/report.json').read_text()),
    'playback_package_retry': json.loads((scratch / 'playback-rerun/report.json').read_text()),
    'playback_exact_workspace_artifact_retry': json.loads((scratch / 'playback-exact/report.json').read_text()),
    'gap_backend_unchanged_during_gate': True,
    'concurrent_source_changes_during_gate': gate['changed_sources'],
    'focused_completed_cases': {'plan': 55, 'frozen_adapter': 12, 'decoded_pcm': 5, 'cli_audio_inspection': 14, 'cli_project_commands': 16},
    'incomplete_focused_runs': ['core/focused-rerun.log and migration/store-final-2.log contain no final test result; final workspace run supplies complete coverage'],
    'corrections': [
        'Initial compilation saw the registered gaps test module before its file existed; the file was subsequently completed.',
        'InsertTime legacy test still expected capture of an unaffected positive-gap prefix to fail. It now asserts success and retains rejection of shifted unsupported structures.',
        'Migration fixture gained a legacy Repeat while its test still wrapped another Repeat; the test now captures the existing old-gap-owner.',
        'The signed bound-definition PCM oracle used unbound fractional phase. It now pins phase zero at the retained allocated anchor, with origin -1/3 and nonunity scale 3/2. Production timing was unchanged.',
        'Concurrent optional UI harness manifest/lock mismatch interrupted a locked test before execution. Cargo metadata resolved existing pins offline; later checks remained locked.',
        'Initial full gate found rustfmt differences in migration module ordering and the concurrent UI harness; formatting was applied without behavior changes.',
        'All-target compilation found omitted recipe and gap_after fields in an older persistence test literal; it now explicitly uses the existing node/default recipe with no gap argument.',
        'Concurrent Dialogs test initializers used a struct update with no remaining fields when ui-harness was disabled. Explicit cfg-gated scripted: None preserves both feature configurations without suppressing Clippy.',
    ],
    'old_binary': {'sha256': provenance['binary_sha256'], 'core': 21, 'database': 27, 'fixture_revisions': 9, 'fixture_history': 5, 'initial_seed': 'Explicit reanchor intent seeded only in the initial no-history snapshot and validated by the old binary; all later history was command-generated'},
    'gui_review': 'Not repeated for this timing/history increment. Concurrent UI harness work is preserved and has independent verification scope.',
    'optional_ui_harness_qualified_by_gate': False,
    'new_imagegen': False,
    'remaining_work': ['Complete movement/raw-recipe binding lifecycle', 'General atomic Original-moment splice', 'Native Visual/register workflow', 'Full normative product and release gates'],
}
(out / 'verification.json').write_text(json.dumps(verification, indent=2) + '\n')
manifest = {str(path.relative_to(out)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(out.rglob('*')) if path.is_file()}
(out / 'sha256.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'directory': str(out), 'files': len(manifest), 'gate': gate}))
