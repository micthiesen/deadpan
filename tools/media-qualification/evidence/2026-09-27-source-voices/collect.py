"""Retain exact source and check evidence, including failures, without Git writes."""
from pathlib import Path
import gzip, hashlib, json, shutil, subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-source-voices-20260927')
evidence = repo / 'tools/media-qualification/evidence/2026-09-27-source-voices'
evidence.mkdir(parents=True, exist_ok=True)
gate = scratch / 'gate-01'
report = json.loads((gate / 'report.json').read_text())
assert len(report['commands']) == 7
assert report['source_unchanged'], report['changed_sources']
sources = json.loads((gate / 'source-after.json').read_text())
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in sources.items())
(scratch / 'final-source.json').write_text(json.dumps(sources, indent=2) + '\n')
runs = []
for folder in sorted(scratch.iterdir()):
    if not folder.is_dir() or not (folder / 'report.json').exists() or folder.name in ('before', 'checkpoint'):
        continue
    target = evidence / folder.name
    target.mkdir(exist_ok=True)
    for path in sorted(folder.rglob('*')):
        if not path.is_file() or path.suffix not in ('.json', '.log'):
            continue
        relative = path.relative_to(folder)
        output = target / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        if path.suffix == '.log':
            output.with_suffix('.log.gz').write_bytes(gzip.compress(path.read_bytes(), mtime=0))
        else:
            shutil.copyfile(path, output)
    runs.append({'run': folder.name, 'report': json.loads((folder / 'report.json').read_text())})
(evidence / 'verification.json').write_text(json.dumps(dict(runs=runs, goal_status='active', core_schema=28, database_schema=34,
    no_requirement_or_gate_promoted=True, ui_replay='not repeated; no GUI changes', contributed_harness='preserved and feature checks included in gate'), indent=2) + '\n')
subprocess.run(['python3', str(scratch / 'summarize-tree.py')], check=True)
white = subprocess.run(['git', '-c', 'core.fsmonitor=false', 'diff', '--check'], cwd=repo, capture_output=True, text=True)
(scratch / 'whitespace.json').write_text(json.dumps(dict(exit_code=white.returncode, stdout=white.stdout, stderr=white.stderr), indent=2) + '\n')
white.check_returncode()
names = ['context.json', 'review.json', 'final-source.json', 'doc-links.json', 'increment.json', 'whitespace.json',
         'next-slice.md', 'gate.py', 'scope.py', 'summarize-tree.py', 'run-checks.py', 'collect.py', 'checkpoint.py', 'records.py', 'followups.py']
for name in names:
    shutil.copyfile(scratch / name, evidence / name)
(evidence / 'increment.diff.gz').write_bytes(gzip.compress((scratch / 'increment.diff').read_bytes(), mtime=0))
hashes = {str(p.relative_to(evidence)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(evidence.rglob('*')) if p.is_file() and p.name != 'sha256.json'}
(evidence / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
subprocess.run(['python3', str(scratch / 'checkpoint.py')], check=True)
print(json.dumps(dict(evidence_files=len(hashes), sources=len(sources), runs=[r['run'] for r in runs])))
