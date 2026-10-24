#!/usr/bin/env python3
"""Separate owned-child memory lane. Root executes only after timing and review."""
from pathlib import Path
import hashlib
import json
import os
import statistics
import subprocess
import time

S = Path('/tmp/deadpan-single-leaf-net-prototype-v2-2026-10-24')
SOURCE = S / 'source'
MAIN = Path('/Users/michael/Code/deadpan')
MAIN_MAP = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23/frozen-sources.json')
BASE = Path('/tmp/deadpan-insert-split-proof-prototype-v3-2026-10-23')
OLD = Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
KESTREL = Path('/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift')
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
ORDER = [('baseline', 'candidate'), ('candidate', 'baseline'), ('baseline', 'candidate')]
COMMANDS = []
FROZEN = {}
OWNED = False


def need(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def save(path, data, exclusive=False):
    with Path(path).open('x' if exclusive else 'w') as stream:
        stream.write(json.dumps(data, indent=2) + '\n')


def absent(path):
    need(not os.path.lexists(path), f'refusing existing output, including symlink: {path}')


def snapshot(root):
    paths = sorted(Path(root).rglob('*'))
    need(not any(p.is_symlink() for p in paths), f'fixture contains symlink: {root}')
    return {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
            for p in paths if p.is_file()}


def run(name, args, env):
    out, err = S / f'{name}.stdout', S / f'{name}.stderr'
    absent(out)
    absent(err)
    started = time.monotonic()
    with out.open('xb') as stdout, err.open('xb') as stderr:
        result = subprocess.run([str(x) for x in args], cwd=SOURCE, env=env,
                                stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
    row = {'name': name, 'argv': [str(x) for x in args], 'cwd': str(SOURCE),
           'exit': result.returncode, 'seconds': time.monotonic() - started,
           'profile_gate': env.get('DEADPAN_PAUSE_ROUTE_PROFILE')}
    COMMANDS.append(row)
    save(S / 'memory-commands.json', COMMANDS)
    print(json.dumps(row), flush=True)
    need(result.returncode == 0, f'{name} exited {result.returncode}; stop and retain evidence')


def quiet(name):
    started, observations = time.monotonic(), []
    path = S / f'{name}-preflight.json'
    absent(path)
    while True:
        result = subprocess.run(['ps', '-axo', 'pid=,comm=,args='],
                                capture_output=True, text=True)
        need(result.returncode == 0, f'ps failed: {result.stderr}')
        conflicts = []
        for line in result.stdout.splitlines():
            parts = line.strip().split(None, 2)
            if len(parts) < 2 or int(parts[0]) == os.getpid():
                continue
            comm = Path(parts[1]).name.lower()
            if any(token in comm for token in
                   ('cargo', 'rustc', 'deadpan', 'memory-observer', 'memory_observer')):
                conflicts.append(line)
        load = list(os.getloadavg())
        observations.append({'elapsed_seconds': time.monotonic() - started,
                             'load': load, 'conflicting_processes': conflicts})
        save(path, observations)
        if load[0] <= 3 and not conflicts:
            power = {}
            for kind, args in [('battery', ['pmset', '-g', 'batt']),
                               ('thermal', ['pmset', '-g', 'therm'])]:
                result = subprocess.run(args, capture_output=True, text=True)
                power[kind] = {'argv': args, 'exit': result.returncode,
                               'stdout': result.stdout, 'stderr': result.stderr}
            save(S / f'{name}-power.json', power, exclusive=True)
            return observations[-1]
        need(time.monotonic() - started < 180, f'{name}: quiet preflight did not clear')
        time.sleep(5)


def guards(bins, baseline_helpers, provenance):
    for path, digest in FROZEN.items():
        need(sha(path) == digest, f'frozen runner/manifest changed: {path}')
    timing = read(S / 'measurement-terminal.json')
    need(timing == {'status': 'complete', 'trials': 18, 'commands': 36,
                    'workflow_checks': 126, 'kestrel_checks': 18},
         'memory lane requires completed timing lane')
    timing_owner = read(S / 'measurement-owner.json')
    need(timing_owner['memory_observer'] is False and
         timing_owner['candidate_app_sha256'] == bins['candidate']['sha256'] and
         timing_owner['matched_profile_app_sha256'] == bins['baseline']['sha256'] and
         timing_owner['source_map_sha256'] == sha(S / 'frozen-sources.json') and
         timing_owner['matched_source_map_sha256'] == sha(S / 'baseline-sources.json'),
         'completed timing owner identity mismatch')
    for path, digest in read(S / 'driver-pins.json').items():
        need(sha(S / path) == digest, f'timing driver pin changed: {path}')
    need(sha(MAIN_MAP) == provenance['qualified_main_source_map_sha256'], 'Main map identity')
    paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=MAIN,
                                    text=True).splitlines()
    need({p: sha(MAIN / p) for p in paths} == read(MAIN_MAP), 'Main Rust map changed')
    sources = read(S / 'frozen-sources.json')
    need({str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')} == sources,
         'candidate source map changed')
    review = read(S / 'source-review.json')
    need(review.get('status') == 'no_findings' and review.get('findings') == [] and
         review.get('source_map_sha256') == sha(S / 'frozen-sources.json'),
         'candidate review does not bind exact clean source map')
    matched_root = Path(provenance['matched_source'])
    need(sha(S / 'baseline-sources.json') == provenance['matched_source_map_sha256'],
         'matched source map identity')
    need({str(p.relative_to(matched_root)): sha(p) for p in matched_root.rglob('*.rs')}
         == read(S / 'baseline-sources.json'), 'matched source tree changed')
    inputs = read(S / 'baseline-inputs.json')
    need(len(inputs) == 6, 'expected actual six input identities')
    for rel, digest in inputs.items():
        for root in (SOURCE, matched_root, MAIN):
            need(sha(root / rel) == digest, f'input changed: {root / rel}')
    for row in [*bins.values(), *baseline_helpers.values(), *read(S / 'binaries.json').values()]:
        need(sha(row['executed']) == row['sha256'], f'binary changed: {row["executed"]}')
    observer = read(S / 'observer-provenance.json')
    for path, digest in observer['pins'].items():
        need(sha(path) == digest, f'observer/fixture/Kestrel provenance changed: {path}')
    old = read(OLD / 'observer-binary.json')
    need(old['sha256'] == observer['observer_sha256'] and
         old['source_sha256'] == observer['actual_source_sha256'] and
         old['adapter_rlib_sha256'] == observer['adapter_rlib_sha256'], 'observer manifest mismatch')
    smokes = read(OLD / 'observer-smoke-resumed/summary.json')
    need(smokes['cases_passed'] == 5 and len(smokes['cases']) == 5 and
         all(x['passed'] is True for x in smokes['cases']), 'retained smoke evidence mismatch')
    terminal = read(S / 'build-terminal.json')
    need(terminal['status'] == 'complete' and terminal['app_sha256'] == bins['candidate']['sha256'],
         'candidate build terminal mismatch')
    need(bins['baseline']['sha256'] == provenance['matched_app_sha256'] ==
         read(S / 'baseline.json')['matched_profile']['sha256'], 'matched ffba identity mismatch')
    need(read(S / 'binaries.json')['deadpan-app']['freshly_built'] is True,
         'candidate must be fresh release app')


def sample_summary(path, receipt):
    rows = read(path)
    need(isinstance(rows, list) and 0 < len(rows) <= 60000 and
         receipt.get('sample_count') == len(rows), 'raw sample count mismatch')
    last = -1
    for row in rows:
        need(set(row) == {'elapsed_ns', 'query_ns', 'phys_footprint_bytes', 'resident_size_bytes'},
             'raw sample schema mismatch')
        need(all(type(v) is int and v >= 0 for v in row.values()), 'invalid sample integer')
        need(row['elapsed_ns'] > last, 'nonmonotonic sample start')
        last = row['elapsed_ns']
    maxima = {'observed_max_phys_footprint_bytes': max(r['phys_footprint_bytes'] for r in rows),
              'observed_max_resident_size_bytes': max(r['resident_size_bytes'] for r in rows)}
    computed = dict(maxima, sample_count=len(rows),
                    first_sample_start_elapsed_ns=rows[0]['elapsed_ns'],
                    last_sample_start_elapsed_ns=rows[-1]['elapsed_ns'],
                    maximum_sample_start_gap_ns=max(
                        (b['elapsed_ns'] - a['elapsed_ns'] for a, b in zip(rows, rows[1:])), default=0),
                    sample_query_sum_ns=sum(r['query_ns'] for r in rows))
    need(all(receipt.get(k) == v for k, v in computed.items()), 'receipt/raw aggregate mismatch')
    window = receipt.get('sampling_window_elapsed_ns')
    need(type(window) is int and window >= rows[-1]['elapsed_ns'] + rows[-1]['query_ns'],
         'sampling window excludes last query')
    return dict(computed, sampling_window_elapsed_ns=window)


def verify_report(path, binary, fixture_hash):
    report = read(path)
    metadata = report['metadata']
    need(report.get('failed') is False and metadata['binary_sha256'] == binary['sha256'] and
         metadata['build_profile'] == 'release' and metadata['fixture_sha256'] == fixture_hash,
         'native report result/identity mismatch')
    need(metadata['cargo_lock_sha256_at_run'] == read(S / 'baseline-inputs.json')['Cargo.lock'],
         'native Cargo.lock identity mismatch')
    checks = {}
    for scenario, count in [('large-first-pause', 7), ('kestrel-shortcuts', 1)]:
        cases = [r for r in report['scenarios'] if r['name'] == scenario]
        need(len(cases) == 1 and len(cases[0]['checks']) == count and
             all(c['passed'] is True for c in cases[0]['checks']), f'{scenario} checks failed')
        checks[scenario] = count
    findings = report.get('findings', []) + [finding for case in report['scenarios']
                                            for finding in case.get('findings', [])]
    need(not any(x.get('severity') == 'error' for x in findings),
         'native error-severity finding')
    return checks, len(findings), sum(x.get('severity') == 'warning' for x in findings), metadata.get('rust')


def trial(round_no, kind, bins, helpers, provenance, env):
    name = f'memory-{round_no}-{kind}'
    fixture = S / f'{name}.deadpan'
    receipt, samples = S / f'{name}-observer-receipt.json', S / f'{name}-samples.json'
    stdout, stderr, report = S / f'{name}-app.stdout', S / f'{name}-app.stderr', S / f'{name}-report'
    for p in (fixture, receipt, samples, stdout, stderr, report,
              S / f'{name}-input-before.json', S / f'{name}-input-after.json'):
        absent(p)
    guards(bins, helpers, provenance)
    quiet(name + '-setup')
    run(name + '-make', [PERF, 'make-large', fixture, '--beats', '10000'], env)
    before = snapshot(fixture)
    need('project.sqlite' in before, 'fresh fixture DB absent')
    save(S / f'{name}-input-before.json', before, exclusive=True)
    preflight = quiet(name)
    guards(bins, helpers, provenance)
    binary = bins[kind]
    observer = read(S / 'observer-provenance.json')
    for p in (receipt, samples, stdout, stderr, report):
        absent(p)
    run(name + '-observer', [
        observer['observer'], '--receipt', receipt, '--samples', samples,
        '--stdout', stdout, '--stderr', stderr, '--', binary['executed'],
        '--ui-check', '--mode', 'performance', '--scenario', 'large-first-pause',
        '--kestrel-source', KESTREL, '--project', fixture, '--output', report], env)
    after = snapshot(fixture)
    save(S / f'{name}-input-after.json', after, exclusive=True)
    need({k: v for k, v in before.items() if k not in SIDECARS} ==
         {k: v for k, v in after.items() if k not in SIDECARS}, 'authored paths/bytes changed')
    need(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0, 'nonempty WAL')
    r = read(receipt)
    true_keys = ('terminal', 'valid_memory_trial', 'samples_written', 'group_teardown_confirmed',
                 'leader_exit_confirmed', 'reap_attempted', 'reaped',
                 'sampled_lower_bound_only', 'whole_app_lifetime_only')
    need(all(r.get(k) is True for k in true_keys), 'invalid terminal/cleanup/scope receipt')
    need(r.get('reason') == 'exited' and r.get('app_exit_code') == 0 and
         r.get('app_exit_signal') is None and r.get('errors') == [] and
         r.get('cleanup_unconfirmed') is False and r.get('per_leaf_memory') is False and
         r.get('memory_ceiling_qualified') is False and
         type(r.get('owned_app_pid')) is int and r['owned_app_pid'] > 0, 'invalid exit/ownership')
    need(r.get('requested_interval_ms') == 5 and r.get('maximum_runtime_seconds') == 180 and
         r.get('maximum_samples') == 60000, 'observer configured limits changed')
    memory = sample_summary(samples, r)
    checks, findings_count, warnings_count, reported_rust = verify_report(
        report / 'report.json', binary, before['project.sqlite']['sha256'])
    text = stderr.read_text(encoding='utf-8', errors='strict')
    need('DEADPAN_PAUSE_ROUTE' not in text and 'DEADPAN_CURRENT_LEAF_PROFILE' not in text,
         'gate-off trial emitted diagnostic profile')
    guards(bins, helpers, provenance)
    return {'lane': 'owned-observer-memory-only', 'round': round_no, 'kind': kind,
            'binary_sha256': binary['sha256'], 'observer_sha256': observer['observer_sha256'],
            'fixture_sha256': before['project.sqlite']['sha256'], 'checks': checks,
            'findings_count': findings_count, 'warnings_count': warnings_count,
            'reported_runtime_rust_field': reported_rust,
            'physical_display_measured': False, 'gpu_readback_in_timing_run': False,
            'expected_pinned_build_rust': '1.97.1',
            'profile_gate_absent': True, 'profile_output_absent': True,
            'authored_paths_bytes_unchanged': True, 'wal_empty': True,
            'memory': memory, 'load_at_start': preflight['load'],
            'receipt': str(receipt), 'samples': str(samples), 'report': str(report / 'report.json')}


def main():
    global OWNED
    for name in ('memory-owner.json', 'memory-commands.json', 'memory-trials.json',
                 'memory-summary.json', 'memory-terminal.json'):
        absent(S / name)
    provenance = read(S / 'baseline-provenance.json')
    candidate = read(S / 'binaries.json')['deadpan-app']
    bins = {'baseline': {'executed': provenance['matched_app'],
                         'sha256': provenance['matched_app_sha256']},
            'candidate': candidate}
    helpers = read(BASE / 'binaries.json')
    files = [Path(__file__), S / 'observer-provenance.json', S / 'baseline.json',
             S / 'baseline-provenance.json', S / 'baseline-sources.json', S / 'baseline-inputs.json',
             S / 'frozen-sources.json', S / 'source-review.json', S / 'binaries.json',
             S / 'build-terminal.json', BASE / 'binaries.json', MAIN_MAP,
             S / 'measurement-terminal.json', S / 'measurement-owner.json',
             S / 'driver-pins.json']
    FROZEN.update({p: sha(p) for p in files})
    guards(bins, helpers, provenance)
    save(S / 'memory-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
         'script': str(Path(__file__)), 'frozen_files': {str(p): h for p, h in FROZEN.items()},
         'order': ORDER, 'separate_from_timing': True}, exclusive=True)
    OWNED = True
    env = dict(os.environ)
    for key in ('DEADPAN_PAUSE_ROUTE_PROFILE', 'DEADPAN_CURRENT_LEAF_PROFILE'):
        env.pop(key, None)
    rows = []
    for round_no, order in enumerate(ORDER, 1):
        for kind in order:
            rows.append(trial(round_no, kind, bins, helpers, provenance, env))
            save(S / 'memory-trials.json', rows)
    need([(r['round'], r['kind']) for r in rows] ==
         [(i, k) for i, pair in enumerate(ORDER, 1) for k in pair], 'pair order changed')
    summary = {'lane': 'owned-observer-memory-only', 'n_per_kind': 3, 'trials': 6,
               'separate_from_timing_lane': True, 'profile_gate': 'absent for all six',
               'sampled_whole_main_app_lifetime_lower_bound_only': True,
               'exact_peak': False, 'per_leaf_memory': False, 'memory_ceiling': False,
               'worker_or_external_gpu_memory': False, 'kinds': {}}
    for kind in bins:
        selected = [r['memory'] for r in rows if r['kind'] == kind]
        need(len(selected) == 3, 'incomplete memory pair batch')
        summary['kinds'][kind] = {}
        for key in ('observed_max_phys_footprint_bytes', 'observed_max_resident_size_bytes',
                    'maximum_sample_start_gap_ns', 'sampling_window_elapsed_ns', 'sample_count'):
            values = [r[key] for r in selected]
            summary['kinds'][kind][key] = {'values': values, 'min': min(values),
                                         'median': statistics.median(values), 'max': max(values)}
    guards(bins, helpers, provenance)
    save(S / 'memory-summary.json', summary, exclusive=True)
    save(S / 'memory-terminal.json', {'status': 'complete', 'trials': 6,
         'summary': str(S / 'memory-summary.json'), 'summary_sha256': sha(S / 'memory-summary.json')},
         exclusive=True)
    print(json.dumps({'status': 'complete', 'memory_trials': 6}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        if OWNED and not os.path.lexists(S / 'memory-terminal.json'):
            save(S / 'memory-terminal.json', {'status': 'failed', 'type': type(error).__name__,
                 'message': str(error), 'commands_completed': len(COMMANDS)}, exclusive=True)
        raise
