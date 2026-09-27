"""Retain only reproducible test/review evidence, not scratch media or binaries."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-source-range-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-source-moments'
out.mkdir(parents=True, exist_ok=False)
for name in ['gate.py', 'focused.py', 'focused-2.py', 'retain.py']:
    shutil.copy2(scratch / name, out / name)
for name in ['gate-1', 'gate-2', 'focused-1', 'focused-2', 'render-focused']:
    assert (scratch / name).is_dir(), name
    shutil.copytree(scratch / name, out / name)
(out / 'old-binary').mkdir()
for name in ['source-selection-commands.json']:
    shutil.copy2(scratch / 'old-binary' / name, out / 'old-binary' / name)
review = {
    'workflow': 'check', 'base': 'origin/main',
    'subject': 'Exact Original moment candidates and selected audio placements only; prior captured framing retained as baseline',
    'reviewers': [
        {'agent': 'source_selection_general_review', 'scope': 'core/media/context host correctness and tests', 'findings': []},
        {'agent': 'source_selection_migration_review', 'scope': 'strict legacy grammars, migration and history', 'findings': []},
        {'agent': 'source_selection_audio_review', 'scope': 'sample phase, physical endpoints, transfer, Preserve and bindings', 'findings': []},
    ],
    'root_additions': [
        'Closed audio-context schema1 input and version-independent exact historical content authentication',
        'Captured-framing aggregate preflight before frozen core19 snapshot materialization',
        'Outside-host/zero-sample selections and current-window changes after timing capture',
    ],
    'remaining_findings': [],
}
(out / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
gate = json.loads((out / 'gate-2/report.json').read_text())
verification = {
    'base_revision': subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'committed': False, 'pushed': False, 'git_write_access': 'read-only in this session',
    'environment': {'os': 'macOS 26.5.2', 'build': '25F84', 'architecture': 'arm64',
                    'rust': '1.97.1 (8bab26f4f 2026-07-14)',
                    'ffmpeg_prefix': '/tmp/deadpan-media-compatible-xyhilms4/prefix',
                    'hardware_query': 'sysctl denied by sandbox'},
    'gate': gate,
    'gui_review': 'Not run: this increment changes no native controls/startup/picture rendering',
    'native_picture_limitations': 'Prior captured-framing Metal/native limitations remain documented separately',
    'remaining_work': ['Native Visual selection', 'Registers and named moments', 'Atomic selected-moment splice with retained sample resume', 'Full normative product and release gates'],
}
(out / 'verification.json').write_text(json.dumps(verification, indent=2) + '\n')
manifest = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'directory': str(out), 'files': len(manifest), 'gate': gate}))
