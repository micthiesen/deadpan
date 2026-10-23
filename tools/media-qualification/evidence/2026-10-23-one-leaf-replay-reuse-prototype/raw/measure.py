"""Unexecuted guarded measurement driver draft for one-leaf replay reuse.

Root must add MEASUREMENT-GO.json after the complete private release build and
focused checks are terminal. This script never builds code or reuses fixtures.
"""
from pathlib import Path
import hashlib
import json
import os
import statistics
import subprocess
import time

S = Path('/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23')
Q = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
PROFILE_Q = Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
MAIN = Path('/Users/michael/Code/deadpan')
KESTREL = Path('/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift')
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
PERF_SHA = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
CONTROL_SHA = 'a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f'
OBSERVER_SHA = 'd1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2'
OBSERVER_MANIFEST = PROFILE_Q / 'observer-binary.json'
SOURCE_COMMIT = '3a32c923358c0cb7ca53f821bfb6db3e01ebb466'
BASE_RUST_COUNT = 2051
PRIVATE_RUST_COUNT = 2053
CHANGED_RUST_COUNT = 14
INPUT_NAMES = {
    'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
    'crates/deadpan-app/src/navigation/registry/baseline-routing.txt',
    'docs/COMMANDS.md',
}
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
PROFILE_PREFIX = b'DEADPAN_CURRENT_LEAF_PROFILE'
PROFILE_ENV = 'DEADPAN_CURRENT_LEAF_PROFILE'
SCENARIO_ARGS = [
    '--ui-check', '--mode', 'performance', '--scenario', 'large-first-pause',
    '--kestrel-source', str(KESTREL),
]
ORDER = [
    ['control', 'candidate'],
    ['candidate', 'control'],
    ['control', 'candidate'],
    ['candidate', 'control'],
    ['control', 'candidate'],
]
commands = []
runner_frozen = {}


def need(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def snapshot(root):
    root = Path(root)
    return {
        str(path.relative_to(root)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
        for path in sorted(root.rglob('*')) if path.is_file()
    }


def no_profile_marker(path):
    total = 0
    with Path(path).open('rb') as stream:
        while raw := stream.readline(65_537):
            need(len(raw) <= 65_536, f'{path}: stderr line exceeded parser bound')
            total += len(raw)
            need(total <= 64 * 1024 * 1024, f'{path}: stderr exceeded parser bound')
            need(PROFILE_PREFIX not in raw, f'{path}: disabled profiler marker appeared')
    return {'prefix_absent': True, 'stderr_bytes': total}


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
        if any(token in comm for token in (
            'cargo', 'rustc', 'deadpan', 'memory_observer', 'memory-observer'
        )):
            conflicts.append({
                'pid': pid, 'comm': fields[1],
                'args': fields[2] if len(fields) > 2 else '',
            })
    return conflicts


def power_observation():
    results = {}
    for name, command in (
        ('battery', ['pmset', '-g', 'batt']),
        ('thermal', ['pmset', '-g', 'therm']),
    ):
        result = subprocess.run(
            command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        results[name] = {
            'command': command, 'exit': result.returncode,
            'stdout': result.stdout, 'stderr': result.stderr,
        }
    return results


def quiet_wait(label):
    started = time.monotonic()
    samples = []
    while True:
        load = list(os.getloadavg())
        conflicts = process_observation()
        sample = {
            'elapsed_seconds': round(time.monotonic() - started, 3),
            'load_average': load, 'conflicting_processes': conflicts,
        }
        samples.append(sample)
        if load[0] <= 3.0 and not conflicts:
            result = {
                'label': label, 'waited_seconds': sample['elapsed_seconds'],
                'samples': samples, 'power': power_observation(), 'ready': True,
            }
            write_json(S / f'{label}-preflight.json', result)
            return result
        if time.monotonic() - started >= 180:
            result = {
                'label': label, 'waited_seconds': sample['elapsed_seconds'],
                'samples': samples, 'power': power_observation(), 'ready': False,
            }
            write_json(S / f'{label}-preflight.json', result)
            raise RuntimeError(f'{label}: quiet-process/load gate did not clear')
        time.sleep(5)


def source_maps():
    base = read_json(S / 'original-source-identities.json')
    candidate = read_json(S / 'formatted-source-identities.json')
    audit = read_json(S / 'formatted-overlay-audit.json')
    inputs = read_json(S / 'input-identities.json')
    basis = read_json(S / 'source-basis.json')
    qualified_base = read_json(Q / 'candidate-all-core-source-identities.json')
    qualified_inputs = read_json(Q / 'candidate-all-core-inputs.json')
    need(basis.get('commit') == SOURCE_COMMIT and basis.get('rust_files') == BASE_RUST_COUNT,
         'frozen source basis is not the specified baseline')
    need(len(base) == BASE_RUST_COUNT and base == qualified_base,
         'baseline map differs from qualified 2,051-file production baseline')
    need(len(candidate) == PRIVATE_RUST_COUNT and len(audit.get('changed', [])) == CHANGED_RUST_COUNT,
         'private path count or formatted overlay count differs')
    need(audit.get('inputs_unchanged') is True and audit.get('nonowned_rust_unchanged') == 2039
         and not any('leaf_profile' in path for path in candidate),
         'formatted overlay audit or no-instrumentation source scope differs')
    need({path for path, digest in candidate.items() if base.get(path) != digest}
         == set(audit['changed']), 'candidate source map differs outside reviewed 14-file overlay')
    live = {
        str(path.relative_to(S / 'source')): sha(path)
        for path in (S / 'source').rglob('*.rs')
    }
    need(live == candidate, 'private source tree no longer matches final formatted map')
    main_paths = subprocess.check_output(
        ['rg', '--files', '-g', '*.rs'], cwd=MAIN, text=True,
    ).splitlines()
    main_map = {path: sha(MAIN / path) for path in main_paths}
    need(len(main_map) == BASE_RUST_COUNT and main_map == base,
         'main checkout no longer matches the 2,051-file baseline')
    need(set(inputs) == INPUT_NAMES and inputs == qualified_inputs,
         'six frozen inputs differ from the qualified baseline')
    for rel, digest in inputs.items():
        need(sha(S / 'source' / rel) == digest, f'private frozen input changed: {rel}')
        need(sha(MAIN / rel) == digest, f'main checkout input changed: {rel}')
    return base, candidate, inputs, basis, audit


def binaries_and_guards():
    candidate = read_json(S / 'binaries.json')
    qualified = read_json(Q / 'release-native-binaries.json')
    control = qualified['deadpan-app']
    observer = read_json(OBSERVER_MANIFEST)
    need(observer.get('sha256') == OBSERVER_SHA, 'memory observer differs from qualified diagnostic binary')
    need(candidate['deadpan-app'].get('freshly_built') is True,
         'private release app is not marked freshly built')
    need(control.get('sha256') == CONTROL_SHA, 'qualified control hash changed')
    need(sha(control['executed']) == CONTROL_SHA, 'qualified control executable changed')
    for name, row in candidate.items():
        executable = Path(row['executed'])
        need(executable.is_file() and sha(executable) == row['sha256'],
             f'candidate binary identity mismatch: {name}')
    for name in ('deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track'):
        row = qualified[name]
        need(Path(candidate[name]['executed']).is_file()
             and sha(candidate[name]['executed']) == row['sha256'] == candidate[name]['sha256'],
             f'qualified helper reuse mismatch: {name}')
    observer_path = Path(observer['path'])  # producer's actual manifest key
    need(observer_path.is_file() and sha(observer_path) == observer['sha256'],
         'reused memory-observer executable mismatch')
    need(Path(observer['source']).is_file()
         and sha(observer['source']) == observer['source_sha256'],
         'reused memory-observer source mismatch')
    need(Path(observer['adapter_rlib']).is_file()
         and sha(observer['adapter_rlib']) == observer['adapter_rlib_sha256'],
         'reused memory-observer adapter mismatch')
    need(sha(PERF) == PERF_SHA, 'qualified fresh-fixture helper hash changed')
    kestrel_hash = read_json(Q / 'release-native-kestrel.json')['sha256']
    need(sha(KESTREL) == kestrel_hash, 'Kestrel source differs from qualified baseline')
    return candidate, control, dict(observer, path=str(observer_path)), qualified


def freeze_runners(candidate):
    paths = [
        Path(__file__), S / 'original-source-identities.json',
        S / 'formatted-source-identities.json', S / 'formatted-overlay-audit.json',
        S / 'input-identities.json', S / 'source-basis.json',
        S / 'binaries.json', S / 'test-binaries.json',
        S / 'build-receipt.json', S / 'independent-review.json',
        S / 'test-warning-correction-review.json', S / 'test-warning-independent-review.json',
        S / 'build_prototype_resumed.py',
        S / 'MEASUREMENT-GO.json',
        OBSERVER_MANIFEST, Q / 'release-native-binaries.json',
        Q / 'candidate-all-core-source-identities.json',
        Q / 'candidate-all-core-inputs.json',
        Q / 'release-native-kestrel.json',
    ]
    for path in paths:
        need(path.is_file(), f'frozen runner/manifest absent: {path}')
        runner_frozen[path] = sha(path)
    runner_frozen[PERF] = sha(PERF)
    runner_frozen[KESTREL] = sha(KESTREL)
    for name, row in candidate.items():
        runner_frozen[Path(row['executed'])] = row['sha256']
    observer = read_json(OBSERVER_MANIFEST)
    runner_frozen[Path(observer['path'])] = observer['sha256']
    runner_frozen[Path(observer['source'])] = observer['source_sha256']
    runner_frozen[Path(observer['adapter_rlib'])] = observer['adapter_rlib_sha256']


def verify_frozen(candidate, control, observer):
    for path, digest in runner_frozen.items():
        need(sha(path) == digest, f'frozen measurement input changed: {Path(path).name}')
    source_maps()
    need(sha(control['executed']) == CONTROL_SHA, 'control binary changed during trials')
    for row in candidate.values():
        need(sha(row['executed']) == row['sha256'], 'candidate/helper binary changed during trials')
    need(sha(observer['path']) == observer['sha256'], 'memory observer changed during trials')
    need(sha(PERF) == PERF_SHA, 'fixture helper changed during trials')


def run_command(name, command, env, cwd):
    started = time.monotonic()
    with (S / f'{name}.stdout').open('wb') as stdout, (S / f'{name}.stderr').open('wb') as stderr:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=stdout, stderr=stderr)
    row = {
        'name': name, 'command': [str(x) for x in command], 'cwd': str(cwd),
        'exit': result.returncode, 'seconds': time.monotonic() - started,
    }
    commands.append(row)
    write_json(S / 'measurement-commands.json', commands)
    if result.returncode != 0:
        raise RuntimeError(f'{name} exited {result.returncode}; preserve evidence and stop')
    return row


def make_fixture(name, env):
    fixture = S / f'{name}.deadpan'
    need(not fixture.exists(), f'{name}: fixture already exists; refusing reuse')
    run_command(name + '-make', [str(PERF), 'make-large', str(fixture), '--beats', '10000'],
                env, S / 'source')
    before = snapshot(fixture)
    need('project.sqlite' in before, f'{name}: fixture lacks main database')
    write_json(S / f'{name}-input-before.json', before)
    return fixture, before


def verify_fixture(before, after, name):
    without_sidecars = lambda m: {k: v for k, v in m.items() if k not in SIDECARS}
    need(without_sidecars(before) == without_sidecars(after),
         f'{name}: authored fixture paths/bytes changed')
    need(before['project.sqlite']['sha256'] == after['project.sqlite']['sha256'],
         f'{name}: main authored database changed')
    need(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0,
         f'{name}: SQLite WAL is nonempty')


def validate_report(report_path, app_hash, fixture_hash, name, inputs):
    report = read_json(report_path)
    need(report.get('failed') is False, f'{name}: report failed')
    meta = report.get('metadata', {})
    need(meta.get('build_profile') == 'release' and meta.get('binary_sha256') == app_hash,
         f'{name}: report binary/profile identity mismatch')
    need(meta.get('fixture_sha256') == fixture_hash, f'{name}: report fixture hash mismatch')
    need(meta.get('cargo_lock_sha256_at_run') == inputs['Cargo.lock'],
         f'{name}: report Cargo.lock identity mismatch')
    need(meta.get('physical_display_measured') is False,
         f'{name}: report claims physical-display measurement')
    need(str(meta.get('rust', '')).startswith('rustc 1.97.1 '),
         f'{name}: source-directory runtime toolchain differs from pinned compiler')
    pauses = [x for x in report.get('scenarios', []) if x.get('name') == 'large-first-pause']
    kestrel = [x for x in report.get('scenarios', []) if x.get('name') == 'kestrel-shortcuts']
    need(len(pauses) == len(kestrel) == 1, f'{name}: required report scenarios absent/repeated')
    need(len(pauses[0].get('checks', [])) == 7
         and all(x.get('passed') is True for x in pauses[0]['checks']),
         f'{name}: seven pause checks did not pass')
    need(len(kestrel[0].get('checks', [])) == 1
         and kestrel[0]['checks'][0].get('passed') is True,
         f'{name}: Kestrel check did not pass')
    metrics = {
        x['name']: x['summary']['completed']['p50_ms']
        for x in pauses[0].get('timings', [])
        if x.get('name', '').startswith('large_first_pause')
    }
    need(set(metrics) == {
        'large_first_pause_input_to_commit_ms',
        'large_first_pause_input_to_offscreen_background_composition_ms',
    }, f'{name}: expected two large-pause metrics missing')
    return report, metrics


def app_env():
    env = dict(os.environ)
    env.pop(PROFILE_ENV, None)
    need(PROFILE_ENV not in env, 'profile instrumentation environment must be absent')
    return env


def timed_trial(pair, kind, candidate, control, observer, inputs):
    name = f'timing-{pair}-{kind}'
    verify_frozen(candidate, control, observer)
    fixture, before = make_fixture(name, app_env())
    preflight = quiet_wait(name)
    verify_frozen(candidate, control, observer)
    row = control if kind == 'control' else candidate['deadpan-app']
    report_dir = S / f'{name}-report'
    need(not report_dir.exists(), f'{name}: report path already exists; refusing repeat')
    run_command(name, [row['executed'], *SCENARIO_ARGS, '--project', str(fixture),
                       '--output', str(report_dir)], app_env(), S / 'source')
    after = snapshot(fixture)
    write_json(S / f'{name}-input-after.json', after)
    verify_fixture(before, after, name)
    prefix = no_profile_marker(S / f'{name}.stderr')
    report, metrics = validate_report(report_dir / 'report.json', row['sha256'],
                                      before['project.sqlite']['sha256'], name, inputs)
    verify_frozen(candidate, control, observer)
    return {
        'lane': 'unobserved-timing', 'pair': pair, 'kind': kind,
        'metrics': metrics, 'profile_marker': prefix,
        'load_average': preflight['samples'][-1]['load_average'],
        'power_process_observation': f'{name}-preflight.json',
        'pause_checks': 7, 'kestrel_checks': 1,
        'source_binary_input_guards': True, 'authored_paths_bytes_unchanged': True,
        'wal_empty': True, 'binary_sha256': row['sha256'],
        'fixture_sha256': report['metadata']['fixture_sha256'],
    }


def sample_summary(samples_path, receipt):
    rows = read_json(samples_path)
    need(isinstance(rows, list) and rows and len(rows) <= 60000,
         'observer sample payload missing, empty, or over bound')
    need(receipt.get('terminal') is True and receipt.get('valid_memory_trial') is True
         and receipt.get('reason') == 'exited', 'observer did not report valid terminal run')
    need(receipt.get('requested_interval_ms') == 5
         and receipt.get('maximum_runtime_seconds') == 180
         and receipt.get('maximum_samples') == 60000, 'observer limits differ from reviewed values')
    need(receipt.get('group_teardown_confirmed') is True
         and receipt.get('leader_exit_confirmed') is True
         and receipt.get('reap_attempted') is True and receipt.get('reaped') is True
         and receipt.get('cleanup_unconfirmed') is False
         and receipt.get('app_exit_code') == 0 and receipt.get('app_exit_signal') is None
         and receipt.get('errors') == [], 'owned-child cleanup or app exit was not verified')
    need(type(receipt.get('owned_app_pid')) is int and receipt['owned_app_pid'] > 0,
         'observer did not record the owned app PID')
    need(receipt.get('samples_written') is True
         and receipt.get('sampled_lower_bound_only') is True
         and receipt.get('whole_app_lifetime_only') is True
         and receipt.get('per_leaf_memory') is False
         and receipt.get('memory_ceiling_qualified') is False,
         'observer memory scope changed or overclaims')
    expected_fields = {'elapsed_ns', 'query_ns', 'phys_footprint_bytes', 'resident_size_bytes'}
    previous = -1
    gaps = []
    for row in rows:
        need(set(row) == expected_fields and all(type(row[k]) is int and row[k] >= 0 for k in expected_fields),
             'memory sample shape/value invalid')
        need(row['elapsed_ns'] > previous, 'memory sample clock is not strictly increasing')
        if previous >= 0:
            gaps.append(row['elapsed_ns'] - previous)
        previous = row['elapsed_ns']
    count = len(rows)
    footprint = max(x['phys_footprint_bytes'] for x in rows)
    resident = max(x['resident_size_bytes'] for x in rows)
    first, last = rows[0]['elapsed_ns'], rows[-1]['elapsed_ns']
    max_gap = max(gaps, default=0)
    query_sum = sum(x['query_ns'] for x in rows)
    window = receipt.get('sampling_window_elapsed_ns')
    need(receipt.get('sample_count') == count
         and receipt.get('observed_max_phys_footprint_bytes') == footprint
         and receipt.get('observed_max_resident_size_bytes') == resident
         and receipt.get('first_sample_start_elapsed_ns') == first
         and receipt.get('last_sample_start_elapsed_ns') == last
         and receipt.get('maximum_sample_start_gap_ns') == max_gap
         and receipt.get('sample_query_sum_ns') == query_sum,
         'sample counts/clocks/gap/query sum/maxima do not match observer receipt')
    need(type(window) is int and window >= last + rows[-1]['query_ns'],
         'observer sampling window excludes the last query')
    return {
        'sample_count': count, 'sampled_max_phys_footprint_bytes': footprint,
        'sampled_max_resident_size_bytes': resident,
        'first_sample_start_elapsed_ns': first, 'last_sample_start_elapsed_ns': last,
        'maximum_sample_start_gap_ns': max_gap, 'sample_query_sum_ns': query_sum,
        'sampling_window_elapsed_ns': window,
    }


def memory_trial(pair, kind, candidate, control, observer, inputs):
    name = f'memory-{pair}-{kind}'
    verify_frozen(candidate, control, observer)
    fixture, before = make_fixture(name, app_env())
    preflight = quiet_wait(name)
    verify_frozen(candidate, control, observer)
    row = control if kind == 'control' else candidate['deadpan-app']
    receipt_path = S / f'{name}-observer-receipt.json'
    samples_path = S / f'{name}-memory-samples.json'
    app_stdout = S / f'{name}-app.stdout'
    app_stderr = S / f'{name}-app.stderr'
    report_dir = S / f'{name}-report'
    for path in (receipt_path, samples_path, app_stdout, app_stderr, report_dir):
        need(not path.exists(), f'{name}: output already exists; refusing repeat')
    args = [*SCENARIO_ARGS, '--project', str(fixture), '--output', str(report_dir)]
    command = [
        observer['path'], '--receipt', str(receipt_path), '--samples', str(samples_path),
        '--stdout', str(app_stdout), '--stderr', str(app_stderr), '--',
        row['executed'], *args,
    ]
    run_command(name + '-observer', command, app_env(), S / 'source')
    after = snapshot(fixture)
    write_json(S / f'{name}-input-after.json', after)
    verify_fixture(before, after, name)
    receipt = read_json(receipt_path)
    memory = sample_summary(samples_path, receipt)
    profile_marker = no_profile_marker(app_stderr)
    report, metrics = validate_report(report_dir / 'report.json', row['sha256'],
                                      before['project.sqlite']['sha256'], name, inputs)
    verify_frozen(candidate, control, observer)
    return {
        'lane': 'owned-observer-memory', 'pair': pair, 'kind': kind,
        'memory': memory,
        'timing_metrics_observer_contaminated_do_not_pool': metrics,
        'profile_marker': profile_marker,
        'load_average': preflight['samples'][-1]['load_average'],
        'power_process_observation': f'{name}-preflight.json',
        'pause_checks': 7, 'kestrel_checks': 1,
        'source_binary_input_observer_guards': True,
        'authored_paths_bytes_unchanged': True, 'wal_empty': True,
        'binary_sha256': row['sha256'], 'observer_sha256': observer['sha256'],
        'fixture_sha256': report['metadata']['fixture_sha256'],
        'memory_scope': 'sampled whole-app lifetime lower bound; not peak or phase attribution',
    }


def summarize_timing(rows):
    result = {
        'lane': 'unobserved-timing', 'diagnostic_only': True,
        'no_percentile_or_target_pass': True, 'n_pairs': 5,
        'kinds': {},
    }
    for kind in ('control', 'candidate'):
        selected = [row['metrics'] for row in rows if row['kind'] == kind]
        need(len(selected) == 5, f'{kind}: expected five timing trials')
        result['kinds'][kind] = {}
        for metric in selected[0]:
            values = [row[metric] for row in selected]
            result['kinds'][kind][metric] = {
                'min': min(values), 'median': statistics.median(values), 'max': max(values),
            }
    return result


def summarize_memory(rows):
    result = {
        'lane': 'owned-observer-memory', 'diagnostic_only': True,
        'no_percentile_or_target_pass': True, 'n_pairs': 5,
        'separate_from_unobserved_timing_lane': True, 'kinds': {},
        'interpretation': 'sampled whole-app lifetime lower bounds; no exact peak, per-leaf, or ceiling claim',
    }
    for kind in ('control', 'candidate'):
        selected = [row['memory'] for row in rows if row['kind'] == kind]
        need(len(selected) == 5, f'{kind}: expected five memory trials')
        result['kinds'][kind] = {}
        for field in ('sampled_max_phys_footprint_bytes', 'sampled_max_resident_size_bytes'):
            values = [sample[field] for sample in selected]
            result['kinds'][kind][field] = {
                'min': min(values), 'median': statistics.median(values), 'max': max(values),
            }
    return result


def main():
    need(not (S / 'measurement-commands.json').exists()
         and not (S / 'timing-trials.json').exists()
         and not (S / 'memory-trials.json').exists(),
         'measurement outputs already exist; refusing restart or overwrite')
    gate = read_json(S / 'MEASUREMENT-GO.json')
    need(gate.get('authorized') is True, 'Root MEASUREMENT-GO.json is absent or unauthorized')
    build = read_json(S / 'build-receipt.json')
    build_names = [
        'rustc-toolchain', 'cargo-toolchain', 'format-check', 'core-refutations',
        'core-boundary-unit', 'store-atomic', 'core-reference', 'core-refresh',
        'core-trim', 'release-app',
    ]
    need([x.get('name') for x in build] == build_names
         and all(x.get('exit') == 0 for x in build),
         'full prototype build/test receipt is incomplete or contains failures')
    build_lines = (S / 'build-driver.stdout').read_text().splitlines()
    build_terminal = json.loads(build_lines[-1]) if build_lines else {}
    need(build_terminal.get('done') is True and build_terminal.get('steps') == 10
         and build_terminal.get('test_phases') == 6,
         'full build driver has no successful terminal receipt')
    need((S / 'build-driver.stderr').read_bytes() == b'',
         'full build driver stderr is nonempty')
    review = read_json(S / 'independent-review.json')
    need(review.get('status') == 'no_findings' and review.get('findings') == [],
         'prototype source review is not clear')
    followup = read_json(S / 'test-warning-independent-review.json')
    need(followup.get('status') == 'no_findings' and followup.get('findings') == []
         and followup.get('assertions_weakened') is False,
         'test-warning correction review is not clear')
    for path, digest in followup['after_sha256'].items():
        need(sha(S / 'source' / path) == digest, f'corrected test source changed: {path}')
    base, candidate_sources, inputs, basis, audit = source_maps()
    candidate, control, observer, _ = binaries_and_guards()
    build_lines = (S / 'build-driver.stdout').read_text().splitlines()
    build_terminal = json.loads(build_lines[-1]) if build_lines else {}
    need(build_terminal.get('app_sha256') == candidate['deadpan-app']['sha256'],
         'candidate app hash differs from successful release build terminal receipt')
    test_bins = read_json(S / 'test-binaries.json')
    need(set(test_bins) == {
        'core-refutations', 'core-boundary-unit', 'store-atomic',
        'core-reference', 'core-refresh', 'core-trim',
    }, 'focused private test binary set is incomplete')
    for name, rows in test_bins.items():
        need(rows and all(Path(r['executed']).is_file() and Path(r['retained']).is_file()
                           and sha(r['executed']) == r['sha256'] == sha(r['retained'])
                           for r in rows), f'{name}: retained/executed test identity mismatch')
    freeze_runners(candidate)
    verify_frozen(candidate, control, observer)

    env = app_env()
    timing = []
    for pair, order in enumerate(ORDER, 1):
        for kind in order:
            timing.append(timed_trial(pair, kind, candidate, control, observer, inputs))
            write_json(S / 'timing-trials.json', timing)
    write_json(S / 'timing-summary.json', summarize_timing(timing))

    memory = []
    for pair, order in enumerate(ORDER, 1):
        for kind in order:
            memory.append(memory_trial(pair, kind, candidate, control, observer, inputs))
            write_json(S / 'memory-trials.json', memory)
    write_json(S / 'memory-summary.json', summarize_memory(memory))
    verify_frozen(candidate, control, observer)
    need(len(commands) == 40 and all(row['exit'] == 0 for row in commands),
         'expected 40 successful fresh-fixture/app commands')
    expected_commands = []
    for lane, kind_orders, action in (
        ('timing', ORDER, 'run'), ('memory', ORDER, 'observer'),
    ):
        for pair, order in enumerate(kind_orders, 1):
            for kind in order:
                stem = f'{lane}-{pair}-{kind}'
                expected_commands.extend([stem + '-make', stem if action == 'run' else stem + '-observer'])
    need([row['name'] for row in commands] == expected_commands,
         'measurement command list differs from the exact two-lane five-pair plan')
    terminal = {
        'done': True, 'timing_trials': len(timing), 'memory_trials': len(memory),
        'timing_summary': str(S / 'timing-summary.json'),
        'memory_summary': str(S / 'memory-summary.json'),
    }
    print(json.dumps(terminal, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
