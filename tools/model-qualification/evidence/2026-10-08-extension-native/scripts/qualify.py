"""Exercise real generation through the ordinary packaged CLI on owned fixtures."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).resolve().parent
parser = argparse.ArgumentParser()
parser.add_argument('--cli', type=Path, required=True)
parser.add_argument('--prepare-only', action='store_true')
args = parser.parse_args()
cli = args.cli.resolve()
assert cli.is_file()

def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')

def system_sample():
    sample = {'monotonic_seconds': time.monotonic()}
    for label, command in [('swap', ['sysctl', 'vm.swapusage']),
                           ('vm', ['vm_stat']),
                           ('thermal', ['pmset', '-g', 'therm']),
                           ('memory_pressure', ['memory_pressure', '-Q'])]:
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=5)
            sample[label] = {'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
        except (OSError, subprocess.TimeoutExpired) as error:
            sample[label] = {'unavailable': str(error)}
    return sample

def run(case, name, *arguments):
    stdout = case / (name + '.json')
    stderr = case / (name + '.stderr')
    assert not stdout.exists(), stdout
    command = [str(cli), *map(str, arguments)]
    samples = [system_sample()] if name == 'generated' else []
    start = time.monotonic()
    with stdout.open('x') as output, stderr.open('x') as errors:
        process = subprocess.Popen(command, cwd=root, stdout=output, stderr=errors)
        while True:
            try:
                process.wait(timeout=15)
                break
            except subprocess.TimeoutExpired:
                if name == 'generated':
                    samples.append(system_sample())
    elapsed = time.monotonic() - start
    if samples:
        samples.append(system_sample())
        save(case / (name + '.system.json'), samples)
    record = {'command': command, 'exit_code': process.returncode, 'seconds': elapsed}
    save(case / (name + '.execution.json'), record)
    print(json.dumps({'case': case.name, 'action': name, **record}), flush=True)
    if process.returncode:
        print(stderr.read_text()[-4000:], flush=True)
        raise RuntimeError(f'{case.name}/{name}: exit {process.returncode}')
    return json.loads(stdout.read_text())

def command(case, name, package, document, payload):
    request = {'protocol': 1, 'project_id': document['project_id'],
               'expected_revision': document['revision_id'], 'new_revision': name, 'command': payload}
    path = case / (name + '.request.json')
    save(path, request)
    return run(case, name, 'command', package, '--json', path)

cases = []
for frames, generated, motion in [(12, 8, 'still'), (24, 24, 'still'), (48, 48, 'subtle'), (72, 72, 'moderate')]:
    for direction in ['from_left', 'from_right']:
        case = root / f'{frames}f-{direction}'
        case.mkdir(exist_ok=True)
        package = case / 'project.deadpan'
        if not package.exists():
            original = repo / 'tools/model-qualification/evidence/2026-10-08-extension-one-second/fixtures' / (direction + '.mp4')
            created = run(case, 'create', 'project', 'create-original', package, original)['created']
            assert created['single_source']['state'] == 'ready'
            assert created['presentation_basis']['frame_rate'] == {'numerator': 24, 'denominator': 1}
            document = run(case, 'original', 'project', 'dump', package, '--json')
            command(case, 'inserted-pause', package, document, {
                'command': 'insert', 'parent': document['root'], 'index': int(direction == 'from_left'),
                'subtree': {'root': 'extension', 'nodes': {'extension': {'label': 'Extension qualification', 'kind': {'type': 'hold',
                    'recipe': {'duration': frames, 'video': {'type': 'freeze', 'asset': created['asset_id'],
                        'timestamp': {'ticks': 8 if direction == 'from_left' else 0, 'time_base': {'numerator': 1, 'denominator': 24}}},
                        'audio': {'type': 'silence'}}}}}, 'overrides': {}, 'gap_overrides': {}}})
            run(case, 'before', 'project', 'dump', package, '--json')
        cases.append({'case': case.name, 'frames': frames, 'generated_frames': generated, 'direction': direction, 'motion': motion})

save(root / 'cases.json', cases)
if args.prepare_only:
    raise SystemExit(0)
save(root / 'executed-binary.json', {'path': str(cli), 'sha256': hashlib.file_digest(cli.open('rb'), 'sha256').hexdigest()})
results = []
for item in cases:
    case = root / item['case']
    package = case / 'project.deadpan'
    if (case / 'generated.json').exists():
        raise RuntimeError('Refusing to overwrite an earlier real run')
    try:
        generated = run(case, 'generated', 'generate-hold', package, '--hold', 'extension', '--mode', 'auto',
                        '--motion', item['motion'], '--seed', '42107')
        assert generated['state'] == 'Ready', generated
        plan = generated['plan']
        assert plan['operation'] == 'extension'
        sampling = plan['plan']['sampling']
        assert sampling['direction'] == item['direction']
        assert sampling['output_frame_count'] == item['frames']
        assert sampling['context_frame_count'] == 9
        assert sampling['generated_frame_count'] == item['generated_frames']
        after = run(case, 'after-ready', 'project', 'dump', package, '--json')
        assert after == json.loads((case / 'before.json').read_text()), 'Ready changed authored state'
        results.append({**item, 'ready': True, 'request': generated['request_id'], 'attempt': generated['attempt_id'],
                        'timings_ms': generated['timings_ms']})
    except Exception as error:
        results.append({**item, 'ready': False, 'error': str(error)})
    save(root / 'results.json', results)
raise SystemExit(0 if all(result['ready'] for result in results) else 1)
