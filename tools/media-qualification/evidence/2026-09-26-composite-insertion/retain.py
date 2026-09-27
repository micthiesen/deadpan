"""Retain command, review, runtime and gate evidence without altering results."""
import hashlib
import gzip
import json
from pathlib import Path
import shutil
import subprocess
import sys

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-composite-splice-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-composite-insertion'
gate_name = sys.argv[1]
gate = json.loads((scratch / gate_name / 'report.json').read_text())
assert len(gate['commands']) == 7
assert gate['source_unchanged'], gate['changed_sources']
post = json.loads((scratch / 'post-validation/report.json').read_text())
assert post['passed'] and post['production_sources_unchanged']
assert post['changed_since_full_gate'] == ['crates/deadpan-core/tests/audio_bindings/gaps.rs']
for path, digest in json.loads((scratch / 'post-validation/source-after.json').read_text()).items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
reviews = json.loads((scratch / 'review.json').read_text())
assert reviews['complete'] and len(reviews['reviews']) == 3
assert all(not review['findings'] for review in reviews['reviews'])
cli = json.loads((scratch / 'durable-app-1/report.json').read_text())
assert cli['passed']
ui = json.loads((scratch / 'ui-editing-1/report.json').read_text())
out.mkdir(exist_ok=False)
for name in ['README.md', 'gate.py', 'retain.py', 'post-validation.py', 'durable-cli.py', 'review.json', 'baseline.json', 'current-binary.json',
             'analyze-preview-latency.py', 'paired-preview-latency.json',
             'wait_probe.rs', 'wait-probe.jsonl', 'wait-probe-context.json',
             'main-focused-1.log', 'native-focused-1.log', 'migration-tests.log', 'ui-editing-1.log']:
    shutil.copy2(scratch / name, out / name)
analysis_source = Path('/tmp/deadpan-ui-performance-02/report.json').read_bytes()
assert hashlib.sha256(analysis_source).hexdigest() == '95a4994a039c79912b16e2f174872507502fd9cf2f6ed0bbf4df42101426079f'
(out / 'prior-host-performance-report.json.gz').write_bytes(gzip.compress(analysis_source, mtime=0))
for directory in sorted(scratch.glob('gate-*')):
    if directory.is_dir() and (directory / 'report.json').exists():
        shutil.copytree(directory, out / directory.name)
shutil.copytree(scratch / 'post-validation', out / 'post-validation')
for name in ['durable-app-1', 'ui-editing-1', 'pcm']:
    (out / name).mkdir()
    for path in (scratch / name).iterdir():
        if path.is_file() and path.suffix in ('.json', '.log', '.html'):
            shutil.copy2(path, out / name / path.name)
fixture = repo / 'crates/deadpan-store/tests/fixtures/v29-composite-insert-history'
provenance = json.loads(fixture.with_suffix('.provenance.json').read_text())
for path, expected in [
    (fixture.with_suffix('.sql'), provenance['sql_sha256']),
    (fixture.parent / provenance['producer_script'], provenance['producer_script_sha256']),
    (fixture.parent / provenance['producer_log'], provenance['producer_log_sha256']),
]:
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, path
report = dict(
    base_revision=subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    committed=False, pushed=False, git_metadata_access='read-only in this session',
    environment=dict(os='macOS 26.5.2', build='25F84', architecture='arm64', rust='1.97.1',
                     ffmpeg_prefix='/tmp/deadpan-ui-ffmpeg/prefix'),
    core_schema=24, database_schema=30, audio_context_schema=3,
    gate=gate, expectation_followup=post, durable_headless=cli, reviews=reviews,
    visual=dict(failed=ui['failed'], binary_sha256=ui['metadata']['binary_sha256'],
                scenarios=[{key: scenario[key] for key in ['name','failed','findings']}
                           for scenario in ui['scenarios']]),
    qualification='docs/qualification/composite-insertion-2026-09-26.md',
    earlier_host_ui_evidence='tools/ui-feedback/evidence/2026-09-26/summary.json',
    performance='Not rerun: the required Metal adapter is unavailable; prior host timing does not cover added composite pause replay',
    native_lifecycle='Not repeated: startup/shutdown unchanged',
)
(out / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
files = {str(path.relative_to(out)): hashlib.sha256(path.read_bytes()).hexdigest()
         for path in out.rglob('*') if path.is_file()}
(out / 'sha256.json').write_text(json.dumps(files, indent=2) + '\n')
print(json.dumps(dict(evidence=str(out), files=len(files), gate_passed=gate['passed'],
                      workspace_tests=gate.get('tests'), source_count=gate['source_count'])))
