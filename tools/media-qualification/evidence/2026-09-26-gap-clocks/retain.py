"""Retain complete verification records without copying scratch media."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-gap-clocks-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-gap-clocks'
gate = json.loads((scratch / 'gate-1/report.json').read_text())
assert len(gate['commands']) == 5, 'complete gate required before retention'
out.mkdir(parents=True, exist_ok=False)
for name in ['focused.py', 'focused-tail.py', 'gate.py', 'retain.py']:
    shutil.copy2(scratch / name, out / name)
for name in ['focused-1', 'focused-2', 'gate-1']:
    shutil.copytree(scratch / name, out / name)
review = {
    'workflow': 'check',
    'base': 'origin/main',
    'subject': 'RepeatGap definition clocks, scoped actual-gap support projection and headless inspection; earlier captured-framing and Original-moment changes retained as baseline',
    'reviewers': [
        {'agent': 'gap_clock_general_review', 'scope': 'correctness, admission, compactness, API and tests', 'findings': []},
        {'agent': 'gap_clock_audio_review', 'scope': 'exact clocks, support/policy masks, cache identity and current recipe semantics', 'findings': []},
    ],
    'root_review': [
        'No preceding-play identity invented for an unplayed gap definition',
        'Actual-media current/historical CLI reads preserve source admission and history',
        'Zero-duration fixture checks rejection before plan construction',
    ],
    'remaining_findings': [],
}
(out / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
verification = {
    'base_revision': subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'committed': False, 'pushed': False, 'git_write_access': 'read-only in this session',
    'environment': {'os': 'macOS 26.5.2', 'build': '25F84', 'architecture': 'arm64',
                    'rust': '1.97.1 (8bab26f4f 2026-07-14)',
                    'ffmpeg_prefix': '/tmp/deadpan-media-compatible-xyhilms4/prefix',
                    'hardware_query': 'No new hardware query; prior sandbox denial retained'},
    'schemas': {'core': 20, 'database': 26, 'audio_context': 2, 'changed_by_this_increment': False},
    'gate': gate,
    'focused_unique_final_cases': {'core': 34, 'plan': 34, 'audio': 29, 'cli': 13},
    'focused_initial_failure': 'One new PCM fixture attempted to construct a forbidden zero-duration Hold; corrected to assert rejection at document admission',
    'gui_review': 'Not run: no native control, startup, presentation or picture changes in this increment',
    'new_imagegen': False,
    'remaining_work': ['Authored Repeat-gap binding ownership and birth', 'Compact per-occurrence resume entry selection', 'Atomic selected-moment splice', 'Native Visual selection and registers', 'Full normative product and release gates'],
}
(out / 'verification.json').write_text(json.dumps(verification, indent=2) + '\n')
manifest = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(out.rglob('*')) if p.is_file()}
(out / 'sha256.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'directory': str(out), 'files': len(manifest), 'gate': gate}))
