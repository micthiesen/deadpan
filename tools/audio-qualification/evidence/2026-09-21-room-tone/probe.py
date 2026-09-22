"""Actual CLI/native-host room-tone loops, silence and exact original-audio resume."""

import datetime
import fractions
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import struct
import subprocess
import tempfile
import time


REPO = Path('/Users/michael/Code/deadpan')
OUTPUT = Path('/tmp/deadpan-room-tone-20260921/probe')
PREFIX = '/tmp/deadpan-media-compatible-xyhilms4/prefix'
EXPECTED_BASE = '320ea14'
OUTPUT.mkdir(parents=True, exist_ok=True)
SCRATCH = Path(tempfile.mkdtemp(prefix='deadpan-room-tone-process-', dir='/tmp'))
PACKAGE = SCRATCH / 'Room tone.deadpan'
ENVIRONMENT = dict(os.environ)
ENVIRONMENT['DEADPAN_FFMPEG_PREFIX'] = PREFIX
ENVIRONMENT['PATH'] = str(REPO / 'target/debug') + os.pathsep + ENVIRONMENT['PATH']
REPORT = {
    'schema_version': 1,
    'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'expected_base_revision_prefix': EXPECTED_BASE,
    'scratch': str(SCRATCH),
    'package': str(PACKAGE),
    'ffmpeg_prefix': PREFIX,
    'commands': [],
    'checks': [],
    'status': 'running',
}


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def save():
    write_json(OUTPUT / 'report.json', REPORT)


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def run(name, arguments, expected_exit=0, decode_json=True):
    started = time.monotonic()
    result = subprocess.run(
        list(map(str, arguments)), cwd=REPO, env=ENVIRONMENT,
        capture_output=True, timeout=300,
    )
    (OUTPUT / (name + '.stdout')).write_bytes(result.stdout)
    (OUTPUT / (name + '.stderr')).write_bytes(result.stderr)
    REPORT['commands'].append({
        'name': name,
        'arguments': list(map(str, arguments)),
        'expected_exit_code': expected_exit,
        'exit_code': result.returncode,
        'elapsed_seconds': time.monotonic() - started,
        'stdout': name + '.stdout',
        'stderr': name + '.stderr',
    })
    save()
    if result.returncode != expected_exit:
        raise RuntimeError(name + ': ' + result.stderr.decode(errors='replace'))
    if decode_json:
        return json.loads(result.stdout if result.stdout else result.stderr)
    return result.stdout.decode().strip()


def cli(name, *arguments, **options):
    return run(name, ['deadpan-cli', *arguments], **options)


def inspect(name, start, count, app=False, mapped=True, expected_exit=0):
    arguments = ['inspect-audio', PACKAGE, '--samples', start, start + count]
    if mapped:
        arguments.append('--time-mapped')
    return run(name, (['deadpan-app', '--headless'] if app else ['deadpan-cli']) + arguments,
               expected_exit=expected_exit)


def command(name, document, revision, operation):
    envelope = {
        'protocol': 1,
        'project_id': document['project_id'],
        'expected_revision': document['revision_id'],
        'new_revision': revision,
        'command': operation,
    }
    request = OUTPUT / (name + '.request.json')
    write_json(request, envelope)
    return cli(name, 'command', PACKAGE, '--json', request)


def database_summary(name):
    connection = sqlite3.connect((PACKAGE / 'project.sqlite').as_uri() + '?mode=ro', uri=True)
    try:
        tables = [row[0] for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        )]
        counts = {
            table: connection.execute('SELECT COUNT(*) FROM "' + table.replace('"', '""') + '"').fetchone()[0]
            for table in tables
        }
        logical = '\n'.join(connection.iterdump()).encode()
        summary = {
            'table_counts': counts,
            'state': list(connection.execute('SELECT singleton,head_revision,cursor FROM state')),
            'history': list(connection.execute('SELECT id,parent_id,revision_id FROM history ORDER BY id')),
            'logical_dump_sha256': hashlib.sha256(logical).hexdigest(),
            'user_version': connection.execute('PRAGMA user_version').fetchone()[0],
        }
        write_json(OUTPUT / (name + '.json'), summary)
        return summary
    finally:
        connection.close()


try:
    REPORT['base_revision'] = run('git-head', ['git', 'rev-parse', 'HEAD'], decode_json=False)
    assert REPORT['base_revision'].startswith(EXPECTED_BASE)
    REPORT['hardware'] = {
        'model': run('hardware-model', ['sysctl', '-n', 'hw.model'], decode_json=False),
        'processor': run('hardware-processor', ['sysctl', '-n', 'machdep.cpu.brand_string'], decode_json=False),
        'memory_bytes': int(run('hardware-memory', ['sysctl', '-n', 'hw.memsize'], decode_json=False)),
        'macos': run('operating-system', ['sw_vers'], decode_json=False),
        'rustc': run('rustc', ['rustc', '--version'], decode_json=False),
    }
    source_paths = [
        'Cargo.lock',
        'crates/deadpan-audio/src/stages.rs',
        'crates/deadpan-audio/src/room_tone.rs',
        'crates/deadpan-audio/src/resample.rs',
        'crates/deadpan-audio/src/session.rs',
        'crates/deadpan-plan/src/audio_signal.rs',
        'crates/deadpan-plan/src/audio.rs',
        'crates/deadpan-cli/src/audio.rs',
        'crates/deadpan-cli/src/lib.rs',
        'native/deadpan-dsp/src/lib.rs',
    ]
    REPORT['source_sha256'] = {path: sha256(REPO / path) for path in source_paths}
    REPORT['binary_sha256'] = {name: sha256(REPO / 'target/debug' / name)
                               for name in ['deadpan-cli', 'deadpan-app']}
    REPORT['script_sha256'] = sha256(Path(__file__))
    fixture = REPO / 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4'
    REPORT['fixture'] = str(fixture.relative_to(REPO))
    REPORT['fixture_sha256'] = sha256(fixture)

    initial = cli('create', 'project', 'create', PACKAGE, '--fps', '30000/1001', '--size', '320x180')
    retained = cli('retain', 'project', 'retain-original', PACKAGE, fixture)
    original = retained['retained_original']['record']['object']['content']
    registration = {
        'protocol': 1,
        'registration': {
            'expected_revision': initial['revision_id'],
            'new_revision': 'source-import',
            'original': original,
            'new_asset_id': 'fixture-audio',
            'label': 'Own synthetic AAC fixture',
            'insertion': {'parent': initial['root'], 'index': 0, 'node': 'fixture-source', 'label': 'Source'},
        },
        'streams': {'type': 'audio_only', 'stream': 1},
    }
    request = OUTPUT / 'registration.request.json'
    write_json(request, registration)
    cli('register', 'project', 'register-source', PACKAGE, '--request-json', request)
    document = cli('registered-document', 'project', 'dump', PACKAGE, '--json')
    original_node = document['nodes']['fixture-source']
    original_duration = original_node['kind']['source']['duration']
    original_audio = inspect('original-selection', 0, 256, mapped=False)['audio']['samples']
    assert len(original_audio) == 256
    selected = {
        'asset': 'fixture-audio',
        'span': {
            'start': {'ticks': 0, 'time_base': {'numerator': 1, 'denominator': 48000}},
            'end': {'ticks': 256, 'time_base': {'numerator': 1, 'denominator': 48000}},
        },
    }
    command('insert-room-tone-and-silence', document, 'room-tone-inserted', {
        'command': 'insert', 'parent': document['root'], 'index': 0,
        'subtree': {
            'root': 'inserted-time', 'overrides': {},
            'nodes': {
                'inserted-time': {
                    'label': 'Room tone then silence',
                    'kind': {'type': 'sequence', 'children': ['room-tone', 'silent-hold']},
                },
                'room-tone': {
                    'label': 'Explicit synthetic room-tone selection',
                    'kind': {'type': 'hold', 'recipe': {
                        'duration': 3, 'video': {'type': 'background'},
                        'audio': {'type': 'room_tone', 'source': selected},
                    }},
                },
                'silent-hold': {
                    'label': 'One silent frame',
                    'kind': {'type': 'hold', 'recipe': {
                        'duration': 1, 'video': {'type': 'background'},
                        'audio': {'type': 'silence'},
                    }},
                },
            },
        },
    })
    before = cli('before-inspection', 'project', 'dump', PACKAGE, '--json')
    before_db = database_summary('database-before')
    assert before['nodes']['fixture-source'] == original_node
    validation = cli('validate', 'project', 'validate', PACKAGE)
    assert validation['duration_frames'] == original_duration + 4
    samples_per_frame = fractions.Fraction(48000 * 1001, 30000)
    room_end = round(3 * samples_per_frame)
    speech_start = round(4 * samples_per_frame)
    assert room_end == 4805 and speech_start == 6406
    speech_phase = fractions.Fraction(speech_start) - 4 * samples_per_frame
    assert speech_phase == fractions.Fraction(-2, 5)
    planned = cli('plan-audio', 'inspect-plan', PACKAGE, '--audio-samples', 0, speech_start + 1)
    spans = planned['audio']['spans']
    assert [(span['samples']['start'], span['samples']['end']) for span in spans] == [
        (0, room_end), (room_end, speech_start), (speech_start, speech_start + 1),
    ]
    assert spans[0]['content']['type'] == 'room_tone'
    assert spans[0]['content']['duration'] == 3
    assert spans[1]['content']['type'] == 'silence'
    origin = spans[2]['transform']['project_origin']
    assert fractions.Fraction(int(origin['numerator']), int(origin['denominator'])) == 4
    REPORT['checks'].append('Three room-tone frames allocate samples 0..4805, one silent frame 4805..6406, then unchanged original Source resumes at exact frame 4 with sample phase -2/5')

    rejected = inspect('source-only-rejection', 0, 256, mapped=False, expected_exit=1)
    assert rejected['error']['code'] == 'AudioOperationUnsupported'
    REPORT['checks'].append('Source-only inspection explicitly rejects room-tone processing')

    first = inspect('room-first-cli', 0, 256)
    assert first == inspect('room-first-app', 0, 256, app=True)
    assert first['audio']['stage'] == 'time_mapped_pcm_before_effects'
    assert first['audio']['suppressed'] == []
    complete = list(first['audio']['samples'])
    for start in range(256, room_end, 256):
        complete.extend(inspect('room-full-' + str(start), start, min(256, room_end - start))['audio']['samples'])
    assert len(complete) == room_end

    def f32(value):
        return struct.unpack('<f', struct.pack('<f', value))[0]

    # The selected original clock is already 48 kHz. The explicit source extent
    # is 256, fade 96 and period 160; compute the scalar contract independently.
    original = [[f32(channel) for channel in frame] for frame in original_audio]
    expected = []
    for sample in range(room_end):
        phase = sample % 160
        if sample < 160 or phase >= 96:
            expected.append(original[phase])
        else:
            weight = phase / 96.0
            expected.append([f32(original[160 + phase][channel] * (1.0 - weight)
                                 + original[phase][channel] * weight) for channel in range(2)])
    actual_f32 = [[f32(channel) for channel in frame] for frame in complete]
    assert actual_f32 == expected
    assert actual_f32[:160] == original[:160]
    assert complete[160:320] == complete[320:480]
    assert max(abs(channel) for frame in complete for channel in frame) > 0.01
    REPORT['checks'].append('All 4805 emitted room-tone samples exactly match an independent scalar 96-sample overlap loop of the actual qualified 256-sample AAC selection')

    partitioned = []
    pattern = [1, 73, 256, 17, 127]
    part = 0
    while len(partitioned) < room_end:
        start = len(partitioned)
        count = min(pattern[part % len(pattern)], room_end - start)
        block = inspect('room-part-' + str(part), start, count)['audio']
        assert block['suppressed'] == []
        partitioned.extend(block['samples'])
        part += 1
    assert partitioned == complete
    for name, start, count in [('seam', 147, 219), ('near-end', room_end - 177, 177)]:
        actual = inspect('room-' + name + '-cli', start, count)
        assert actual == inspect('room-' + name + '-app', start, count, app=True)
        assert actual['audio']['samples'] == complete[start:start + count]
    REPORT['checks'].extend([
        'CLI and native-app headless processes emit identical first, loop-seam and near-end room-tone PCM',
        'Complete room-tone PCM is identical under canonical-size and irregular fresh-process partitions',
    ])

    silence = inspect('silence-cli', room_end, 256)
    assert silence == inspect('silence-app', room_end, 256, app=True)
    assert silence['audio']['samples'] == [[0.0, 0.0]] * 256
    assert silence['audio']['suppressed'] == [{'start': room_end, 'end': room_end + 256}]
    boundary = inspect('room-to-silence-boundary', room_end - 13, 39)['audio']
    assert boundary['samples'][:13] == complete[-13:]
    assert boundary['samples'][13:] == [[0.0, 0.0]] * 26
    assert boundary['suppressed'] == [{'start': room_end, 'end': room_end + 26}]
    resumed = inspect('resumed-speech-cli', speech_start, 256)
    assert resumed == inspect('resumed-speech-app', speech_start, 256, app=True)
    reference = inspect('resumed-speech-source-only', speech_start, 256, mapped=False)
    assert resumed['audio']['samples'] == reference['audio']['samples']
    assert resumed['audio']['suppressed'] == []
    assert max(abs(channel) for frame in resumed['audio']['samples'] for channel in frame) > 0.01
    crossing = inspect('silence-to-speech-boundary', speech_start - 7, 31)['audio']
    assert crossing['samples'][:7] == [[0.0, 0.0]] * 7
    assert crossing['samples'][7:] == resumed['audio']['samples'][:24]
    assert crossing['suppressed'] == [{'start': speech_start - 7, 'end': speech_start}]
    REPORT['checks'].extend([
        'Silent Hold emits exact zeros and explicit suppression through both boundaries',
        'Original speech remains authored identically and time-mapped output equals source-only sampling at its exact shifted project origin',
    ])

    after = cli('after-inspection', 'project', 'dump', PACKAGE, '--json')
    after_db = database_summary('database-after')
    assert after == before
    assert after_db == before_db
    assert before_db['table_counts']['history'] == 2
    assert before_db['table_counts']['revisions'] == 3
    REPORT['checks'].append('Authored snapshot, every SQLite table count, history rows and complete logical database dump remain unchanged')
    write_json(OUTPUT / 'complete-room-pcm.json', complete)
    REPORT['room_duration_frames'] = 3
    REPORT['room_duration_samples'] = room_end
    REPORT['silent_sample_range'] = [room_end, speech_start]
    REPORT['speech_start_sample_phase'] = {'numerator': -2, 'denominator': 5}
    REPORT['total_duration_frames'] = validation['duration_frames']
    REPORT['total_duration_samples'] = round(validation['duration_frames'] * samples_per_frame)
    REPORT['complete_pcm_sha256'] = sha256(OUTPUT / 'complete-room-pcm.json')
    REPORT['database_before'] = before_db
    REPORT['database_after'] = after_db
    REPORT['status'] = 'passed'
except Exception as error:
    REPORT['status'] = 'failed'
    REPORT['error'] = repr(error)
    raise
finally:
    REPORT['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
    print(OUTPUT / 'report.json')
