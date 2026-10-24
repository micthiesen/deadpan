from pathlib import Path
import hashlib
import json
import math
import os
import statistics
import subprocess
import time

S = Path('/tmp/deadpan-compound-wire-reuse-production-2026-10-24')
SOURCE = S / 'source'

CONTROL_SHA256 = '970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2'
BASE_MAP_SHA256 = 'f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53'
CANDIDATE_MAP_SHA256 = '2b0997f1ad22ce7dd869beba699909d22972efba434279cee350478dc63cb728'
OWNED_SOURCES = {'crates/deadpan-core/src/compound.rs',
                 'crates/deadpan-core/src/one_leaf_replay.rs'}
PIN_REQUIRED = {'build.py', 'measure.py', 'expected-build-stages.json',
                'expected-tests.json', 'expected-measurement.json', 'baseline.json',
                'baseline-provenance.json', 'baseline-sources.json', 'baseline-inputs.json',
                'frozen-sources.json', 'source-review.json'}
GATE = 'DEADPAN_PAUSE_ROUTE_PROFILE'
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
PERF_HASH = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
PREFIX = 'DEADPAN_PAUSE_ROUTE_PROFILE'
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
STATE = {'stage': 'preflight', 'trial': None}
OWNED = False
METRICS = {'large_first_pause_input_to_commit_ms',
           'large_first_pause_input_to_offscreen_background_composition_ms'}


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def read(name):
    return json.loads((S / name).read_text())


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def save_exclusive(name, value):
    with (S / name).open('x') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def snapshot(root):
    return {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
            for p in sorted(root.rglob('*')) if p.is_file()}


def main():
    global OWNED
    assert not (S / 'measurement-receipt.json').exists(), 'refusing to overwrite trials'
    assert not (S / 'measurement-terminal.json').exists(), 'refusing to reuse terminal measurement'
    assert not (S / 'measurement-owner.json').exists(), 'refusing to replace measurement owner'
    build = read('build-receipt.json')
    expected_stages = read('expected-build-stages.json')
    terminal = read('build-terminal.json')
    assert expected_stages and [row['name'] for row in build] == expected_stages
    assert all(row['exit'] == 0 and row['validation'] == 'passed' for row in build)
    assert terminal['status'] == 'complete' and terminal['steps'] == len(expected_stages)
    assert terminal['stage_names'] == expected_stages
    assert sha(PERF) == PERF_HASH
    pins = read('driver-pins.json')
    assert PIN_REQUIRED <= pins.keys(), 'Root must review and pin the complete protocol before measurement'
    assert all(sha(S / path) == digest for path, digest in pins.items())
    protocol = read('expected-measurement.json')
    control = read('baseline.json')['control']
    assert control['sha256'] == CONTROL_SHA256 == protocol['control_sha256']
    app = read('binaries.json')['deadpan-app']
    assert terminal['app_sha256'] == app['sha256']
    assert app['freshly_built'] and not app['qualified_helper_reuse']
    assert Path(app['source']).resolve() == (S / 'target/release/deadpan-app').resolve()
    assert Path(app['executed']).resolve() == (S / 'bin/deadpan-app').resolve()
    matched_sources = read('baseline-sources.json')
    provenance = read('baseline-provenance.json')
    matched_root = Path(provenance['matched_source'])
    assert sha(S / 'baseline-sources.json') == BASE_MAP_SHA256 == protocol['control_source_map_sha256']
    assert sha(S / 'frozen-sources.json') == CANDIDATE_MAP_SHA256 == protocol['candidate_source_map_sha256']
    assert provenance['matched_source_map_sha256'] == BASE_MAP_SHA256
    assert provenance['matched_app'] == control['path']
    assert provenance['matched_app_sha256'] == CONTROL_SHA256
    bins = {'control': {'executed': control['path'], 'sha256': control['sha256']},
            'candidate': app}
    helpers = read('binaries.json')
    review = read('source-review.json')
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == CANDIDATE_MAP_SHA256
    sources = read('frozen-sources.json')
    inputs = read('baseline-inputs.json')
    # Ownership is claimed only after all immutable-source and review guards pass.
    commands, trials, first_fixture = [], [], None

    def guard():
        actual = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')}
        assert actual == sources and len(actual) == 2053
        assert {str(p.relative_to(matched_root)): sha(p) for p in matched_root.rglob('*.rs')} == matched_sources
        assert set(actual) == set(matched_sources)
        assert {path for path in actual if actual[path] != matched_sources[path]} == OWNED_SOURCES
        assert sum(actual[path] == matched_sources[path] for path in actual) == 2051
        assert len(inputs) == 6
        assert read('source-review.json') == review
        assert sha(S / 'frozen-sources.json') == CANDIDATE_MAP_SHA256
        assert sha(S / 'baseline-sources.json') == BASE_MAP_SHA256
        assert all(sha(SOURCE / p) == digest for p, digest in inputs.items())
        assert all(sha(matched_root / p) == digest for p, digest in inputs.items())
        assert all(sha(row['executed']) == row['sha256'] for row in bins.values())
        assert all(sha(row['executed']) == row['sha256'] for row in helpers.values())
        assert all(sha(S / path) == digest for path, digest in pins.items())
        assert sha(PERF) == PERF_HASH

    def run(name, args, env):
        STATE['stage'] = name
        started = time.monotonic()
        assert env.get(GATE) is None, 'profiling must be off'
        with (S / (name + '.stdout')).open('x') as out, (S / (name + '.stderr')).open('x') as err:
            result = subprocess.run(args, cwd=SOURCE, env=env, stdout=out, stderr=err)
        row = {'name': name, 'command': args, 'exit': result.returncode,
               'seconds': time.monotonic() - started, 'profile_gate': env.get(GATE)}
        commands.append(row)
        save('measurement-receipt.json', commands)
        print(json.dumps(row), flush=True)
        assert result.returncode == 0, name
        assert PREFIX not in ((S / (name + '.stdout')).read_text()
                              + (S / (name + '.stderr')).read_text()), 'profile output is forbidden'

    def quiet(name):
        STATE['stage'] = 'quiet preflight'
        start, observations = time.monotonic(), []
        save(name + '-preflight.json', observations)
        while True:
            result = subprocess.run(['ps', '-axo', 'pid=,comm='], capture_output=True, text=True)
            conflicts = [line for line in result.stdout.splitlines()
                         if any(x in line.split(maxsplit=1)[-1].lower()
                                for x in ['cargo', 'rustc', 'deadpan-app', 'memory_observer', 'memory-observer'])]
            load = list(os.getloadavg())
            observations.append({'seconds': time.monotonic() - start, 'load': load,
                                 'conflicting_processes': conflicts, 'ps_exit': result.returncode,
                                 'ps_stderr': result.stderr})
            save(name + '-preflight.json', observations)
            assert result.returncode == 0, observations[-1]
            if load[0] <= 3 and not conflicts:
                break
            assert time.monotonic() - start < 240, observations[-1]
            time.sleep(5)

    orders = protocol['orders']
    assert orders == [['control', 'candidate'], ['candidate', 'control'],
                      ['control', 'candidate'], ['candidate', 'control'],
                      ['control', 'candidate']]
    assert (protocol['trials'], protocol['commands'], protocol['pause_checks'],
            protocol['kestrel_checks'], protocol['warnings']) == (10, 20, 70, 10, 20)
    assert protocol['profile_gate'] is None and not protocol['profile_output_allowed']
    assert not protocol['memory_observer'] and not protocol['memory_claim']
    guard()
    save_exclusive('measurement-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                             'script': str(Path(__file__)), 'memory_observer': False})
    OWNED = True
    for rotation, order in enumerate(orders, 1):
        for kind in order:
            guard()
            name = f'rotation-{rotation}-{kind}'
            STATE['trial'] = name
            fixture, output = S / (name + '.deadpan'), S / (name + '-report')
            assert not fixture.exists() and not output.exists()
            env = dict(os.environ)
            env.pop(GATE, None)
            run(name + '-make', [str(PERF), 'make-large', str(fixture), '--beats', '10000'], env)
            before = snapshot(fixture)
            if first_fixture is None:
                first_fixture = before
            assert before == first_fixture, 'fresh fixture bytes differ across trials'
            assert before.get('project.sqlite-wal', {}).get('bytes', 0) == 0
            save_exclusive(name + '-input-before.json', before)
            quiet(name)
            binary = bins[kind]
            run(name, [binary['executed'], '--ui-check', '--mode', 'performance',
                       '--scenario', 'large-first-pause', '--project', str(fixture),
                       '--output', str(output)], env)
            STATE['stage'] = 'post-command validation'
            after = snapshot(fixture)
            save_exclusive(name + '-input-after.json', after)
            assert {k: v for k, v in before.items() if k not in SIDECARS} == {
                k: v for k, v in after.items() if k not in SIDECARS}
            assert after.get('project.sqlite-wal', {}).get('bytes', 0) == 0
            report = json.loads((output / 'report.json').read_text())
            assert report['failed'] is False
            meta = report['metadata']
            assert meta['binary_sha256'] == binary['sha256'] and meta['build_profile'] == 'release'
            assert meta['fixture_sha256'] == before['project.sqlite']['sha256']
            assert meta['cargo_lock_sha256_at_run'] == inputs['Cargo.lock']
            assert meta['physical_display_measured'] is False
            assert meta['gpu_readback_in_timing_run'] is False
            assert str(meta['rust']).startswith('rustc 1.97.1 ')
            assert [c['name'] for c in report['scenarios']] == ['kestrel-shortcuts', 'large-first-pause']
            case = report['scenarios'][1]
            assert len(case['checks']) == 7 and all(c['passed'] for c in case['checks'])
            kestrel = report['scenarios'][0]
            assert len(kestrel['checks']) == 1 and kestrel['checks'][0]['passed']
            metric_rows = [t for t in case['timings'] if t['name'].startswith('large_first_pause')]
            assert len(metric_rows) == 2 and {t['name'] for t in metric_rows} == METRICS
            metrics = {t['name']: t['summary']['completed']['p50_ms'] for t in metric_rows}
            assert all(type(value) in (float, int) and math.isfinite(value) and value >= 0
                       for value in metrics.values()), metrics
            stderr = (S / (name + '.stderr')).read_text()
            stdout = (S / (name + '.stdout')).read_text()
            assert PREFIX not in stderr + stdout, 'uninstrumented app emitted profile output'
            assert 'DEADPAN_PAUSE_ROUTE_PROFILE_V3' not in stderr + stdout
            assert not kestrel['findings']
            findings = case['findings']
            assert len(findings) == 2 and all(f['severity'] == 'warning' for f in findings)
            assert all(finding['message'].startswith(prefix) for finding, prefix
                       in zip(findings, protocol['warning_message_prefixes'], strict=True))
            row = {'rotation': rotation, 'kind': kind, 'binary_sha256': binary['sha256'],
                   'metrics': metrics, 'checks': 7, 'kestrel_checks': 1,
                   'warnings': len(findings), 'findings': findings,
                   'fixture_sha256': before['project.sqlite']['sha256'],
                   'authored_input_unchanged': True, 'wal_empty': True}
            trials.append(row)
            save('trials.json', trials)
            guard()
    assert len(commands) == 20 and len(trials) == 10
    assert all(row['exit'] == 0 and row['profile_gate'] is None for row in commands)
    assert sum(row['checks'] for row in trials) == protocol['pause_checks'] == 70
    assert sum(row['kestrel_checks'] for row in trials) == protocol['kestrel_checks'] == 10
    assert sum(row['warnings'] for row in trials) == protocol['warnings'] == 20
    summary = {'diagnostic_only': True, 'n_per_kind': 5, 'kinds': {},
               'phase_accounting': 'no diagnostic hooks; production-route observation only',
               'no_percentile_target_or_memory_claim': True}
    for kind in ['control', 'candidate']:
        subset = [t for t in trials if t['kind'] == kind]
        summary['kinds'][kind] = {m: {'min': min(values), 'median': statistics.median(values), 'max': max(values)}
                                 for m in subset[0]['metrics']
                                 for values in [[t['metrics'][m] for t in subset]]}
    commit = 'large_first_pause_input_to_commit_ms'
    compose = 'large_first_pause_input_to_offscreen_background_composition_ms'
    paired = []
    for rotation in range(1, 6):
        pair = {t['kind']: t for t in trials if t['rotation'] == rotation}
        assert set(pair) == {'control', 'candidate'}
        paired.append({'rotation': rotation,
                       'control_minus_candidate_commit_ms': pair['control']['metrics'][commit] - pair['candidate']['metrics'][commit],
                       'control_minus_candidate_composition_ms': pair['control']['metrics'][compose] - pair['candidate']['metrics'][compose]})
    summary['pairs'] = paired
    summary['paired_commit_reduction_median_ms'] = statistics.median(t['control_minus_candidate_commit_ms'] for t in paired)
    summary['paired_composition_reduction_median_ms'] = statistics.median(t['control_minus_candidate_composition_ms'] for t in paired)
    save('summary.json', summary)
    save_exclusive('measurement-terminal.json', {'status': 'complete', 'trials': len(trials),
                                      'commands': len(commands), 'workflow_checks': 70,
                                      'kestrel_checks': 10, 'warnings': 20})
    print(json.dumps({'terminal': True, 'trials': 10, 'commands': 20,
                      'workflow_checks': 70, 'kestrel_checks': 10, 'warnings': 20}), flush=True)


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
