from pathlib import Path
import hashlib
import importlib.util
import json
import math
import os
import statistics
import subprocess
import time

S = Path('/tmp/deadpan-current-pause-detail-profile-2026-10-23')
SOURCE = S / 'source'
GATE = 'DEADPAN_PAUSE_ROUTE_PROFILE'
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
PERF_HASH = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
PREFIX = 'DEADPAN_PAUSE_ROUTE_PROFILE'
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
STATE = {'stage': 'preflight', 'trial': None}
METRICS = {'large_first_pause_input_to_commit_ms',
           'large_first_pause_input_to_offscreen_background_composition_ms'}


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def read(name):
    return json.loads((S / name).read_text())


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def snapshot(root):
    return {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
            for p in sorted(root.rglob('*')) if p.is_file()}


def main():
    assert not (S / 'measurement-receipt.json').exists(), 'refusing to overwrite trials'
    build = read('build-receipt.json')
    assert len(build) == 8 and all(row['exit'] == 0 for row in build)
    assert read('build-terminal.json')['status'] == 'complete'
    assert sha(PERF) == PERF_HASH
    pins = read('driver-pins.json')
    assert all(sha(S / path) == digest for path, digest in pins.items())
    spec = importlib.util.spec_from_file_location('profile_parser', S / 'check_profile.py')
    parser = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(parser)
    control = read('baseline.json')['control']
    app = read('binaries.json')['deadpan-app']
    bins = {'control': {'executed': control['path'], 'sha256': control['sha256']},
            'off': app, 'on': app}
    helpers = read('binaries.json')
    assert read('source-review.json')['source_map_sha256'] == sha(S / 'frozen-sources.json')
    sources = read('frozen-sources.json')
    inputs = read('baseline-inputs.json')
    save('measurement-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                   'script': str(Path(__file__)), 'memory_observer': False})
    commands, trials = [], []

    def guard():
        assert {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')} == sources
        assert all(sha(SOURCE / p) == digest for p, digest in inputs.items())
        assert all(sha(row['executed']) == row['sha256'] for row in bins.values())
        assert all(sha(row['executed']) == row['sha256'] for row in helpers.values())
        assert all(sha(S / path) == digest for path, digest in pins.items())
        assert sha(PERF) == PERF_HASH

    def run(name, args, env):
        STATE['stage'] = name
        started = time.monotonic()
        with (S / (name + '.stdout')).open('w') as out, (S / (name + '.stderr')).open('w') as err:
            result = subprocess.run(args, cwd=SOURCE, env=env, stdout=out, stderr=err)
        row = {'name': name, 'command': args, 'exit': result.returncode,
               'seconds': time.monotonic() - started, 'profile_gate': env.get(GATE)}
        commands.append(row)
        save('measurement-receipt.json', commands)
        print(json.dumps(row), flush=True)
        assert result.returncode == 0, name

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

    for rotation, order in enumerate([['control', 'off', 'on'], ['off', 'on', 'control'],
                                       ['on', 'control', 'off']], 1):
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
            save(name + '-input-before.json', before)
            quiet(name)
            if kind == 'on':
                env[GATE] = '1'
            binary = bins[kind]
            run(name, [binary['executed'], '--ui-check', '--mode', 'performance',
                       '--scenario', 'large-first-pause', '--project', str(fixture),
                       '--output', str(output)], env)
            STATE['stage'] = 'post-command validation'
            after = snapshot(fixture)
            save(name + '-input-after.json', after)
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
            parsed = None
            if kind == 'on':
                parsed = parser.parse_profile(stderr)
                assert parsed['block_count'] == 1, 'one actual insertion per fresh invocation'
                save(name + '-profile.json', parsed)
            else:
                assert PREFIX not in stderr, 'gate-off/control emitted profile output'
            row = {'rotation': rotation, 'kind': kind, 'binary_sha256': binary['sha256'],
                   'metrics': metrics, 'profile': parsed, 'checks': 7, 'kestrel_checks': 1,
                   'warnings': sum(len(c['findings']) for c in report['scenarios']),
                   'authored_input_unchanged': True, 'wal_empty': True}
            trials.append(row)
            save('trials.json', trials)
            guard()
    assert len(commands) == 18 and len(trials) == 9
    assert all(row['exit'] == 0 for row in commands)
    summary = {'diagnostic_only': True, 'n_per_kind': 3, 'kinds': {},
               'phase_accounting': 'exclusive; exact conservation; timer/helper overhead included',
               'no_percentile_target_or_memory_claim': True}
    for kind in ['control', 'off', 'on']:
        subset = [t for t in trials if t['kind'] == kind]
        summary['kinds'][kind] = {m: {'min': min(values), 'median': statistics.median(values), 'max': max(values)}
                                 for m in subset[0]['metrics']
                                 for values in [[t['metrics'][m] for t in subset]]}
    on = [t['profile']['selected'] for t in trials if t['kind'] == 'on']
    summary['exclusive_phases'] = {
        phase: {'min_ns': min(values), 'median_ns': statistics.median(values), 'max_ns': max(values)}
        for i, phase in enumerate(parser.PHASES)
        for values in [[block['phases'][i]['elapsed_ns'] for block in on]]}
    summary['worker_outer_ns'] = [block['elapsed_ns'] for block in on]
    save('summary.json', summary)
    save('measurement-terminal.json', {'status': 'complete', 'trials': len(trials),
                                      'commands': len(commands)})
    print(json.dumps({'terminal': True, 'trials': 9, 'commands': 18,
                      'workflow_checks': 63, 'kestrel_checks': 9}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        save('measurement-terminal.json', {'status': 'failed', **STATE,
                                          'exception': type(error).__name__, 'message': str(error)})
        raise
