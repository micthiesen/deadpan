"""Unexecuted bounded evidence packager. Requires Root FINAL-PACKAGING-GO.json."""
from pathlib import Path
import difflib
import gzip
import hashlib
import json
import re
import shutil
import statistics

ROOT = Path('/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-one-leaf-replay-reuse-prototype'
STAGE = DEST.with_name(DEST.name + '.staging')
Q = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
PROFILE_Q = Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
CONTROL_SHA = 'a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f'
CANDIDATE_SHA = '6d1eaa132978c7c796ae1f56bb6d417bb19627511ce48cdb2e2b48724ccde73b'
OBSERVER_SHA = 'd1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2'
PERF_SHA = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
BASE_COMMIT = '3a32c923358c0cb7ca53f821bfb6db3e01ebb466'
BASE_RUST = 2051
FINAL_RUST = 2053
CHANGED = [
    'crates/deadpan-cli/src/macros.rs',
    'crates/deadpan-core/src/audio_reference.rs',
    'crates/deadpan-core/src/compound.rs',
    'crates/deadpan-core/src/insert_time/retimed.rs',
    'crates/deadpan-core/src/lib.rs',
    'crates/deadpan-core/src/one_leaf_replay.rs',
    'crates/deadpan-core/src/semantic/planner.rs',
    'crates/deadpan-core/src/semantic/planner/content.rs',
    'crates/deadpan-core/src/semantic/planner/pause.rs',
    'crates/deadpan-core/src/semantic/planner/tests.rs',
    'crates/deadpan-core/tests/semantic_leaf_replay.rs',
    'crates/deadpan-store/src/compound.rs',
    'crates/deadpan-store/src/lib.rs',
    'crates/deadpan-store/tests/compound_preparation.rs',
]
NEW_FILES = {
    'crates/deadpan-core/src/one_leaf_replay.rs',
    'crates/deadpan-core/tests/semantic_leaf_replay.rs',
}
INPUT_NAMES = {
    'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
    'crates/deadpan-app/src/navigation/registry/baseline-routing.txt',
    'docs/COMMANDS.md',
}
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
SAFE_SUFFIXES = {
    '.json', '.py', '.md', '.txt', '.log', '.stdout', '.stderr', '.patch', '.html',
}
COMPRESS_SUFFIXES = {'.json', '.txt', '.md', '.log', '.stdout', '.stderr', '.html', '.patch'}
LIMIT = 100_000
PAIR_ORDER = [
    ['control', 'candidate'], ['candidate', 'control'],
    ['control', 'candidate'], ['candidate', 'control'],
    ['control', 'candidate'],
]
BUILD_PHASES = [
    'rustc-toolchain', 'cargo-toolchain', 'format-check', 'core-refutations',
    'core-boundary-unit', 'store-atomic', 'core-reference', 'core-refresh',
    'core-trim', 'release-app',
]
TEST_PHASES = [
    'core-refutations', 'core-boundary-unit', 'store-atomic',
    'core-reference', 'core-refresh', 'core-trim',
]


def need(condition, message):
    if not condition:
        raise RuntimeError(message)


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def canonical(value, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def copy_payload(source, relative):
    source = Path(source)
    need(source.is_file(), f'missing retained payload: {source}')
    destination = STAGE / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    raw = source.read_bytes()
    if len(raw) > LIMIT and source.suffix.lower() in COMPRESS_SUFFIXES:
        destination = Path(str(destination) + '.gz')
        with destination.open('wb') as compressed:
            with gzip.GzipFile(filename='', mode='wb', compresslevel=9,
                               fileobj=compressed, mtime=0) as writer:
                writer.write(raw)
        with gzip.open(destination, 'rb') as reader:
            need(reader.read() == raw, f'gzip reread mismatch: {source}')
    else:
        destination.write_bytes(raw)
    return destination


def copy_tree(source, relative, allowed=SAFE_SUFFIXES, skip_parts=()):
    source = Path(source)
    need(source.is_dir(), f'missing provenance directory: {source}')
    copied = 0
    for path in sorted(source.rglob('*')):
        if not path.is_file() or any(part in skip_parts for part in path.parts):
            continue
        if path.suffix.lower() not in allowed:
            continue
        copy_payload(path, Path(relative) / path.relative_to(source))
        copied += 1
    need(copied > 0, f'no bounded files copied from: {source}')
    return copied


def file_snapshot(root):
    root = Path(root)
    return {
        str(path.relative_to(root)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
        for path in sorted(root.rglob('*')) if path.is_file()
    }


def pair_order(lane):
    return [(pair, kind) for pair, kinds in enumerate(PAIR_ORDER, 1) for kind in kinds]


def validate_source_maps():
    base = read(ROOT / 'original-source-identities.json')
    final = read(ROOT / 'formatted-source-identities.json')
    initial = read(ROOT / 'initial-build/formatted-source-identities.json')
    audit = read(ROOT / 'formatted-overlay-audit.json')
    basis = read(ROOT / 'source-basis.json')
    inputs = read(ROOT / 'input-identities.json')
    control_base = read(Q / 'candidate-all-core-source-identities.json')
    control_inputs = read(Q / 'candidate-all-core-inputs.json')
    need(basis.get('commit') == BASE_COMMIT and basis.get('rust_files') == BASE_RUST,
         'source basis does not identify the qualified 2,051-file base')
    need(len(base) == BASE_RUST and base == control_base,
         'baseline map differs from qualified source identity')
    need(len(final) == FINAL_RUST and len(initial) == FINAL_RUST
         and set(initial) == set(final),
         'final or initial-build Rust source maps differ in path count')
    need(set(audit.get('changed', [])) == set(CHANGED)
         and audit.get('rust_files') == FINAL_RUST
         and audit.get('inputs_unchanged') is True,
         'formatted overlay audit does not match the exact 14-file change set')
    need({path for path, digest in final.items() if base.get(path) != digest} == set(CHANGED),
         'source map differs outside the exact 14-file overlay')
    corrected_test_paths = {
        'crates/deadpan-core/tests/semantic_leaf_replay.rs',
        'crates/deadpan-store/tests/compound_preparation.rs',
    }
    initial_differences = {path for path in final if initial[path] != final[path]}
    need(initial_differences == corrected_test_paths
         and audit.get('initial_formatted_after') == {path: initial[path] for path in CHANGED}
         and audit.get('formatted_after') == {path: final[path] for path in CHANGED},
         'initial/final source maps do not match the exact reviewed test-only corrections')
    need(set(final) - set(base) == NEW_FILES
         and sum(1 for path in final if path in base and final[path] != base[path]) == 12,
         'expected 12 modified existing files and two new files')
    live = {
        str(path.relative_to(ROOT / 'source')): sha(path)
        for path in (ROOT / 'source').rglob('*.rs')
    }
    need(live == final, 'frozen private source tree no longer matches final map')
    actual_main_paths = __import__('subprocess').check_output(
        ['rg', '--files', '-g', '*.rs'], cwd=REPO, text=True,
    ).splitlines()
    main = {path: sha(REPO / path) for path in actual_main_paths}
    need(len(main) == BASE_RUST and main == base,
         'shared main checkout no longer matches the 2,051-file baseline')
    need(set(inputs) == INPUT_NAMES and inputs == control_inputs,
         'six frozen inputs differ from qualified baseline')
    for path, digest in inputs.items():
        need(sha(ROOT / 'source' / path) == digest and sha(REPO / path) == digest,
             f'frozen input changed: {path}')
    return base, final, initial, inputs, audit, basis


def validate_binaries():
    binaries = read(ROOT / 'binaries.json')
    control_manifest = read(Q / 'release-native-binaries.json')
    observer = read(PROFILE_Q / 'observer-binary.json')
    need(binaries['deadpan-app']['sha256'] == CANDIDATE_SHA
         and binaries['deadpan-app']['freshly_built'] is True,
         'candidate release app identity changed')
    candidate_app = Path(binaries['deadpan-app']['executed'])
    need(candidate_app.is_file() and sha(candidate_app) == CANDIDATE_SHA,
         'candidate executed app binary mismatch')
    control = control_manifest['deadpan-app']
    control_app = Path(control['executed'])
    need(control['sha256'] == CONTROL_SHA and control_app.is_file()
         and sha(control_app) == CONTROL_SHA,
         'qualified control app identity changed')
    for name in ('deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track'):
        row = binaries[name]
        qualified = control_manifest[name]
        path = Path(row['executed'])
        need(path.is_file() and row['sha256'] == qualified['sha256'] == sha(path),
             f'qualified helper reuse changed: {name}')
    for name, row in binaries.items():
        path = Path(row['executed'])
        need(path.is_file() and sha(path) == row['sha256'],
             f'executed candidate binary mismatch: {name}')
    observer_path = Path(observer['path'])
    need(observer_path.is_file() and sha(observer_path) == OBSERVER_SHA
         and observer['sha256'] == OBSERVER_SHA,
         'owned-observer binary mismatch')
    need(Path(observer['source']).is_file()
         and sha(observer['source']) == observer['source_sha256'],
         'owned-observer source mismatch')
    need(Path(observer['adapter_rlib']).is_file()
         and sha(observer['adapter_rlib']) == observer['adapter_rlib_sha256'],
         'owned-observer linked process adapter mismatch')
    need(sha(PERF) == PERF_SHA, 'qualified fixture helper changed')
    kestrel_record = read(Q / 'release-native-kestrel.json')
    kestrel_path = Path(kestrel_record.get('path', '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'))
    need(kestrel_path.is_file() and sha(kestrel_path) == kestrel_record['sha256'],
         'Kestrel source changed from qualified identity')
    return binaries, control, observer, kestrel_path


def validate_build_and_history():
    go = read(ROOT / 'FINAL-PACKAGING-GO.json')
    need(go.get('authorized') is True, 'Root final-packaging GO missing')
    measure_gate = read(ROOT / 'MEASUREMENT-GO.json')
    need(measure_gate.get('authorized') is True
         and measure_gate.get('candidate_app_sha256') == CANDIDATE_SHA
         and measure_gate.get('control_app_sha256') == CONTROL_SHA
         and measure_gate.get('observer_sha256') == OBSERVER_SHA
         and measure_gate.get('timing_pairs') == 5
         and measure_gate.get('separate_memory_pairs') == 5
         and measure_gate.get('no_production_adoption') is True,
         'measurement GO does not match the final prototype scope')
    need(sha(ROOT / 'measure.py') == measure_gate.get('measure_sha256'),
         'executed measurement driver differs from authorized copy')
    final_build = read(ROOT / 'build-receipt.json')
    need([row.get('name') for row in final_build] == BUILD_PHASES
         and all(row.get('exit') == 0 for row in final_build),
         'current complete build receipt is not ten successful phases')
    terminal_lines = (ROOT / 'build-driver.stdout').read_text().splitlines()
    terminal = json.loads(terminal_lines[-1]) if terminal_lines else {}
    need(terminal.get('done') is True and terminal.get('steps') == 10
         and terminal.get('test_phases') == 6
         and terminal.get('app_sha256') == CANDIDATE_SHA,
         'current build lacks matching successful terminal receipt')
    need((ROOT / 'build-driver.stderr').read_bytes() == b'',
         'current complete build driver stderr is nonempty')
    initial = read(ROOT / 'initial-build/build-receipt.json')
    need([row.get('name') for row in initial] == BUILD_PHASES
         and all(row.get('exit') == 0 for row in initial),
         'initial build was not ten successful phases')
    initial_bin = read(ROOT / 'initial-build/binaries.json')
    need(initial_bin['deadpan-app']['sha256'] == CANDIDATE_SHA,
         'initial/final release app bytes differ')
    correction = read(ROOT / 'test-warning-correction-review.json')
    review = read(ROOT / 'test-warning-independent-review.json')
    original_review = read(ROOT / 'independent-review.json')
    need(set(correction.get('changed', [])) == {
        'crates/deadpan-core/tests/semantic_leaf_replay.rs',
        'crates/deadpan-store/tests/compound_preparation.rs',
    } and correction.get('production_and_other_sources_unchanged') == 2051
         and correction.get('inputs_unchanged') is True
         and correction.get('initial_tests_all_passed') == 39
         and correction.get('initial_warnings') == 4,
         'test-warning correction metadata differs from bounded two-file correction')
    need(review.get('status') == 'no_findings' and review.get('findings') == []
         and review.get('assertions_weakened') is False,
         'warning-correction review has findings or weakened assertions')
    need(original_review.get('status') == 'no_findings'
         and original_review.get('findings') == [],
         'prototype source review has findings')
    first_current, initial_results = test_results(ROOT), test_results(ROOT / 'initial-build')
    need(first_current['passed'] == initial_results['passed'] == 39
         and first_current['ignored'] == initial_results['ignored'] == 0,
         'current/initial six focused outputs do not each show 39 passed, zero ignored')
    current_warnings = compiler_warning_details(ROOT)
    old_warnings = compiler_warning_details(ROOT / 'initial-build')
    need(current_warnings == [] and len(old_warnings) == 4,
         'expected zero final compiler warnings and four initial test warnings')
    relocation = read(ROOT / 'initial-build/retained-test-binary-relocations.json')
    need(len(relocation) == 6, 'initial executed-test binary relocation list incomplete')
    initial_bin_rows = {
        row['sha256']: row
        for rows in read(ROOT / 'initial-build/test-binaries.json').values()
        for row in rows
    }
    for record in relocation:
        original = record['original']
        archived = Path(record['archived_retained'])
        need(original['sha256'] == record['sha256'] and archived.is_file()
             and sha(archived) == record['sha256']
             and archived.stat().st_size == original['bytes']
             and record['sha256'] in initial_bin_rows,
             'initial archived test-binary copy no longer matches relocation receipt')
    failure = read(ROOT / 'resume-preflight-failure/failure.json')
    need(failure.get('phase') == 'resume wrapper preflight'
         and failure.get('commands_executed') == 0,
         'retained resumed-build preflight failure is not the known no-command refusal')
    return {
        'final_go': go, 'measurement_go': measure_gate,
        'build_phases': len(final_build), 'initial_build_phases': len(initial),
        'current_tests': first_current, 'initial_tests': initial_results,
        'current_compiler_warnings': current_warnings,
        'initial_test_compiler_warnings': old_warnings,
        'warning_correction_review': correction, 'review_status': review['status'],
        'initial_test_binaries_relocated_and_verified': len(relocation),
        'resume_preflight_failure': failure,
    }


def test_results(root):
    root = Path(root)
    passed = ignored = failed = 0
    per_phase = {}
    for phase in TEST_PHASES:
        text = (root / f'{phase}.stdout').read_text()
        matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
        need(len(matches) == 1, f'{root.name}/{phase}: expected one Rust test result')
        p, f, i = map(int, matches[0])
        per_phase[phase] = {'passed': p, 'failed': f, 'ignored': i}
        passed += p
        failed += f
        ignored += i
    need(failed == 0, f'{root.name}: failed Rust tests present')
    return {'passed': passed, 'failed': failed, 'ignored': ignored, 'per_phase': per_phase}


def compiler_warning_details(root):
    details = []
    for phase in TEST_PHASES + ['release-app']:
        path = Path(root) / f'{phase}.stderr'
        if not path.exists():
            continue
        for line in path.read_text().splitlines():
            if re.match(r'^warning: (unused |unreachable )', line):
                details.append({'phase': phase, 'warning': line})
    return details


def no_profile_marker(path):
    marker = b'DEADPAN_CURRENT_LEAF_PROFILE'
    total = 0
    with Path(path).open('rb') as stream:
        while chunk := stream.read(1_048_576):
            total += len(chunk)
            need(marker not in chunk, f'{path}: profiler marker appeared')
    return total


def validate_fixture(key):
    before = read(ROOT / f'{key}-input-before.json')
    after = read(ROOT / f'{key}-input-after.json')
    filtered = lambda mapping: {p: v for p, v in mapping.items() if p not in SIDECARS}
    need(filtered(before) == filtered(after), f'{key}: authored path/bytes changed')
    need(before['project.sqlite']['sha256'] == after['project.sqlite']['sha256'],
         f'{key}: main SQLite database changed')
    need(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0,
         f'{key}: final WAL is nonempty')
    return before, after


def validate_report(key, app_sha, lock_sha):
    report = read(ROOT / f'{key}-report/report.json')
    metadata = report.get('metadata', {})
    need(report.get('failed') is False
         and metadata.get('build_profile') == 'release'
         and metadata.get('binary_sha256') == app_sha,
         f'{key}: release report failure/profile/binary mismatch')
    need(metadata.get('rust', '').startswith('rustc 1.97.1 ')
         and metadata.get('cargo_lock_sha256_at_run') == lock_sha
         and metadata.get('physical_display_measured') is False,
         f'{key}: toolchain, Cargo.lock or physical-display metadata mismatch')
    need(metadata.get('fixture_sha256') == read(ROOT / f'{key}-input-before.json')['project.sqlite']['sha256'],
         f'{key}: report fixture digest mismatch')
    need(metadata.get('gpu_readback_in_timing_run') is False,
         f'{key}: GPU readback unexpectedly included in timing')
    pause = [s for s in report.get('scenarios', []) if s.get('name') == 'large-first-pause']
    kestrel = [s for s in report.get('scenarios', []) if s.get('name') == 'kestrel-shortcuts']
    need(len(pause) == len(kestrel) == 1
         and len(pause[0].get('checks', [])) == 7
         and all(check.get('passed') is True for check in pause[0]['checks'])
         and len(kestrel[0].get('checks', [])) == 1
         and kestrel[0]['checks'][0].get('passed') is True,
         f'{key}: seven pause checks / one Kestrel check did not pass')
    warnings = pause[0].get('findings', [])
    need(len(warnings) == 2 and all(w.get('severity') == 'warning' for w in warnings),
         f'{key}: actual pause warnings do not match the preserved report')
    metrics = {
        timing['name']: timing['summary']['completed']['p50_ms']
        for timing in pause[0].get('timings', [])
        if timing.get('name', '').startswith('large_first_pause')
    }
    need(set(metrics) == {
        'large_first_pause_input_to_commit_ms',
        'large_first_pause_input_to_offscreen_background_composition_ms',
    }, f'{key}: expected two pause timing metrics missing')
    return report, metrics, warnings


def validate_observer(key):
    receipt = read(ROOT / f'{key}-observer-receipt.json')
    rows = read(ROOT / f'{key}-memory-samples.json')
    need(isinstance(rows, list) and rows and len(rows) <= 60000,
         f'{key}: raw samples missing or over bound')
    need(receipt.get('terminal') is True and receipt.get('valid_memory_trial') is True
         and receipt.get('reason') == 'exited'
         and receipt.get('owned_app_pid', 0) > 0,
         f'{key}: observer trial is not a valid owned-child run')
    need(receipt.get('requested_interval_ms') == 5
         and receipt.get('maximum_runtime_seconds') == 180
         and receipt.get('maximum_samples') == 60000
         and receipt.get('samples_written') is True,
         f'{key}: observer bounds/sample persistence differ')
    need(receipt.get('group_teardown_confirmed') is True
         and receipt.get('leader_exit_confirmed') is True
         and receipt.get('cleanup_unconfirmed') is False
         and receipt.get('reap_attempted') is True and receipt.get('reaped') is True
         and receipt.get('app_exit_code') == 0 and receipt.get('app_exit_signal') is None
         and receipt.get('errors') == [],
         f'{key}: observer cleanup/app-exit receipt invalid')
    need(receipt.get('sampled_lower_bound_only') is True
         and receipt.get('whole_app_lifetime_only') is True
         and receipt.get('per_leaf_memory') is False
         and receipt.get('memory_ceiling_qualified') is False,
         f'{key}: memory evidence scope overclaims')
    keys = {'elapsed_ns', 'query_ns', 'phys_footprint_bytes', 'resident_size_bytes'}
    previous = -1
    gaps = []
    for sample in rows:
        need(set(sample) == keys and all(type(sample[k]) is int and sample[k] >= 0 for k in keys),
             f'{key}: raw memory sample shape/value invalid')
        need(sample['elapsed_ns'] > previous, f'{key}: raw sample clock is not increasing')
        if previous >= 0:
            gaps.append(sample['elapsed_ns'] - previous)
        previous = sample['elapsed_ns']
    count = len(rows)
    maxima = {
        'sampled_max_phys_footprint_bytes': max(x['phys_footprint_bytes'] for x in rows),
        'sampled_max_resident_size_bytes': max(x['resident_size_bytes'] for x in rows),
    }
    first, last = rows[0]['elapsed_ns'], rows[-1]['elapsed_ns']
    max_gap = max(gaps, default=0)
    query_sum = sum(x['query_ns'] for x in rows)
    window = receipt.get('sampling_window_elapsed_ns')
    need(receipt.get('sample_count') == count
         and receipt.get('observed_max_phys_footprint_bytes') == maxima['sampled_max_phys_footprint_bytes']
         and receipt.get('observed_max_resident_size_bytes') == maxima['sampled_max_resident_size_bytes']
         and receipt.get('first_sample_start_elapsed_ns') == first
         and receipt.get('last_sample_start_elapsed_ns') == last
         and receipt.get('maximum_sample_start_gap_ns') == max_gap
         and receipt.get('sample_query_sum_ns') == query_sum
         and type(window) is int and window >= last + rows[-1]['query_ns'],
         f'{key}: raw sample counts/timing/gaps/query sum/maxima do not match receipt')
    return {
        'sample_count': count, **maxima, 'first_sample_start_elapsed_ns': first,
        'last_sample_start_elapsed_ns': last, 'maximum_sample_start_gap_ns': max_gap,
        'sample_query_sum_ns': query_sum, 'sampling_window_elapsed_ns': window,
    }


def validate_trials(inputs, binaries):
    timing = read(ROOT / 'timing-trials.json')
    memory = read(ROOT / 'memory-trials.json')
    need(len(timing) == len(memory) == 10, 'expected ten unobserved and ten memory trials')
    expected = pair_order('timing')
    need([(r['pair'], r['kind']) for r in timing] == expected
         and [(r['pair'], r['kind']) for r in memory] == expected,
         'five-pair alternating order mismatch')
    app_hashes = {
        'control': CONTROL_SHA,
        'candidate': CANDIDATE_SHA,
    }
    metrics = {'timing': {}, 'memory': {}}
    all_warnings = []
    validated_reports = []
    for lane, rows in [('timing', timing), ('memory', memory)]:
        expected_lane = 'unobserved-timing' if lane == 'timing' else 'owned-observer-memory'
        for row in rows:
            pair, kind = row['pair'], row['kind']
            key = f'{lane}-{pair}-{kind}'
            need(row.get('lane') == expected_lane and row.get('pause_checks') == 7
                 and row.get('kestrel_checks') == 1
                 and row.get('binary_sha256') == app_hashes[kind]
                 and row.get('authored_paths_bytes_unchanged') is True
                 and row.get('wal_empty') is True,
                 f'{key}: trial guard/counter/hash mismatch')
            need(row.get('load_average', [9])[0] <= 3.0, f'{key}: quiet-load bound failed')
            profile = row.get('profile_marker', {})
            stderr = ROOT / (f'{key}.stderr' if lane == 'timing' else f'{key}-app.stderr')
            need(profile.get('prefix_absent') is True
                 and profile.get('stderr_bytes') == no_profile_marker(stderr),
                 f'{key}: profiler environment marker was emitted')
            preflight = read(ROOT / f'{key}-preflight.json')
            need(preflight.get('ready') is True and preflight.get('samples')
                 and preflight['samples'][-1].get('load_average', [9])[0] <= 3.0
                 and preflight['samples'][-1].get('conflicting_processes') == [],
                 f'{key}: quiet load/process preflight invalid')
            if lane == 'memory':
                need(row.get('source_binary_input_observer_guards') is True
                     and row.get('observer_sha256') == OBSERVER_SHA,
                     f'{key}: memory source/binary/observer guard missing')
                sample = validate_observer(key)
                need(row.get('memory') == sample, f'{key}: memory row does not match raw samples')
            else:
                need(row.get('source_binary_input_guards') is True,
                     f'{key}: timing source/binary/input guard missing')
            before, after = validate_fixture(key)
            report, report_metrics, warnings = validate_report(key, app_hashes[kind], inputs['Cargo.lock'])
            need(report['metadata']['fixture_sha256'] == before['project.sqlite']['sha256']
                 and before['project.sqlite']['sha256'] == after['project.sqlite']['sha256'],
                 f'{key}: fixture and report identities disagree')
            all_warnings.extend(warnings)
            validated_reports.append({'lane': lane, 'pair': pair, 'kind': kind,
                                      'warning_count': len(warnings)})
            if lane == 'timing':
                need(row.get('metrics') == report_metrics, f'{key}: report and trial timing values differ')
                metrics['timing'][(pair, kind)] = report_metrics
            else:
                need(row.get('timing_metrics_observer_contaminated_do_not_pool') == report_metrics,
                     f'{key}: observer timing report values differ')
                metrics['memory'][(pair, kind)] = {
                    'timing_observer_contaminated_do_not_pool': report_metrics,
                    'memory': row['memory'],
                }
    need(len(validated_reports) == 20
         and len(all_warnings) == 40
         and all(x['warning_count'] == 2 for x in validated_reports),
         'expected 20 reports and 40 retained pause warnings')
    command_rows = read(ROOT / 'measurement-commands.json')
    need(len(command_rows) == 40 and all(x.get('exit') == 0 for x in command_rows),
         'measurement command receipt must contain 40 successful commands')
    expected_commands = []
    for lane in ('timing', 'memory'):
        for pair, kind in pair_order(lane):
            stem = f'{lane}-{pair}-{kind}'
            expected_commands += [stem + '-make', stem if lane == 'timing' else stem + '-observer']
    need([x['name'] for x in command_rows] == expected_commands,
         'measurement commands differ from the exact 40-command plan')
    terminal_lines = (ROOT / 'measurement-driver.stdout').read_text().splitlines()
    terminal = json.loads(terminal_lines[-1]) if terminal_lines else {}
    need(terminal.get('done') is True and terminal.get('timing_trials') == 10
         and terminal.get('memory_trials') == 10
         and (ROOT / 'measurement-driver.stderr').read_bytes() == b'',
         'measurement driver has no clean 20-trial terminal receipt')
    timing_summary = read(ROOT / 'timing-summary.json')
    memory_summary = read(ROOT / 'memory-summary.json')
    need(timing_summary.get('diagnostic_only') is True
         and timing_summary.get('no_percentile_or_target_pass') is True
         and timing_summary.get('n_pairs') == 5,
         'timing summary claims scope beyond diagnostic data')
    need(memory_summary.get('diagnostic_only') is True
         and memory_summary.get('no_percentile_or_target_pass') is True
         and memory_summary.get('n_pairs') == 5
         and memory_summary.get('separate_from_unobserved_timing_lane') is True,
         'memory summary claims scope beyond diagnostic data')
    for kind in ('control', 'candidate'):
        for metric in (
            'large_first_pause_input_to_commit_ms',
            'large_first_pause_input_to_offscreen_background_composition_ms',
        ):
            values = [row['metrics'][metric] for row in timing if row['kind'] == kind]
            summary = timing_summary['kinds'][kind][metric]
            expected_stats = {
                'min': min(values), 'median': statistics.median(values), 'max': max(values),
            }
            need(all(abs(float(summary[key]) - value) < 1e-9 for key, value in expected_stats.items()),
                 f'timing {kind}/{metric} summary does not recompute')
        for field in ('sampled_max_phys_footprint_bytes', 'sampled_max_resident_size_bytes'):
            values = [row['memory'][field] for row in memory if row['kind'] == kind]
            summary = memory_summary['kinds'][kind][field]
            expected_stats = {
                'min': min(values), 'median': statistics.median(values), 'max': max(values),
            }
            need(all(abs(float(summary[key]) - value) < 1e-9 for key, value in expected_stats.items()),
                 f'memory {kind}/{field} summary does not recompute')
    return {
        'timing_trials': timing, 'memory_trials': memory,
        'timing_summary': timing_summary, 'memory_summary': memory_summary,
        'reports': validated_reports, 'pause_warning_count': len(all_warnings),
        'metrics': metrics,
    }


def validate_owner_gates(build):
    gate = read(ROOT / 'FINAL-PACKAGING-GO.json')
    need(gate.get('authorized') is True, 'Root FINAL-PACKAGING-GO.json is absent')
    measurement = build['measurement_go']
    # The executed driver includes no instrumentation profile and writes its own 40-row terminal receipt.
    return gate


def include_top(path):
    if path.name in {'source.tar'} or path.suffix.lower() in {
        '.deadpan', '.rlib', '.a', '.dylib', '.so', '.mp4',
    }:
        return False
    if re.match(r'^(timing|memory)-[1-5]-(control|candidate)(?:-|\.)', path.name):
        return False
    return path.suffix.lower() in SAFE_SUFFIXES


def main():
    need(not DEST.exists() and not STAGE.exists(),
         'destination/staging exists; preserving existing evidence')
    source = validate_source_maps()
    base, final, initial, inputs, audit, basis = source
    binaries, control, observer, kestrel = validate_binaries()
    build = validate_build_and_history()
    trial_data = validate_trials(inputs, binaries)
    go = validate_owner_gates(build)

    STAGE.mkdir(parents=True)
    try:
        # Keep small, decision-relevant top-level driver receipts. Prefix-filtering
        # avoids duplicates; the trial payloads are retained once in their lane folders.
        for path in sorted(ROOT.iterdir()):
            if path.is_file() and include_top(path):
                copy_payload(path, Path('raw') / path.name)

        # Initial successful build and the corrected final build are independent histories.
        copy_tree(ROOT / 'initial-build', 'history/initial-build',
                  skip_parts=('test-bin',))
        copy_tree(ROOT / 'resume-preflight-failure', 'history/resume-preflight-failure')
        copy_tree(ROOT / 'worker-notes', 'provenance/worker-notes',
                  allowed=SAFE_SUFFIXES,
                  skip_parts=('all-proposed-file-identities.json', 'before-all-file-identities.json'))
        copy_tree(ROOT / 'test-worker-notes', 'provenance/test-worker-notes')

        # Every fresh timing/observer trial: report, stdout/stderr, exact fixture
        # maps, preflight/load/power evidence, raw sample receipts and samples.
        for lane in ('timing', 'memory'):
            for pair in range(1, 6):
                for kind in ('control', 'candidate'):
                    key = f'{lane}-{pair}-{kind}'
                    for path in sorted(ROOT.iterdir()):
                        if path.is_file() and path.name.startswith(key) and path.suffix.lower() in SAFE_SUFFIXES:
                            copy_payload(path, Path('trials') / lane / key / path.name)
                    copy_tree(ROOT / f'{key}-report', Path('trials') / lane / key / 'report')

        # Baseline, pre-correction build formatting, and final source copies remain .rs.txt.
        for relative in CHANGED:
            final_file = ROOT / 'source' / relative
            final_digest = final[relative]
            need(final_file.is_file() and sha(final_file) == final_digest,
                 f'final source copy differs from frozen map: {relative}')
            copy_payload(final_file, Path('sources/final') / f'{relative}.rs.txt')

            before = ROOT / 'worker-notes/before' / f'{relative}.txt'
            if relative in base:
                need(before.is_file() and sha(before) == base[relative],
                     f'baseline source snapshot mismatch: {relative}')
                copy_payload(before, Path('sources/baseline') / f'{relative}.rs.txt')
            else:
                need(relative in NEW_FILES and relative not in base,
                     f'unexpected absent baseline source: {relative}')
                note = STAGE / 'sources/baseline' / f'{relative}.rs.txt.note'
                note.parent.mkdir(parents=True, exist_ok=True)
                note.write_text('New Rust source path; no baseline file existed.\\n')

            flat_name = relative.replace('/', '__') + '.rs.txt'
            pre_warning = ROOT / 'formatted-sources' / flat_name
            audit_hash = audit.get('initial_formatted_after', {}).get(relative)
            need(pre_warning.is_file() and sha(pre_warning) == audit_hash,
                 f'pre-warning-correction formatted source mismatch: {relative}')
            copy_payload(pre_warning, Path('sources/formatted-before-test-warning-fix') / f'{relative}.rs.txt')

        # Copy the two exact source snapshots that were actually built before the
        # test-only warning fixes; validate against the archived initial map.
        initial_map = read(ROOT / 'initial-build/formatted-source-identities.json')
        for relative in sorted(read(ROOT / 'test-warning-correction-review.json')['before']):
            old = ROOT / 'initial-build/sources' / f'{relative}.txt'
            need(old.is_file() and sha(old) == initial_map[relative],
                 f'initial build source snapshot mismatch: {relative}')
            copy_payload(old, Path('sources/initial-built') / f'{relative}.rs.txt')

        # Reconstruct a final patch against the exact baseline snapshots.
        diff = []
        for relative in CHANGED:
            before = ROOT / 'worker-notes/before' / f'{relative}.txt'
            after = ROOT / 'source' / relative
            old_lines = before.read_text().splitlines(keepends=True) if before.is_file() else []
            new_lines = after.read_text().splitlines(keepends=True)
            diff.extend(difflib.unified_diff(
                old_lines, new_lines, fromfile='a/' + relative, tofile='b/' + relative,
            ))
        (STAGE / 'final-overlay.patch').write_text(''.join(diff))

        # Verified summaries are derived from raw reports/sample streams above.
        verified = {
            'source_commit': BASE_COMMIT,
            'baseline_rust_paths': len(base),
            'final_rust_paths': len(final),
            'changed_paths': CHANGED,
            'new_paths': sorted(NEW_FILES),
            'six_input_hashes': inputs,
            'candidate_app_sha256': binaries['deadpan-app']['sha256'],
            'control_app_sha256': control['sha256'],
            'observer_sha256': observer['sha256'],
            'fixture_helper_sha256': PERF_SHA,
            'build_and_history': build,
            'measurements': {
                'timing_trials': 10, 'separate_memory_trials': 10,
                'timing_pairs': 5, 'memory_pairs': 5,
                'pause_checks': 140, 'kestrel_checks': 20,
                'pause_warnings': trial_data['pause_warning_count'],
                'timing_summary': trial_data['timing_summary'],
                'memory_summary': trial_data['memory_summary'],
                'memory_scope': 'sampled whole-app lifetime lower bound; no exact peak, per-leaf, or ceiling claim',
                'observer_timing_lane_pooled': False,
                'instrumentation_overhead_qualified': False,
                'latency_target_pass': False,
            },
            'schema': {'core': 58, 'sqlite': 87},
            'production_adoption': False,
            'package_go': go,
        }
        canonical(verified, STAGE / 'verified-gates.json')

        files = []
        for path in sorted(STAGE.rglob('*'), key=lambda p: p.as_posix()):
            if not path.is_file() or path == STAGE / 'inventory.json':
                continue
            if path.suffix.lower() == '.gz':
                with gzip.open(path, 'rb') as reader:
                    reader.read()
            files.append({
                'path': path.relative_to(STAGE).as_posix(),
                'bytes': path.stat().st_size,
                'sha256': sha(path),
            })
        inventory = {
            'total_files': len(files),
            'total_bytes': sum(row['bytes'] for row in files),
            'files': files,
        }
        canonical(inventory, STAGE / 'inventory.json')
        need(read(STAGE / 'inventory.json') == inventory, 'inventory reread mismatch')
        for row in files:
            path = STAGE / row['path']
            need(path.stat().st_size == row['bytes'] and sha(path) == row['sha256'],
                 f"payload hash/size reread mismatch: {row['path']}")
        inventory_hash = sha(STAGE / 'inventory.json')
        need(not DEST.exists(), 'destination appeared while staging')
        STAGE.rename(DEST)
        print(json.dumps({
            'destination': str(DEST), 'total_files': len(files),
            'total_bytes': inventory['total_bytes'],
            'inventory_sha256': inventory_hash,
            'trials': 20, 'pause_checks': 140, 'kestrel_checks': 20,
            'pause_warnings': trial_data['pause_warning_count'],
        }, sort_keys=True))
    except Exception:
        shutil.rmtree(STAGE, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
