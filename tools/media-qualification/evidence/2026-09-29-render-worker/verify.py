"""Audit retained command journals, exact executables, and complete raw planes."""
import gzip
import hashlib
import json
import math
from pathlib import Path
import re
import tarfile

root = Path(__file__).resolve().parent
read_json = lambda name: json.loads((root / name).read_text())
digest = lambda data: hashlib.sha256(data).hexdigest()
manifest = read_json('manifest.json')
assert sorted(manifest) == sorted(str(path.relative_to(root)) for path in root.rglob('*') if path.is_file() and path.name != 'manifest.json')
for name, expected in manifest.items():
    path = root / name
    assert path.is_file() and not path.is_symlink(), name
    data = path.read_bytes()
    assert {'bytes': len(data), 'sha256': digest(data)} == expected, name
summary = read_json('summary.json')
for name, expected in summary['commands'].items():
    journal = read_json('commands/' + name + '.json')
    assert journal['exit_code'] == expected['exit_code'], name
    assert journal['source_manifest_sha256'] == expected['source_manifest_sha256'], name
    source = gzip.decompress((root / 'sources' / ('source-' + journal['source_manifest_sha256'] + '.json.gz')).read_bytes())
    assert digest(source) == journal['source_manifest_sha256'], name
for name in ('clippy-workspace', 'build-metal', 'metal', 'test-workspace', 'fmt-final', 'pipe-inheritance'):
    assert summary['commands'][name]['exit_code'] == 0
    assert summary['commands'][name]['source_manifest_sha256'] == summary['final_source_manifest_sha256']
results = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', gzip.decompress((root / 'commands/test-workspace.log.gz').read_bytes()).decode())
assert results and all(row[0] == 'ok' and row[2:4] == ('0', '0') for row in results)
assert summary['tests'] == {'passed': sum(int(row[1]) for row in results), 'failed': 0, 'ignored': 0, 'result_records': len(results)}
artifact = read_json('metal-artifact.json')
assert artifact['exit_code'] == 0 and not artifact.get('timed_out')
build_rows = []
for line in gzip.decompress((root / 'commands/build-metal.log.gz').read_bytes()).decode().splitlines():
    try:
        build_rows.append(json.loads(line))
    except json.JSONDecodeError:
        pass
for target in ('deadpan-cli', 'qualify_project_picture'):
    record = artifact['artifacts'][target]
    assert record['unchanged'] and record['sha256'] == record['sha256_after']
    assert record['cargo_artifact']['target']['name'] == target
    assert record['cargo_artifact'] in build_rows
pipe = read_json('pipe-inheritance.json')
assert pipe['passed'] and all(row['returncode'] == 0 and row['group_cleanup_confirmed'] and not row['cleanup_errors'] and not row['timed_out'] for row in pipe['commands'])

inventory = read_json('frames.json')
frames = {}
with tarfile.open(root / 'frames.tar.gz', 'r:gz') as archive:
    for member in archive:
        assert member.isfile() and member.name in inventory and member.name not in frames
        assert member.size <= 512 * 1024 * 1024
        data = archive.extractfile(member).read()
        assert inventory[member.name] == {'bytes': len(data), 'sha256': digest(data)}
        frames[member.name] = data
assert set(inventory) == set(frames)
report = json.loads(gzip.decompress((root / 'metal-report.json.gz').read_bytes()))
assert report['status'] == report['generated']['status'] == report['worker']['status'] == 'passed'
assert all(check['passed'] for group in (report, report['worker']) for check in group['checks'])
actual_frames = {Path(frame['path']).name: frame for frame in report['frames']}
for name, frame in actual_frames.items():
    assert len(frames[name]) == frame['byte_count'] and digest(frames[name]) == frame['sha256']
seen = set(actual_frames)
comparisons = [check['actual'] for check in report['checks'] if 'planes' in check['actual']]
worker = report['worker']
worker_frames = {}
for case in worker['cases']:
    assert case['status'] == 'passed'
    path, direct = (Path(case[key]).name for key in ('path', 'direct_path'))
    seen.update((path, direct))
    raw = frames[path]
    assert raw == frames[direct]
    assert digest(raw) == case['sha256'] == case['manifest']['planes']['sha256']
    contract = case['contract']
    width, height = contract['raster']
    length = width * height * 3 // 2
    assert len(raw) == case['manifest']['planes']['byte_length'] == len(case['frames']) * length
    assert len(case['frames']) == contract['frame_count']
    for ordinal, frame in enumerate(case['frames']):
        chunk = raw[ordinal*length:(ordinal+1)*length]
        assert digest(chunk) == frame['sha256'] == frame['direct_sha256']
        assert len(chunk) == frame['byte_length']
        assert frame['output_timing'] == frame['direct_metadata']['output_timing']
        worker_frames[(case['label'], ordinal)] = (chunk, (width, height))
worker_comparisons = [check['actual'] for check in worker['checks'] if 'planes' in check['actual']]

def compare(comparison, actual, raster):
    name = Path(comparison['reference_path']).name
    seen.add(name)
    expected = frames[name]
    assert digest(expected) == comparison['reference_sha256']
    width, height = raster
    y_length = width * height
    assert len(actual) == len(expected) == y_length * 3 // 2
    for plane, start, length in [('Y', 0, y_length), ('Cb', y_length, y_length // 4), ('Cr', y_length * 5 // 4, y_length // 4)]:
        differences = [abs(a-b) for a,b in zip(actual[start:start+length], expected[start:start+length])]
        observed = next(value for value in comparison['planes'] if value['plane'] == plane)
        assert observed['codes'] == length
        assert observed['different_codes'] == sum(value != 0 for value in differences)
        assert observed['codes_beyond_one'] == sum(value > 1 for value in differences) == 0
        assert observed['maximum_absolute_error'] == max(differences) <= 1
        assert math.isclose(observed['mean_absolute_error'], sum(differences)/length, abs_tol=1e-15)
for comparison in comparisons:
    actual_name = Path(comparison['reference_path']).name.replace('-reference-', '-')
    compare(comparison, frames[actual_name], actual_frames[actual_name]['raster'])
for comparison in worker_comparisons:
    actual, raster = worker_frames[(comparison['case'], comparison['ordinal'])]
    compare(comparison, actual, raster)
assert seen == set(frames)
assert len(worker_frames) == summary['metal']['worker_frames']
assert len(worker['checks']) == summary['metal']['worker_checks']
assert len(worker_comparisons) == summary['metal']['worker_reference_comparisons']
assert len(report['checks']) == summary['metal']['direct_checks']
assert len(report['frames']) == summary['metal']['direct_frames']
assert len(comparisons) == summary['metal']['direct_reference_comparisons']
assert worker['cancellation']['error'] == 'render preparation was cancelled'
progress = worker['cancellation']['progress'][0]
assert 0 < progress['completed_frames'] < progress['total_frames']
history = worker['live_history']
assert 0 < history['trigger_completed_frames'] < history['trigger_total_frames']
fixture = read_json('fixture-reference.json')
repository = root.parents[3]
assert digest((repository / fixture['path']).read_bytes()) == fixture['sha256']
print(json.dumps({'status':'passed','files':len(manifest),'plane_files':len(frames),'tests':summary['tests'],'metal':summary['metal']},indent=2))
