"""Draft only: rotated timing lane followed by owned-observer memory lane."""
from pathlib import Path
import hashlib, json, os, statistics, subprocess, sys, time

S = Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
Q = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
MAIN = Path('/Users/michael/Code/deadpan')
KESTREL = Path('/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift')
CONTROL_SHA = 'a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f'
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
PERF_SHA = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
PROFILE_ENV = 'DEADPAN_CURRENT_LEAF_PROFILE'
SCENARIO_ARGS = ['--ui-check', '--mode', 'performance', '--scenario', 'large-first-pause',
                 '--kestrel-source', str(KESTREL)]
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}


def need(ok, msg):
    if not ok:
        raise RuntimeError(msg)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read_json(path):
    with Path(path).open() as f:
        return json.load(f)


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def snapshot(root):
    root = Path(root)
    return {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
            for p in sorted(root.rglob('*')) if p.is_file()}


def current_load():
    return list(os.getloadavg())


def process_observation():
    raw = subprocess.check_output(['ps', '-axo', 'pid=,comm=,args='], text=True)
    conflicts = []
    for line in raw.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) < 2:
            continue
        try:
            pid = int(fields[0])
        except ValueError:
            continue
        if pid == os.getpid():
            continue
        comm = Path(fields[1]).name.lower()
        if any(token in comm for token in ('cargo', 'rustc', 'deadpan', 'memory_observer', 'memory-observer')):
            conflicts.append({'pid': pid, 'comm': fields[1], 'args': fields[2] if len(fields) > 2 else ''})
    return conflicts


def power_observation():
    observations = {}
    for name, command in [('battery', ['pmset', '-g', 'batt']), ('thermal', ['pmset', '-g', 'therm'])]:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        observations[name] = {'command': command, 'exit': result.returncode,
                              'stdout': result.stdout, 'stderr': result.stderr}
    return observations


def quiet_wait(label):
    started = time.monotonic()
    samples = []
    while True:
        load = current_load()
        conflicts = process_observation()
        sample = {'elapsed_seconds': round(time.monotonic() - started, 3),
                  'load_average': load, 'conflicting_processes': conflicts}
        samples.append(sample)
        if load[0] <= 3.0 and not conflicts:
            result = {'label': label, 'waited_seconds': sample['elapsed_seconds'],
                      'samples': samples, 'power': power_observation(), 'ready': True}
            write_json(S / f'{label}-preflight.json', result)
            return result
        if time.monotonic() - started >= 180:
            result = {'label': label, 'waited_seconds': sample['elapsed_seconds'],
                      'samples': samples, 'power': power_observation(), 'ready': False}
            write_json(S / f'{label}-preflight.json', result)
            raise RuntimeError(f'{label}: quiet-process/load preflight did not clear within 180s')
        time.sleep(5)


def verify_frozen_guards(candidate_bins, control_bin, observer_bin=None):
    for path, digest in runner_frozen.items():
        need(sha(path) == digest, f'frozen runner/manifest changed: {path.name}')
    candidate_sources = read_json(S / 'instrumented-source-identities.json')
    original_sources = read_json(S / 'original-source-identities.json')
    base_sources = read_json(Q / 'candidate-all-core-source-identities.json')
    need(len(candidate_sources) == 2052, 'private source map must contain 2,052 Rust paths')
    need(len(original_sources) == 2051 and original_sources == base_sources,
         'profile baseline must equal the qualified 2,051-file production source map')
    main_paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=MAIN, text=True).splitlines()
    need({p: sha(MAIN / p) for p in main_paths} == original_sources,
         'main Rust paths/bytes differ from qualified baseline')
    need(set(candidate_sources) == set(original_sources) | {'crates/deadpan-core/src/leaf_profile.rs'},
         'private profile Rust path set is not the single temporary module')
    current = {str(p.relative_to(S / 'source')): sha(p)
               for p in (S / 'source').rglob('*.rs')}
    need(current == candidate_sources, 'profile source tree changed from frozen 2,052-file map')
    inputs = read_json(S / 'input-identities.json')
    qualified_inputs = read_json(Q / 'candidate-all-core-inputs.json')
    need(len(inputs) == 6 and inputs == qualified_inputs, 'six profile inputs differ from qualified production inputs')
    for rel, digest in inputs.items():
        need(sha(S / 'source' / rel) == digest, f'private input changed: {rel}')
        need(sha(MAIN / rel) == digest, f'main input changed: {rel}')
    need(sha(KESTREL) == read_json(Q / 'release-native-kestrel.json')['sha256'],
         'Kestrel source changed from qualified control')
    need(sha(PERF) == PERF_SHA, 'candidate-perf helper hash changed')
    for label, row in candidate_bins.items():
        p = Path(row.get('executed', row.get('path', '')))
        need(p.is_file() and sha(p) == row.get('sha256'), f'private executable changed: {label}')
    need(candidate_bins['deadpan-app'].get('sha256'), 'private release app hash missing')
    need(control_bin.get('sha256') == CONTROL_SHA, 'qualified release control app hash changed')
    cp = Path(control_bin['executed'])
    need(cp.is_file() and sha(cp) == CONTROL_SHA, 'qualified control binary copy changed')
    if observer_bin is not None:
        p = Path(observer_bin.get('executed', ''))
        need(p.is_file() and sha(p) == observer_bin.get('sha256'), 'owned memory observer binary changed')
        need(sha(Path(observer_bin['source'])) == observer_bin['source_sha256'],
             'owned observer source changed')
        need(sha(Path(observer_bin['adapter_rlib'])) == observer_bin['adapter_rlib_sha256'],
             'owned observer adapter rlib changed')
    return {'private_rust_paths': len(candidate_sources), 'baseline_rust_paths': len(original_sources),
            'input_paths': len(inputs), 'candidate_app_sha256': candidate_bins['deadpan-app']['sha256'],
            'control_app_sha256': control_bin['sha256'], 'perf_sha256': PERF_SHA,
            'observer_sha256': observer_bin.get('sha256') if observer_bin else None}


def resolve_observer_manifest():
    path = S / 'observer-binary.json'
    need(path.is_file(), 'observer-binary.json is not yet available; memory lane must not start')
    data = read_json(path)
    need('path' in data and 'sha256' in data and 'source_sha256' in data,
         'observer-binary.json does not match the actual observer producer')
    return dict(data, executed=data['path'])


def read_binary_manifests():
    candidate = read_json(S / 'binaries.json')
    qualified = read_json(Q / 'release-native-binaries.json')
    control = qualified['deadpan-app']
    need(candidate.get('deadpan-app', {}).get('freshly_built') is True,
         'profile release app must be marked freshly built')
    observer = resolve_observer_manifest()
    verify_frozen_guards(candidate, control, observer)
    return candidate, control, observer


def run_command(name, command, env, cwd):
    started = time.monotonic()
    with (S / f'{name}.stdout').open('wb') as out, (S / f'{name}.stderr').open('wb') as err:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=out, stderr=err)
    row = {'name': name, 'command': [str(x) for x in command], 'cwd': str(cwd),
           'exit': result.returncode, 'seconds': time.monotonic() - started}
    commands.append(row)
    write_json(S / 'measurement-commands.json', commands)
    if result.returncode != 0:
        raise RuntimeError(f'{name} exited {result.returncode}; preserve artifacts and stop')
    return row


def make_fresh_fixture(name, env):
    fixture = S / f'{name}.deadpan'
    need(not fixture.exists(), f'fixture already exists; refusing reuse: {fixture}')
    run_command(name + '-make', [str(PERF), 'make-large', str(fixture), '--beats', '10000'], env, S / 'source')
    before = snapshot(fixture)
    need('project.sqlite' in before, 'fresh fixture lacks project.sqlite')
    write_json(S / f'{name}-input-before.json', before)
    return fixture, before


def verify_fixture(before, after, name):
    without_sidecars = lambda x: {k: v for k, v in x.items() if k not in SIDECARS}
    need(without_sidecars(before) == without_sidecars(after), f'{name}: authored fixture paths/bytes changed')
    need(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0, f'{name}: SQLite WAL is not empty')
    need(before['project.sqlite']['sha256'] == after['project.sqlite']['sha256'],
         f'{name}: main authored DB changed')


def verify_report(report_path, binary_sha, fixture_sha, name):
    report = read_json(report_path)
    need(report.get('failed') is False, f'{name}: performance report failed')
    metadata = report.get('metadata', {})
    need(metadata.get('binary_sha256') == binary_sha and metadata.get('build_profile') == 'release',
         f'{name}: report app identity/profile mismatch')
    need(metadata.get('fixture_sha256') == fixture_sha, f'{name}: report fixture hash mismatch')
    inputs = read_json(S / 'input-identities.json')
    need(metadata.get('cargo_lock_sha256_at_run') == inputs.get('Cargo.lock'),
         f'{name}: report Cargo.lock identity mismatch')
    case = [x for x in report.get('scenarios', []) if x.get('name') == 'large-first-pause']
    kestrel = [x for x in report.get('scenarios', []) if x.get('name') == 'kestrel-shortcuts']
    need(len(case) == len(kestrel) == 1, f'{name}: required scenarios absent or duplicated')
    need(len(case[0].get('checks', [])) == 7 and all(x.get('passed') is True for x in case[0]['checks']),
         f'{name}: seven pause checks did not pass')
    need(len(kestrel[0].get('checks', [])) == 1 and all(x.get('passed') is True for x in kestrel[0]['checks']),
         f'{name}: Kestrel check did not pass')
    timings = {x['name']: x['summary']['completed']['p50_ms'] for x in case[0].get('timings', [])
               if x.get('name', '').startswith('large_first_pause')}
    need(set(timings) == {'large_first_pause_input_to_commit_ms',
                          'large_first_pause_input_to_offscreen_background_composition_ms'},
         f'{name}: expected two pause p50 timings')
    return report, timings


def parse_profile(name, expect_off):
    command = ['python', str(S / 'check_profile.py'), str(S / f'{name}.stderr')]
    if expect_off:
        command.append('--expect-off')
    run_command(name + '-parse', command, dict(os.environ), S)
    parsed = read_json(S / f'{name}-parse.stdout')
    if expect_off:
        need(parsed.get('profile_absent') is True, f'{name}: profile emitted while gate off')
    else:
        need(parsed.get('diagnostic_only') is True, f'{name}: profile parser did not accept exact block')
        need(parsed.get('counts') == {'app_service_total': 1, 'semantic_leaf_apply_call': 1,
                                      'replay_leaf_apply_call': 1, 'app_refresh_succeeded': 1},
             f'{name}: profile count contract mismatch')
    return parsed


def timed_trial(round_no, kind, candidate, control, env_base):
    name = f'timing-{round_no}-{kind}'
    verify_frozen_guards(candidate, control)
    fixture, before = make_fresh_fixture(name, env_base)
    preflight = quiet_wait(name)
    verify_frozen_guards(candidate, control)
    app = control if kind == 'control' else candidate['deadpan-app']
    env = dict(env_base)
    if kind == 'on':
        env[PROFILE_ENV] = '1'
    else:
        env.pop(PROFILE_ENV, None)
    output = S / f'{name}-report'
    need(not output.exists(), f'{name}: report output already exists; refusing rerun')
    run_command(name, [app['executed'], *SCENARIO_ARGS, '--project', str(fixture), '--output', str(output)], env, S)
    after = snapshot(fixture)
    write_json(S / f'{name}-input-after.json', after)
    verify_fixture(before, after, name)
    report, metrics = verify_report(output / 'report.json', app['sha256'], before['project.sqlite']['sha256'], name)
    parsed = parse_profile(name, expect_off=(kind != 'on'))
    verify_frozen_guards(candidate, control)
    need(sha(PERF) == PERF_SHA, 'candidate-perf helper changed during timing trial')
    return {'lane': 'timing', 'round': round_no, 'kind': kind, 'metrics': metrics,
            'profile': parsed, 'load_average': preflight['samples'][-1]['load_average'],
            'process_power_observation': f'{name}-preflight.json', 'checks': 7, 'kestrel_checks': 1,
            'source_binary_input_guards': True, 'authored_paths_bytes_unchanged': True,
            'wal_empty': True, 'binary_sha256': app['sha256'],
            'fixture_sha256': report['metadata']['fixture_sha256']}


def sample_summary(path, receipt):
    rows = read_json(path)
    need(isinstance(rows, list) and len(rows) > 0, 'memory samples missing or empty')
    need(len(rows) <= 60000 and receipt.get('sample_count') == len(rows), 'sample count mismatch/bound')
    last = -1
    for row in rows:
        need(set(row) == {'elapsed_ns', 'query_ns', 'phys_footprint_bytes', 'resident_size_bytes'},
             'unexpected memory sample shape')
        for key in row:
            need(type(row[key]) is int and row[key] >= 0, f'invalid memory sample {key}')
        need(row['elapsed_ns'] > last, 'sample elapsed timestamps are not strictly increasing')
        last = row['elapsed_ns']
    footprint = max(row['phys_footprint_bytes'] for row in rows)
    resident = max(row['resident_size_bytes'] for row in rows)
    need(receipt.get('observed_max_phys_footprint_bytes') == footprint and
         receipt.get('observed_max_resident_size_bytes') == resident,
         'observer receipt maxima do not recompute from raw samples')
    max_gap = max((b['elapsed_ns'] - a['elapsed_ns'] for a, b in zip(rows, rows[1:])), default=0)
    need(receipt.get('first_sample_start_elapsed_ns') == rows[0]['elapsed_ns'] and
         receipt.get('last_sample_start_elapsed_ns') == rows[-1]['elapsed_ns'] and
         receipt.get('maximum_sample_start_gap_ns') == max_gap and
         receipt.get('sample_query_sum_ns') == sum(r['query_ns'] for r in rows),
         'observer receipt sample clocks/gaps/query sum do not recompute')
    window = receipt.get('sampling_window_elapsed_ns')
    need(type(window) is int and window >= rows[-1]['elapsed_ns'] + rows[-1]['query_ns'],
         'observer sampling window does not contain its last query')
    return {'sample_count': len(rows), 'observed_max_phys_footprint_bytes': footprint,
            'observed_max_resident_size_bytes': resident,
            'first_sample_start_elapsed_ns': rows[0]['elapsed_ns'],
            'last_sample_start_elapsed_ns': rows[-1]['elapsed_ns'],
            'sampling_window_elapsed_ns': receipt.get('sampling_window_elapsed_ns'),
            'maximum_sample_start_gap_ns': receipt.get('maximum_sample_start_gap_ns')}


def memory_trial(round_no, kind, candidate, control, observer, env_base):
    name = f'memory-{round_no}-{kind}'
    verify_frozen_guards(candidate, control, observer)
    fixture, before = make_fresh_fixture(name, env_base)
    preflight = quiet_wait(name)
    verify_frozen_guards(candidate, control, observer)
    app = control if kind == 'control' else candidate['deadpan-app']
    env = dict(env_base)
    if kind == 'on':
        env[PROFILE_ENV] = '1'
    else:
        env.pop(PROFILE_ENV, None)
    receipt_path = S / f'{name}-observer-receipt.json'
    samples_path = S / f'{name}-memory-samples.json'
    app_stdout = S / f'{name}-app.stdout'
    app_stderr = S / f'{name}-app.stderr'
    report = S / f'{name}-report'
    for output_path in (receipt_path, samples_path, app_stdout, app_stderr, report):
        need(not output_path.exists(), f'{name}: output already exists; refusing rerun: {output_path.name}')
    app_args = [*SCENARIO_ARGS, '--project', str(fixture), '--output', str(report)]
    cmd = [observer['executed'], '--receipt', str(receipt_path), '--samples', str(samples_path),
           '--stdout', str(app_stdout), '--stderr', str(app_stderr), '--', app['executed'], *app_args]
    run_command(name + '-observer', cmd, env, S)
    after = snapshot(fixture)
    write_json(S / f'{name}-input-after.json', after)
    verify_fixture(before, after, name)
    receipt = read_json(receipt_path)
    need(receipt.get('terminal') is True and receipt.get('valid_memory_trial') is True and
         receipt.get('reason') == 'exited', f'{name}: observer receipt not a valid terminal run')
    need(receipt.get('group_teardown_confirmed') is True and receipt.get('reap_attempted') is True and
         receipt.get('reaped') is True and receipt.get('app_exit_code') == 0 and receipt.get('errors') == [],
         f'{name}: owned-process cleanup/app exit failed')
    need(type(receipt.get('owned_app_pid')) is int and receipt['owned_app_pid'] > 0 and
         receipt.get('leader_exit_confirmed') is True and receipt.get('cleanup_unconfirmed') is False and
         receipt.get('app_exit_signal') is None, f'{name}: observer ownership/exit fields invalid')
    need(receipt.get('requested_interval_ms') == 5 and receipt.get('maximum_runtime_seconds') == 180 and
         receipt.get('maximum_samples') == 60000, f'{name}: observer limits differ from reviewed schema')
    need(receipt.get('sampled_lower_bound_only') is True and receipt.get('whole_app_lifetime_only') is True and
         receipt.get('per_leaf_memory') is False and receipt.get('memory_ceiling_qualified') is False,
         f'{name}: observer memory-scope claims changed')
    samples = sample_summary(samples_path, receipt)
    need(receipt.get('samples_written') is True and receipt.get('sampled_lower_bound_only') is True,
         f'{name}: observer did not preserve the bounded sample output')
    perf_report, metrics = verify_report(report / 'report.json', app['sha256'], before['project.sqlite']['sha256'], name)
    parsed = parse_profile_from_stderr(name, app_stderr, expect_off=(kind != 'on'))
    verify_frozen_guards(candidate, control, observer)
    need(sha(PERF) == PERF_SHA, 'candidate-perf helper changed during memory trial')
    return {'lane': 'owned-observer-memory', 'round': round_no, 'kind': kind,
            'timing_metrics_observer_contaminated_do_not_pool': metrics, 'profile': parsed,
            'memory': samples, 'load_average': preflight['samples'][-1]['load_average'],
            'process_power_observation': f'{name}-preflight.json', 'checks': 7, 'kestrel_checks': 1,
            'source_binary_input_observer_guards': True, 'authored_paths_bytes_unchanged': True,
            'wal_empty': True, 'binary_sha256': app['sha256'], 'observer_sha256': observer['sha256'],
            'fixture_sha256': perf_report['metadata']['fixture_sha256'],
            'memory_interpretation': 'sampled whole-app lifetime lower bound; not exact peak or phase attribution'}


def parse_profile_from_stderr(name, stderr_path, expect_off):
    command = ['python', str(S / 'check_profile.py'), str(stderr_path)]
    if expect_off:
        command.append('--expect-off')
    run_command(name + '-parse', command, dict(os.environ), S)
    parsed = read_json(S / f'{name}-parse.stdout')
    if expect_off:
        need(parsed.get('profile_absent') is True, f'{name}: profile emitted while gate off')
    else:
        need(parsed.get('diagnostic_only') is True, f'{name}: profile parser did not accept exact block')
        need(parsed.get('counts') == {'app_service_total': 1, 'semantic_leaf_apply_call': 1,
                                      'replay_leaf_apply_call': 1, 'app_refresh_succeeded': 1},
             f'{name}: profile count contract mismatch')
    return parsed


def summarize(rows, kinds, lane):
    summary = {'lane': lane, 'diagnostic_only': True, 'no_percentile_or_target_pass': True,
               'n_per_kind': 3, 'kinds': {}}
    for kind in kinds:
        selected = [r for r in rows if r['kind'] == kind]
        need(len(selected) == 3, f'{lane}/{kind}: expected three trials')
        summary['kinds'][kind] = {}
        if lane == 'timing':
            metric_names = set(selected[0]['metrics'])
            for metric in sorted(metric_names):
                vals = [r['metrics'][metric] for r in selected]
                summary['kinds'][kind][metric] = {'min': min(vals), 'median': statistics.median(vals), 'max': max(vals)}
    if lane == 'timing':
        on = [r['profile']['nanoseconds'] for r in rows if r['kind'] == 'on']
        summary['on_profile_phase_ms'] = {phase: {'min': min(v)/1e6, 'median': statistics.median(v)/1e6,
                                                   'max': max(v)/1e6}
                                          for phase in on[0] for v in [[r[phase] for r in on]]}
        residuals = [r['profile']['other_service_cleanup_and_overhead_ns'] for r in rows if r['kind'] == 'on']
        summary['on_other_service_cleanup_and_overhead_ms'] = {
            'min': min(residuals)/1e6, 'median': statistics.median(residuals)/1e6,
            'max': max(residuals)/1e6}
        summary['instrumentation_overhead_qualified'] = False
        summary['queue_wait_measured'] = False
    else:
        summary['memory_is_separate_from_timing_lane'] = True
        for kind in kinds:
            selected = [r['memory'] for r in rows if r['kind'] == kind]
            summary['kinds'][kind]['sampled_max_phys_footprint_bytes'] = {
                'min': min(x['observed_max_phys_footprint_bytes'] for x in selected),
                'median': statistics.median(x['observed_max_phys_footprint_bytes'] for x in selected),
                'max': max(x['observed_max_phys_footprint_bytes'] for x in selected)}
            summary['kinds'][kind]['sampled_max_resident_size_bytes'] = {
                'min': min(x['observed_max_resident_size_bytes'] for x in selected),
                'median': statistics.median(x['observed_max_resident_size_bytes'] for x in selected),
                'max': max(x['observed_max_resident_size_bytes'] for x in selected)}
        summary['memory_measurement'] = 'sampled lower bounds over whole main-app lifetime; observer load; not peak/phase/ceiling'
    return summary


commands = []
runner_frozen = {}

def main():
    need(not (S / 'measurement-commands.json').exists() and
         not (S / 'timing-trials.json').exists() and not (S / 'memory-trials.json').exists(),
         'measurement artifacts already exist; refusing to restart or overwrite the driver')
    for path in [Path(__file__), S / 'check_profile.py', S / 'schema.json',
                 S / 'instrumented-source-identities.json', S / 'original-source-identities.json',
                 S / 'input-identities.json', S / 'binaries.json', S / 'observer-binary.json']:
        runner_frozen[path] = sha(path)
    write_json(S / 'measurement-owner.json', {'pid': os.getpid(), 'script': str(Path(__file__)),
                                             'started_unix': time.time(), 'frozen_runners': {
                                                 str(p): h for p, h in runner_frozen.items()}})
    need(sha(PERF) == PERF_SHA, 'candidate-perf helper SHA mismatch')
    candidate, control, observer = read_binary_manifests()
    env = dict(os.environ)
    env.pop(PROFILE_ENV, None)
    timing_rows = []
    timing_order = [['control', 'off', 'on'], ['off', 'on', 'control'], ['on', 'control', 'off']]
    for round_no, order in enumerate(timing_order, 1):
        for kind in order:
            timing_rows.append(timed_trial(round_no, kind, candidate, control, env))
            write_json(S / 'timing-trials.json', timing_rows)
    write_json(S / 'timing-summary.json', summarize(timing_rows, ['control', 'off', 'on'], 'timing'))

    memory_rows = []
    memory_order = [['control', 'off', 'on'], ['off', 'on', 'control'], ['on', 'control', 'off']]
    for round_no, order in enumerate(memory_order, 1):
        for kind in order:
            memory_rows.append(memory_trial(round_no, kind, candidate, control, observer, env))
            write_json(S / 'memory-trials.json', memory_rows)
    write_json(S / 'memory-summary.json', summarize(memory_rows, ['control', 'off', 'on'], 'memory'))
    expected_order = [['control', 'off', 'on'], ['off', 'on', 'control'], ['on', 'control', 'off']]
    for lane_rows in (timing_rows, memory_rows):
        need([(r['round'], r['kind']) for r in lane_rows] ==
             [(round_no, kind) for round_no, order in enumerate(expected_order, 1) for kind in order],
             'triplet order differs from the planned rotation')
    verify_frozen_guards(candidate, control, observer)
    print(json.dumps({'done': True, 'timing_trials': len(timing_rows), 'memory_trials': len(memory_rows),
                      'timing_summary': str(S / 'timing-summary.json'),
                      'memory_summary': str(S / 'memory-summary.json')}), flush=True)


if __name__ == '__main__':
    main()
