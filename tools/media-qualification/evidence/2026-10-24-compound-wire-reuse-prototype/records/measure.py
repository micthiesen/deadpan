from pathlib import Path
import hashlib
import importlib.util
import itertools
import json
import math
import os
import statistics
import subprocess
import time

S = Path('/tmp/deadpan-compound-wire-reuse-prototype-2026-10-24')
SOURCE = S / 'source'
PROFILE_ENV = 'DEADPAN_PAUSE_ROUTE_PROFILE'
PROFILE_PREFIX_FAMILY = 'DEADPAN_PAUSE_ROUTE_PROFILE'
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
PERF_HASH = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
HELPER_MANIFEST = Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json')
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
STATE = {'stage': 'preflight', 'trial': None}
OWNED = False
METRICS = (
    'large_first_pause_input_to_commit_ms',
    'large_first_pause_input_to_offscreen_background_composition_ms',
)
ORDERS = [
    ['control', 'baseline', 'candidate'],
    ['baseline', 'candidate', 'control'],
    ['candidate', 'control', 'baseline'],
    ['control', 'candidate', 'baseline'],
    ['candidate', 'baseline', 'control'],
    ['baseline', 'control', 'candidate'],
]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(name):
    return json.loads((S / name).read_text())


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def save_exclusive(name, value):
    with (S / name).open('x') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def source_map(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*.rs'))}


def snapshot(root):
    result = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise AssertionError(('fixture contains symlink', str(path)))
        if path.is_file():
            result[str(path.relative_to(root))] = {
                'bytes': path.stat().st_size, 'sha256': sha(path)}
    return result


def validate_pins():
    pins = read('driver-pins.json')
    required = {'build.py', 'measure.py', 'expected-build-stages.json',
                'expected-tests.json', 'check_profile.py'}
    assert required <= pins.keys(), 'both drivers and parser must be pinned'
    assert all(sha(S / path) == digest for path, digest in pins.items()), (
        'pinned harness input changed')
    return pins


def validate_sources():
    candidate = read('frozen-sources.json')
    matched = read('baseline-sources.json')
    base = read('baseline-provenance.json')
    matched_root = Path(base['matched_source'])
    inputs = read('baseline-inputs.json')
    assert sha(S / 'frozen-sources.json') == read('source-review.json')['source_map_sha256']
    assert source_map(SOURCE) == candidate, 'candidate Rust source changed'
    assert source_map(matched_root) == matched, 'matched profile source changed'
    assert sha(S / 'baseline-sources.json') == base['matched_source_map_sha256']
    assert set(candidate) == set(matched), 'Rust source path set changed'
    assert all(sha(SOURCE / path) == digest for path, digest in inputs.items())
    assert all(sha(matched_root / path) == digest for path, digest in inputs.items())
    review = read('source-review.json')
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == sha(S / 'frozen-sources.json')


def validate_build():
    stages = read('expected-build-stages.json')
    expected = read('build-receipt.json')
    terminal = read('build-terminal.json')
    assert stages == read('expected-build-stages.json')
    assert [row['name'] for row in expected] == stages
    assert all(row['exit'] == 0 and row['validation'] == 'passed' for row in expected)
    assert terminal['status'] == 'complete' and terminal['steps'] == len(stages)
    assert terminal['stage_names'] == stages
    tests = read('test-binaries.json')
    full = tests['core-unit-all']
    assert full['tests']['failed'] == 0 and full['tests']['ignored'] == 0
    assert full['tests']['executed_tests']
    assert full['executed'] == full['retained']
    assert sha(full['executed']) == full['sha256']
    binaries = read('binaries.json')
    assert terminal['app_sha256'] == binaries['deadpan-app']['sha256']
    assert binaries['deadpan-app']['freshly_built'] is True
    assert all(sha(row['executed']) == row['sha256'] for row in binaries.values())
    base = read('baseline-provenance.json')
    baseline = read('baseline.json')
    assert base['matched_app'] == baseline['matched_profile']['path']
    assert base['matched_app_sha256'] == baseline['matched_profile']['sha256']
    assert sha(base['matched_app']) == base['matched_app_sha256']
    assert sha(baseline['control']['path']) == baseline['control']['sha256']
    assert baseline['control']['sha256'] == '970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2'
    assert baseline['matched_profile']['sha256'] == '1a12c2e9f866de1cef3ec64ea3997b26a1baafa194a95e3eef9b4587607d288a'
    assert sha(PERF) == PERF_HASH


def no_outputs_yet():
    names = ['measurement-owner.json', 'measurement-receipt.json',
             'measurement-terminal.json', 'trials.json', 'summary.json']
    names += [f'rotation-{rotation}-{kind}{suffix}'
              for rotation in range(1, 7)
              for kind in ['control', 'baseline', 'candidate']
              for suffix in [
                  '.stdout', '.stderr', '-make.stdout', '-make.stderr',
                  '-input-before.json', '-input-after.json', '-preflight.json',
                  '-profile.json', '.deadpan']]
    names += [f'rotation-{rotation}-{kind}-report'
              for rotation in range(1, 7)
              for kind in ['control', 'baseline', 'candidate']]
    assert all(not (S / name).exists() for name in names), (
        'measurement output exists; refusing overwrite',
        [name for name in names if (S / name).exists()])


def main():
    global OWNED
    assert not (S / 'measurement-owner.json').exists(), 'refusing to replace measurement owner'
    assert not (S / 'measurement-receipt.json').exists(), 'refusing to overwrite measurements'
    assert not (S / 'measurement-terminal.json').exists(), 'refusing to reuse terminal measurements'
    no_outputs_yet()
    validate_pins()
    validate_sources()
    validate_build()
    assert sha(PERF) == PERF_HASH
    helper_manifest = json.loads(HELPER_MANIFEST.read_text())
    for name in ['deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
        row = read('binaries.json')[name]
        assert row['qualified_helper_reuse'] is True
        assert row['sha256'] == helper_manifest[name]['sha256']
        assert sha(row['executed']) == row['sha256']
    parser_spec = importlib.util.spec_from_file_location('pause_profile_parser', S / 'check_profile.py')
    parser = importlib.util.module_from_spec(parser_spec)
    parser_spec.loader.exec_module(parser)
    assert parser.PREFIX == 'DEADPAN_PAUSE_ROUTE_PROFILE_V3'
    assert len(parser.PHASES) == 35
    assert len(parser.EXPECTED_COUNTS) == 35

    baseline = read('baseline.json')
    provenance = read('baseline-provenance.json')
    candidate_app = read('binaries.json')['deadpan-app']
    bins = {
        'control': {'executed': baseline['control']['path'],
                    'sha256': baseline['control']['sha256']},
        'baseline': {'executed': provenance['matched_app'],
                     'sha256': provenance['matched_app_sha256']},
        'candidate': {'executed': candidate_app['executed'],
                      'sha256': candidate_app['sha256']},
    }
    candidate_sources = read('frozen-sources.json')
    matched_sources = read('baseline-sources.json')
    inputs = read('baseline-inputs.json')
    pins = read('driver-pins.json')

    save_exclusive('measurement-owner.json', {
        'pid': os.getpid(), 'started_unix': time.time(),
        'script': str(Path(__file__)), 'memory_observer': False,
        'source_map_sha256': sha(S / 'frozen-sources.json'),
        'matched_source_map_sha256': sha(S / 'baseline-sources.json'),
        'candidate_app_sha256': candidate_app['sha256'],
        'control_app_sha256': bins['control']['sha256'],
        'matched_profile_app_sha256': bins['baseline']['sha256'],
    })
    OWNED = True
    commands, trials = [], []

    def guard():
        validate_pins()
        assert source_map(SOURCE) == candidate_sources
        assert source_map(Path(provenance['matched_source'])) == matched_sources
        assert all(sha(SOURCE / path) == digest for path, digest in inputs.items())
        assert all(sha(Path(provenance['matched_source']) / path) == digest
                   for path, digest in inputs.items())
        assert all(sha(row['executed']) == row['sha256'] for row in bins.values())
        assert all(sha(row['executed']) == row['sha256']
                   for row in read('binaries.json').values())
        assert sha(PERF) == PERF_HASH

    def run(name, args, env):
        STATE['stage'] = name
        started = time.monotonic()
        with (S / (name + '.stdout')).open('x') as out, (S / (name + '.stderr')).open('x') as err:
            result = subprocess.run(args, cwd=SOURCE, env=env, stdout=out, stderr=err)
        row = {'name': name, 'command': args, 'cwd': str(SOURCE),
               'exit': result.returncode, 'seconds': time.monotonic() - started,
               'profile_gate': env.get(PROFILE_ENV)}
        commands.append(row)
        save('measurement-receipt.json', commands)
        print(json.dumps(row), flush=True)
        assert result.returncode == 0, (name, result.returncode)

    def quiet(name):
        STATE['stage'] = 'quiet preflight'
        started, observations = time.monotonic(), []
        save(name + '-preflight.json', observations)
        while True:
            result = subprocess.run(['ps', '-axo', 'pid=,comm='],
                                    capture_output=True, text=True)
            conflicts = [line for line in result.stdout.splitlines()
                         if any(x in line.split(maxsplit=1)[-1].lower()
                                for x in ['cargo', 'rustc', 'deadpan-app',
                                          'memory_observer', 'memory-observer'])]
            load = list(os.getloadavg())
            observations.append({
                'seconds': time.monotonic() - started, 'load': load,
                'conflicting_processes': conflicts, 'ps_exit': result.returncode,
                'ps_stderr': result.stderr,
            })
            save(name + '-preflight.json', observations)
            assert result.returncode == 0, observations[-1]
            if load[0] <= 3 and not conflicts:
                return
            assert time.monotonic() - started < 240, observations[-1]
            time.sleep(5)

    for rotation, order in enumerate(ORDERS, 1):
        assert len(order) == 3 and set(order) == set(bins)
        for kind in order:
            guard()
            name = f'rotation-{rotation}-{kind}'
            STATE['trial'] = name
            fixture = S / (name + '.deadpan')
            output = S / (name + '-report')
            assert not fixture.exists() and not output.exists()
            env = dict(os.environ)
            env.pop(PROFILE_ENV, None)
            run(name + '-make', [str(PERF), 'make-large', str(fixture), '--beats', '10000'], env)
            before = snapshot(fixture)
            assert 'project.sqlite' in before
            save(name + '-input-before.json', before)
            quiet(name)
            if kind in ('baseline', 'candidate'):
                env[PROFILE_ENV] = '1'
            run(name, [bins[kind]['executed'], '--ui-check', '--mode', 'performance',
                       '--scenario', 'large-first-pause', '--project', str(fixture),
                       '--output', str(output)], env)
            STATE['stage'] = 'post-command validation'
            after = snapshot(fixture)
            save(name + '-input-after.json', after)
            assert {key: value for key, value in before.items() if key not in SIDECARS} == {
                key: value for key, value in after.items() if key not in SIDECARS}, (
                name, 'authored fixture changed')
            assert after.get('project.sqlite-wal', {}).get('bytes', 0) == 0, (
                name, 'WAL is nonempty')
            report = json.loads((output / 'report.json').read_text())
            assert report['failed'] is False
            metadata = report['metadata']
            assert metadata['binary_sha256'] == bins[kind]['sha256']
            assert metadata['build_profile'] == 'release'
            assert metadata['fixture_sha256'] == before['project.sqlite']['sha256']
            assert metadata['cargo_lock_sha256_at_run'] == inputs['Cargo.lock']
            assert metadata['physical_display_measured'] is False
            assert metadata['gpu_readback_in_timing_run'] is False
            assert str(metadata['rust']).startswith('rustc 1.97.1 ')
            assert [scenario['name'] for scenario in report['scenarios']] == [
                'kestrel-shortcuts', 'large-first-pause']
            kestrel = report['scenarios'][0]
            pause = report['scenarios'][1]
            assert len(kestrel['checks']) == 1 and kestrel['checks'][0]['passed']
            assert len(pause['checks']) == 7 and all(row['passed'] for row in pause['checks'])
            metric_rows = [row for row in pause['timings']
                           if row['name'].startswith('large_first_pause')]
            assert len(metric_rows) == 2 and {row['name'] for row in metric_rows} == set(METRICS)
            metrics = {
                row['name']: row['summary']['completed']['p50_ms']
                for row in metric_rows
            }
            assert all(type(value) in (int, float) and math.isfinite(value) and value >= 0
                       for value in metrics.values()), metrics
            stderr = (S / (name + '.stderr')).read_text()
            profile = None
            if kind in ('baseline', 'candidate'):
                profile = parser.parse_profile(stderr)
                assert profile['block_count'] == 1, (
                    name, 'expected one trace block for one qualified insertion')
                selected = profile['selected']
                assert [row['name'] for row in selected['phases']] == list(parser.PHASES)
                assert selected['phase_sum_ns'] == selected['elapsed_ns']
                save(name + '-profile.json', profile)
            else:
                assert PROFILE_PREFIX_FAMILY not in stderr, (
                    name, 'profile gate off emitted trace output')
            warnings = sum(len(scenario.get('findings', []))
                           for scenario in report['scenarios'])
            row = {
                'rotation': rotation, 'kind': kind,
                'binary_sha256': bins[kind]['sha256'],
                'source_map_sha256': sha(S / 'frozen-sources.json'),
                'matched_source_map_sha256': sha(S / 'baseline-sources.json'),
                'metrics': metrics, 'profile': profile,
                'checks': len(pause['checks']),
                'kestrel_checks': len(kestrel['checks']),
                'warnings': warnings,
                'authored_input_unchanged': True,
                'wal_empty': True,
            }
            trials.append(row)
            save('trials.json', trials)
            guard()

    assert len(commands) == 36 and len(trials) == 18
    assert all(row['exit'] == 0 for row in commands)
    assert {row['rotation'] for row in trials} == set(range(1, 7))
    for rotation in range(1, 7):
        rows = [row for row in trials if row['rotation'] == rotation]
        assert len(rows) == 3 and {row['kind'] for row in rows} == set(bins)
    assert all(row['checks'] == 7 and row['kestrel_checks'] == 1 for row in trials)

    summary = {
        'diagnostic_only': True, 'n_per_kind': 6, 'kinds': {},
        'phase_accounting': 'existing 35 exclusive phases; parser verifies exact ordering and conservation',
        'no_percentile_target_or_memory_claim': True,
        'profile_lanes': {},
        'pairs': [],
    }
    for kind in ['control', 'baseline', 'candidate']:
        subset = [row for row in trials if row['kind'] == kind]
        summary['kinds'][kind] = {
            metric: {
                'min': min(row['metrics'][metric] for row in subset),
                'median': statistics.median(row['metrics'][metric] for row in subset),
                'max': max(row['metrics'][metric] for row in subset),
            }
            for metric in METRICS
        }
    for rotation in range(1, 7):
        by_kind = {row['kind']: row for row in trials if row['rotation'] == rotation}
        summary['pairs'].append({
            'rotation': rotation,
            'baseline_minus_candidate_commit_ms':
                by_kind['baseline']['metrics']['large_first_pause_input_to_commit_ms']
                - by_kind['candidate']['metrics']['large_first_pause_input_to_commit_ms'],
            'baseline_minus_candidate_composition_ms':
                by_kind['baseline']['metrics']['large_first_pause_input_to_offscreen_background_composition_ms']
                - by_kind['candidate']['metrics']['large_first_pause_input_to_offscreen_background_composition_ms'],
            'control_minus_candidate_commit_ms':
                by_kind['control']['metrics']['large_first_pause_input_to_commit_ms']
                - by_kind['candidate']['metrics']['large_first_pause_input_to_commit_ms'],
        })
    for kind in ['baseline', 'candidate']:
        blocks = [row['profile']['selected'] for row in trials if row['kind'] == kind]
        assert len(blocks) == 6
        summary['profile_lanes'][kind] = {
            'exclusive_phases': {
                phase: {
                    'min_ns': min(block['phases'][index]['elapsed_ns'] for block in blocks),
                    'median_ns': statistics.median(
                        block['phases'][index]['elapsed_ns'] for block in blocks),
                    'max_ns': max(block['phases'][index]['elapsed_ns'] for block in blocks),
                    'count': blocks[0]['phases'][index]['count'],
                }
                for index, phase in enumerate(parser.PHASES)
            },
            'outer_elapsed_ns': [block['elapsed_ns'] for block in blocks],
        }
    save('summary.json', summary)
    save_exclusive('measurement-terminal.json', {
        'status': 'complete', 'trials': len(trials), 'commands': len(commands),
        'workflow_checks': sum(row['checks'] for row in trials),
        'kestrel_checks': sum(row['kestrel_checks'] for row in trials),
    })
    print(json.dumps({
        'terminal': True, 'trials': len(trials), 'commands': len(commands),
        'workflow_checks': sum(row['checks'] for row in trials),
        'kestrel_checks': sum(row['kestrel_checks'] for row in trials),
    }), flush=True)


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        failure = {'status': 'failed', **STATE,
                   'exception': type(error).__name__, 'message': str(error)}
        if OWNED:
            try:
                save_exclusive('measurement-terminal.json', failure)
            except FileExistsError:
                pass
        else:
            save_exclusive(f'refused-measurement-{time.time_ns()}.json', failure)
        raise
