"""Retain one evidence bundle, including failures and source-bound continuations."""
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import tarfile

scratch = Path(sys.argv[1])
destination = Path(sys.argv[2])
destination.mkdir(parents=True, exist_ok=False)

def digest(data):
    return hashlib.sha256(data).hexdigest()

def copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)

def compress(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(gzip.compress(source.read_bytes(), mtime=0))

journals = [
    'format', 'clippy-workspace', 'clippy-workspace-2', 'clippy-workspace-3',
    'test-workspace', 'clippy-ui', 'test-ui', 'visual-generated',
    'performance-generated', 'test-layout-app', 'clippy-layout', 'clippy-layout-ui',
    'visual-final', 'performance-final', 'test-ui-final', 'format-final',
    'visual-status-failure', 'visual-status-occlusion', 'visual-status-fixed',
    'visual-all-status-fixed', 'visual-room-tone-final', 'visual-room-tone-compact',
    'format-complete', 'clippy-app-status-fixed', 'clippy-ui-status-fixed',
    'test-app-status-fixed', 'test-ui-status-fixed',
    'performance-status-fixed', 'performance-all-status-fixed',
]
verification = []
for name in journals:
    journal = scratch / (name + '.json')
    result = json.loads(journal.read_text())
    assert 'exit_code' in result, name
    copy(journal, destination / 'commands' / journal.name)
    log = scratch / (name + '.log')
    compress(log, destination / 'commands' / (name + '.log.gz'))
    source = scratch / ('source-' + result['source_manifest_sha256'] + '.json')
    assert digest(source.read_bytes()) == result['source_manifest_sha256']
    compress(source, destination / 'sources' / (source.name + '.gz'))
    totals = [tuple(map(int, match)) for match in re.findall(
        r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log.read_text())]
    verification.append({'name': name, **result, 'test_result_records': len(totals),
                         'test_totals': [sum(row[i] for row in totals) for i in range(3)]})

summaries = {}
captures = []
runs = ['visual', 'performance', 'visual-final', 'performance-final',
        'visual-status-failure', 'visual-status-occlusion', 'visual-status-fixed',
        'visual-all-status-fixed', 'visual-room-tone-final', 'visual-room-tone-compact',
        'performance-status-fixed', 'performance-all-status-fixed']
for run in runs:
    report_path = scratch / run / 'report.json'
    report = json.loads(report_path.read_text())
    compress(report_path, destination / 'reports' / (run + '.json.gz'))
    summaries[run] = {'failed': report['failed'], 'metadata': report['metadata'], 'scenarios': []}
    for scenario in report['scenarios']:
        summaries[run]['scenarios'].append({
            key: scenario[key] for key in ['name', 'failed', 'skipped', 'findings']
        } | {'checks': len(scenario['checks']),
             'failed_checks': [c for c in scenario['checks'] if not c['passed']],
             'timings': [{key: metric[key] for key in ['name', 'summary']} for metric in scenario['timings']]})
        for step in scenario['steps']:
            label = step['input']
            chosen = (scenario['name'] == 'generated-picture' and label in [
                'Accepted Generated frame 0 with retained captured framing',
                'Generated frame 29 settled at minimum size',
            ]) or (run in ['visual-room-tone-final', 'visual-room-tone-compact'] and label in [
                'Saved room tone in Hold inspector at 960x640',
                'Saved room tone in Hold inspector at 1280x820',
            ]) or (run == 'visual-all-status-fixed' and label in [
                'Original viewer at the minimum window size',
                'Paint saved room-tone inspector after resize',
                'Resize the measured gain waveform viewport',
            ]) or (run == 'visual-status-occlusion' and label == 'Failure state')
            if not chosen or not step.get('screenshot'):
                continue
            name = run + '-' + step['screenshot']
            copy(scratch / run / step['screenshot'], destination / 'captures' / name)
            captures.append({'file': 'captures/' + name, 'label': label,
                             'viewport_points': step['semantic']['viewport_points'],
                             'source': run, 'frame': step['frame']})

(destination / 'summary.json').write_text(json.dumps(summaries, indent=2) + '\n')
(destination / 'verification.json').write_text(json.dumps(verification, indent=2) + '\n')
(destination / 'captures.json').write_text(json.dumps(captures, indent=2) + '\n')
copy(scratch / 'host.json', destination / 'host.json')
copy(scratch / 'fixture/generated-picture-fixture.json', destination / 'generated-picture-fixture.json')
copy(scratch / 'final-review.md', destination / 'review.md')
copy(Path(__file__), destination / 'retain-evidence.py')
copy(Path('/tmp/deadpan-project-pictures-EYNVPYJA/run-native.py'), destination / 'run-native.py')
objects = sorted((scratch / 'fixture/accepted.deadpan/Media/Generated').iterdir())
assert len(objects) == 6 and all(path.is_file() for path in objects)
with tarfile.open(destination / 'generated-objects.tar.gz', 'w:gz') as archive:
    for path in objects:
        archive.add(path, arcname=path.name, recursive=False)
(destination / 'generated-objects.json').write_text(json.dumps({path.name: {
    'bytes': path.stat().st_size, 'sha256': digest(path.read_bytes())
} for path in objects}, indent=2) + '\n')
print(json.dumps({'destination': str(destination), 'captures': len(captures), 'objects': len(objects)}))
