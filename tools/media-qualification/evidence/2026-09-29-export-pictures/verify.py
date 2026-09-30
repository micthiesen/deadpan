"""Audit retained command journals and every synthetic I420 plane."""
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
for name in ('clippy-final', 'build-metal', 'metal', 'test-workspace', 'fmt-final'):
    assert summary['commands'][name]['exit_code'] == 0
    assert summary['commands'][name]['source_manifest_sha256'] == summary['final_source_manifest_sha256']
results = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', gzip.decompress((root / 'commands/test-workspace.log.gz').read_bytes()).decode())
assert all(row[0] == 'ok' and row[2:4] == ('0', '0') for row in results)
assert summary['tests'] == {'passed': sum(int(row[1]) for row in results), 'failed': 0, 'ignored': 0, 'result_records': len(results)}
artifact = read_json('metal-artifact.json')
assert artifact['exit_code'] == 0 and artifact['binary_unchanged']
assert artifact['sha256'] == artifact['sha256_after']
assert artifact['cargo_artifact']['target']['name'] == 'qualify_project_picture'
build_rows = []
for line in gzip.decompress((root / 'commands/build-metal.log.gz').read_bytes()).decode().splitlines():
    try:
        build_rows.append(json.loads(line))
    except json.JSONDecodeError:
        pass
assert artifact['cargo_artifact'] in build_rows

inventory = read_json('frames.json')
frames = {}
with tarfile.open(root / 'frames.tar.gz', 'r:gz') as archive:
    for member in archive:
        assert member.isfile() and member.name in inventory and member.name not in frames
        assert member.size <= 1920 * 1080 * 3 // 2
        data = archive.extractfile(member).read()
        assert inventory[member.name] == {'bytes': len(data), 'sha256': digest(data)}
        frames[member.name] = data
assert set(inventory) == set(frames)
report = json.loads(gzip.decompress((root / 'metal-report.json.gz').read_bytes()))
assert report['status'] == report['generated']['status'] == 'passed'
assert all(check['passed'] for check in report['checks'])
actual_frames = {Path(frame['path']).name: frame for frame in report['frames']}
for name, frame in actual_frames.items():
    assert len(frames[name]) == frame['byte_count'] and digest(frames[name]) == frame['sha256']
comparisons = [check['actual'] for check in report['checks'] if 'planes' in check['actual']]
seen_references = set()
codes = beyond_one = maximum = 0
for comparison in comparisons:
    name = Path(comparison['reference_path']).name
    assert name not in seen_references and digest(frames[name]) == comparison['reference_sha256']
    seen_references.add(name)
    actual_name = name.replace('-reference-', '-')
    actual = frames[actual_name]
    expected = frames[name]
    width, height = actual_frames[actual_name]['raster']
    y_length = width * height
    assert len(actual) == len(expected) == y_length * 3 // 2
    for plane, start, length in [('Y', 0, y_length), ('Cb', y_length, y_length // 4), ('Cr', y_length * 5 // 4, y_length // 4)]:
        differences = [abs(a - b) for a, b in zip(actual[start:start+length], expected[start:start+length])]
        observed = next(value for value in comparison['planes'] if value['plane'] == plane)
        assert observed['codes'] == length
        assert observed['different_codes'] == sum(value != 0 for value in differences)
        assert observed['codes_beyond_one'] == sum(value > 1 for value in differences) == 0
        assert observed['maximum_absolute_error'] == max(differences) <= 1
        assert math.isclose(observed['mean_absolute_error'], sum(differences) / length, abs_tol=1e-15)
        codes += length
        beyond_one += observed['codes_beyond_one']
        maximum = max(maximum, observed['maximum_absolute_error'])
assert set(frames) == set(actual_frames) | seen_references
assert summary['metal'] == {'checks': len(report['checks']), 'frames': len(report['frames']), 'reference_comparisons': len(comparisons), 'codes_compared': codes, 'codes_beyond_one': beyond_one, 'maximum_absolute_error': maximum}
fixture = read_json('fixture-reference.json')
repository = root.parents[3]
assert digest((repository / fixture['path']).read_bytes()) == fixture['sha256']
print(json.dumps({'status': 'passed', 'files': len(manifest), 'plane_files': len(frames), 'tests': summary['tests'], 'metal': summary['metal']}, indent=2))
