"""Actual CLI/native-host parity for revision-bound Preserve-stage AAC reads."""

import datetime
import fractions
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time


REPO = Path('/Users/michael/Code/deadpan')
OUTPUT = Path('/tmp/deadpan-stages-audio-20260921/probe')
PREFIX = '/tmp/deadpan-media-compatible-xyhilms4/prefix'
EXPECTED_BASE = '9bc3bb9'
OUTPUT.mkdir(parents=True, exist_ok=True)
SCRATCH = Path(tempfile.mkdtemp(prefix='deadpan-stages-audio-process-', dir='/tmp'))
PACKAGE = SCRATCH / 'Stages.deadpan'
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
    command('remove-original-beat', document, 'remove-source', {'command': 'delete', 'node': 'fixture-source'})
    removed = cli('removed-document', 'project', 'dump', PACKAGE, '--json')
    command('insert-preserve', removed, 'preserve-three-into-two', {
        'command': 'insert', 'parent': removed['root'], 'index': 0,
        'subtree': {
            'root': 'preserve', 'overrides': {},
            'nodes': {
                'source-copy': original_node,
                'preserve': {
                    'label': 'Three frames into two, preserve pitch',
                    'kind': {'type': 'retime', 'child': 'source-copy', 'duration': 2,
                             'mapping': {'start': 0, 'end': 3}, 'pitch': 'preserve'},
                },
            },
        },
    })
    before = cli('before-inspection', 'project', 'dump', PACKAGE, '--json')
    before_db = database_summary('database-before')
    validation = cli('validate', 'project', 'validate', PACKAGE)
    assert validation['duration_frames'] == 2
    samples_per_frame = fractions.Fraction(48000 * 1001, 30000)
    frames = round(2 * samples_per_frame)
    assert frames == 3203
    planned = cli('plan-audio', 'inspect-plan', PACKAGE, '--audio-samples', 0, frames)
    stages = [stage for span in planned['audio']['spans'] for stage in span['retimes']]
    assert len(stages) == 1
    speed = stages[0]['child_frames_per_local_frame']
    assert fractions.Fraction(int(speed['numerator']), int(speed['denominator'])) == fractions.Fraction(3, 2)
    assert stages[0]['pitch'] == 'preserve'
    REPORT['checks'].append('Stored three-frame child selection maps to two NTSC frames with exact Preserve speed 3/2 and final allocation 3203 samples')

    rejected = inspect('source-only-rejection', 0, 256, mapped=False, expected_exit=1)
    assert rejected['error']['code'] == 'AudioOperationUnsupported'
    REPORT['checks'].append('Source-only inspection explicitly rejects pitch-preserving processing')

    first = inspect('mapped-first-cli', 0, 256)
    assert first == inspect('mapped-first-app', 0, 256, app=True)
    assert first['audio']['stage'] == 'time_mapped_pcm_before_effects'
    assert first['audio']['suppressed'] == []
    assert max(abs(channel) for frame in first['audio']['samples'] for channel in frame) > 0.0001
    complete = list(first['audio']['samples'])
    for start in range(256, frames, 256):
        complete.extend(inspect('mapped-full-' + str(start), start, min(256, frames - start))['audio']['samples'])
    assert len(complete) == frames

    partitioned = []
    pattern = [1, 73, 256, 17, 127]
    part = 0
    while len(partitioned) < frames:
        start = len(partitioned)
        count = min(pattern[part % len(pattern)], frames - start)
        partitioned.extend(inspect('mapped-part-' + str(part), start, count)['audio']['samples'])
        part += 1
    assert partitioned == complete
    for name, start, count in [('middle', 1499, 219), ('near-end', frames - 177, 177)]:
        actual = inspect('mapped-' + name + '-cli', start, count)
        assert actual == inspect('mapped-' + name + '-app', start, count, app=True)
        assert actual['audio']['samples'] == complete[start:start + count]
    REPORT['checks'].extend([
        'CLI and native-app headless processes emit identical first, middle and near-end time-mapped PCM',
        'A complete 3203-sample read split into canonical-size and irregular process queries is exactly equal',
    ])
    after = cli('after-inspection', 'project', 'dump', PACKAGE, '--json')
    after_db = database_summary('database-after')
    assert after == before
    assert after_db == before_db
    assert before_db['table_counts']['history'] == 3
    assert before_db['table_counts']['revisions'] == 4
    REPORT['checks'].append('Authored snapshot, every SQLite table count, history rows and complete logical database dump remain unchanged')
    write_json(OUTPUT / 'complete-pcm.json', complete)
    REPORT['rendered_frames'] = frames
    REPORT['prepared_input_extent'] = {'numerator': 24024, 'denominator': 5}
    REPORT['prepared_output_extent'] = {'numerator': 16016, 'denominator': 5}
    REPORT['complete_pcm_sha256'] = sha256(OUTPUT / 'complete-pcm.json')
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
