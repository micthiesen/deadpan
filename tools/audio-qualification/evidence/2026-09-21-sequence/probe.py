import datetime
import fractions
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path('/Users/michael/Code/deadpan')
scratch = Path(tempfile.mkdtemp(prefix='deadpan-sequence-audio-probe-', dir='/tmp'))
package = scratch / 'Sequence.deadpan'
output = Path('/tmp/deadpan-sequence-audio-20260921/probe')
output.mkdir(parents=True, exist_ok=True)
environment = dict(os.environ)
environment['DEADPAN_FFMPEG_PREFIX'] = '/tmp/deadpan-media-compatible-xyhilms4/prefix'
environment['PATH'] = str(repo / 'target/debug') + os.pathsep + environment['PATH']
report = {'schema_version': 1, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
          'scratch': str(scratch), 'commands': [], 'checks': [], 'status': 'running'}

def save():
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

def run(name, arguments):
    result = subprocess.run(arguments, cwd=repo, env=environment, capture_output=True)
    (output / (name + '.stdout')).write_bytes(result.stdout)
    (output / (name + '.stderr')).write_bytes(result.stderr)
    report['commands'].append({'name': name, 'arguments': arguments, 'exit_code': result.returncode,
                               'stdout': name + '.stdout', 'stderr': name + '.stderr'})
    save()
    if result.returncode:
        raise RuntimeError(name + ': ' + result.stderr.decode())
    return json.loads(result.stdout)

def cli(name, *arguments):
    return run(name, ['deadpan-cli', *map(str, arguments)])

def inspect(name, start, count=256, app=False):
    arguments = ['inspect-audio', str(package), '--samples', str(start), str(start + count)]
    return run(name, (['deadpan-app', '--headless'] if app else ['deadpan-cli']) + arguments)

try:
    initial = cli('create', 'project', 'create', package, '--fps', '30000/1001', '--size', '320x180')
    fixture = repo / 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4'
    retained = cli('retain', 'project', 'retain-original', package, fixture)
    original = retained['retained_original']['record']['object']['content']
    registration = {'protocol': 1, 'registration': {
        'expected_revision': initial['revision_id'], 'new_revision': 'source-import',
        'original': original, 'new_asset_id': 'fixture-audio', 'label': 'Own synthetic AAC fixture',
        'insertion': {'parent': initial['root'], 'index': 0, 'node': 'fixture-source', 'label': 'Source'}},
        'streams': {'type': 'audio_only', 'stream': 1}}
    request = scratch / 'registration.json'
    request.write_text(json.dumps(registration, indent=2) + '\n')
    cli('register', 'project', 'register-source', package, '--request-json', request)
    document = cli('source-document', 'project', 'dump', package, '--json')
    duration = document['nodes']['fixture-source']['kind']['source']['duration']
    first = inspect('source-cli', 0)
    assert first == inspect('source-app', 0, app=True)
    assert first['audio']['stage'] == 'source_pcm_before_effects'
    assert len(first['audio']['samples']) == 256
    assert max(abs(channel) for frame in first['audio']['samples'] for channel in frame) > 0.01
    report['checks'].append('CLI and native host headless entrypoints emit identical source PCM around the known AAC impulse at sample 100')
    repeat = {'protocol': 1, 'project_id': document['project_id'],
              'expected_revision': document['revision_id'], 'new_revision': 'three-plays',
              'command': {'command': 'wrap_repeat', 'node': 'fixture-source', 'id': 'repeat', 'plays': 3,
                          'gap': {'duration': 1, 'video': {'type': 'background'}, 'audio': {'type': 'silence'}}}}
    command = scratch / 'repeat.json'
    command.write_text(json.dumps(repeat, indent=2) + '\n')
    cli('repeat', 'command', package, '--json', command)
    before = cli('before-inspection', 'project', 'dump', package, '--json')
    ticks = fractions.Fraction(48000 * 1001, 30000)
    boundary = lambda frame: round(frame * ticks)
    starts = [boundary((duration + 1) * play) for play in range(3)]
    first_repeat = inspect('repeat-first', starts[0])
    assert first_repeat['audio']['samples'] == first['audio']['samples']
    second = inspect('repeat-second', starts[1])
    assert second == inspect('repeat-second-app', starts[1], app=True)
    pieces = inspect('repeat-second-prefix', starts[1], 73)['audio']['samples']
    pieces += inspect('repeat-second-suffix', starts[1] + 73, 183)['audio']['samples']
    assert pieces == second['audio']['samples']
    for play in range(2):
        gap = inspect('gap-' + str(play), boundary(duration + (duration + 1) * play))
        assert all(frame == [0.0, 0.0] for frame in gap['audio']['samples'])
    third = inspect('repeat-third', starts[2])
    assert max(abs(channel) for frame in third['audio']['samples'] for channel in frame) > 0.01
    validation = cli('validate', 'project', 'validate', package)
    assert validation['duration_frames'] == 3 * duration + 2
    assert cli('after-inspection', 'project', 'dump', package, '--json') == before
    report['checks'].extend(['Three total plays, two silent gaps, no trailing gap in validated duration',
                             'Irregular query partitions preserve exact second-play PCM on the NTSC sample grid',
                             'Post-inspection authored snapshot equals pre-inspection snapshot'])
    report['fixture_sha256'] = hashlib.sha256(fixture.read_bytes()).hexdigest()
    report['source_duration_frames'] = duration
    report['repeat_duration_frames'] = validation['duration_frames']
    report['repeat_duration_samples'] = boundary(validation['duration_frames'])
    report['play_sample_starts'] = starts
    report['status'] = 'passed'
except Exception as error:
    report['status'] = 'failed'
    report['error'] = repr(error)
    raise
finally:
    report['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    print(output / 'report.json')
