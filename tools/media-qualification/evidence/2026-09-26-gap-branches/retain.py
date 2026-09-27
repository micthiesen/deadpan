"""Retain bounded logs and provenance for the gap-branch increment."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-gap-branches-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-gap-branches'
gate_name = sys.argv[1]
gate = json.loads((scratch / gate_name / 'report.json').read_text())
assert len(gate['commands']) == 7, 'complete workspace and optional-feature gate required'
assert gate['source_unchanged'], gate['changed_sources']
post = json.loads((scratch / 'post-validation/report.json').read_text())
assert post['passed'] and post['production_sources_unchanged']
assert post['changed_since_full_gate'] == ['crates/deadpan-cli/tests/project_commands.rs']
for path, digest in json.loads((scratch / 'post-validation/source-after.json').read_text()).items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
reviews = json.loads((scratch / 'review.json').read_text())
assert not reviews['remaining_findings']
cli = json.loads((scratch / 'durable-cli-final/report.json').read_text())
assert cli['passed']
out.mkdir(exist_ok=False)
for name in ['README.md', 'gate.py', 'retain.py', 'durable-cli.py', 'review.json', 'post-validation.py']:
    shutil.copy2(scratch / name, out / name)
for gate_dir in sorted(scratch.glob('gate-*')):
    if gate_dir.is_dir() and (gate_dir / 'report.json').exists():
        shutil.copytree(gate_dir, out / gate_dir.name)
shutil.copytree(scratch / 'post-validation', out / 'post-validation')
for name in ['durable-cli-1', 'durable-cli-final', 'playback-exact']:
    source = scratch / name
    if source.is_dir():
        (out / name).mkdir()
        for path in source.iterdir():
            if path.is_file() and path.suffix in ('.json', '.log'):
                shutil.copy2(path, out / name / path.name)
(out / 'old-binary').mkdir()
for label in ['gap-branch', 'gap-binding']:
    fixture = repo / f'crates/deadpan-store/tests/fixtures/v28-{label}-history'
    provenance = json.loads(fixture.with_suffix('.provenance.json').read_text())
    assert hashlib.sha256(fixture.with_suffix('.sql').read_bytes()).hexdigest() == provenance['sql_sha256']
    producer = fixture.parent / provenance['producer_script']
    assert hashlib.sha256(producer.read_bytes()).hexdigest() == provenance['producer_script_sha256']
    commands = Path(provenance['producer_log'])
    assert hashlib.sha256(commands.read_bytes()).hexdigest() == provenance['producer_log_sha256']
    shutil.copy2(commands, out / 'old-binary' / f'{label}-commands.json')
report = dict(
    base_revision=subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    committed=False, pushed=False, git_metadata_access='read-only in this session',
    environment=dict(os='macOS 26.5.2', build='25F84', architecture='arm64', rust='1.97.1',
                     ffmpeg_prefix='/tmp/deadpan-ui-ffmpeg/prefix'),
    core_schema=23, database_schema=29, audio_context_schema=3,
    gate=gate, schema_expectation_followup=post, durable_cli=cli,
    qualification='docs/qualification/gap-branches-2026-09-26.md',
    ui_evidence='tools/ui-feedback/evidence/2026-09-26/summary.json',
    ui_scope='Feature lint/tests in this gate; earlier Metal visual/performance results retain their separate build identities',
)
(out / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
files = {str(path.relative_to(out)): hashlib.sha256(path.read_bytes()).hexdigest()
         for path in out.rglob('*') if path.is_file()}
(out / 'sha256.json').write_text(json.dumps(files, indent=2) + '\n')
print(json.dumps(dict(evidence=str(out), files=len(files), gate_passed=gate['passed'],
                      workspace_tests=gate.get('tests'), source_count=gate['source_count'])))
