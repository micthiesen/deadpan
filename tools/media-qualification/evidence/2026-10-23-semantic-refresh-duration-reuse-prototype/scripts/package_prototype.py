#!/usr/bin/env python3
"""Bounded packager for a private semantic-refresh prototype; root runs after gates finish."""
from __future__ import annotations
import argparse, gzip, hashlib, json, re, shutil, statistics
from pathlib import Path

S = Path('/tmp/deadpan-semantic-refresh-duration-reuse-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
PRIOR = Path('/tmp/deadpan-full-timing-closure-reuse-qualification-2026-10-23')
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-semantic-refresh-duration-reuse-prototype'
BASE = '56f748565189dd2c759f927f18c845faec3d1160'
CONTROL_APP = '2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608'
SELECTION = 'crates/deadpan-core/src/semantic/planner/selection.rs'
NEW_TEST = 'crates/deadpan-core/tests/semantic_refresh.rs'
CHANGED = {SELECTION, NEW_TEST}
EXCLUDED_ROOT_SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
LIMIT = 100_000


def need(ok, message):
    if not ok:
        raise SystemExit(message)


def load(path):
    try:
        return json.loads(path.read_text())
    except Exception as exc:
        raise SystemExit(f'missing/invalid JSON {path}: {exc}')


def required(path):
    need(path.is_file(), f'missing final evidence: {path}')
    return path


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def phase_receipt(path, expected_names):
    rows = load(required(path))
    need([row.get('name') for row in rows] == expected_names,
         f'{path.name}: phase names/order differ')
    need(all(row.get('exit') == 0 for row in rows), f'{path.name}: a required phase failed')
    return rows


def verify_focus_tests():
    receipt = phase_receipt(S / 'build-resumed-receipt.json',
                            ['format', 'semantic-refresh-tests', 'semantic-trim-tests',
                             'command-work-tests', 'release-build'])
    failure = load(required(S / 'first-compile-failure/failure.json'))
    need(failure.get('stage') == 'compilation of standalone semantic_refresh integration target; no tests executed'
         and failure.get('production_change') is False
         and 'E0624' in failure.get('error', '')
         and 'current_revision' in failure.get('correction', '')
         and 'construct identical' in failure.get('correction', ''),
         'initial compile failure/correction record does not match the retained history')
    initial = load(required(S / 'build-receipt.json'))
    need([row.get('name') for row in initial] == ['clone-release-cache', 'format', 'semantic-refresh-tests']
         and [row.get('exit') for row in initial] == [0, 0, 101],
         'initial build history differs from the retained first compile failure')
    first_fail = load(required(S / 'first-compile-failure/build-receipt.json'))
    need(first_fail and first_fail[-1].get('exit') == 101,
         'first compile failure run receipt is missing or does not show exit 101')
    failed_map = load(required(S / 'first-compile-failure/build-source-identities.json'))
    final_map = load(required(S / 'build-source-identities.json'))
    need(set(failed_map) == set(final_map)
         and {path for path in final_map if failed_map[path] != final_map[path]} == {NEW_TEST},
         'compile correction changed production or unrelated source')
    failed_source = required(S / 'first-compile-failure' / (NEW_TEST + '.txt'))
    need(sha_bytes(failed_source.read_bytes()) == failed_map.get(NEW_TEST),
         'retained failed test source does not match the failed compile source map')

    binaries = load(required(S / 'focused-binaries.json'))
    expected = {'semantic-refresh-tests': 4, 'semantic-trim-tests': 3, 'command-work-tests': 3}
    need({row.get('step') for row in binaries} == set(expected), 'focused test executable rows incomplete')
    for row in binaries:
        retained = required(Path(row['retained_executed_copy']))
        need(sha(retained) == row.get('sha256'), f"retained executed test binary hash mismatch: {row['step']}")
        need(Path(row['source']).is_file() and sha(Path(row['source'])) == row['sha256'],
             f"executed test binary differs from retained copy: {row['step']}")
        result_rows = row.get('test_results', [])
        passed = sum(int(values[0]) for values in result_rows)
        need(passed == expected[row['step']] and all(values[1:] == ['0', '0'] for values in result_rows),
             f"focused test counts differ: {row['step']}")
    return {'resumed_steps': len(receipt), 'initial_failure_exit': 101,
            'focused_tests': expected, 'executed_test_binaries': len(binaries)}


def verify_sources():
    original = load(required(S / 'original-source-identities.json'))
    final = load(required(S / 'build-source-identities.json'))
    note = load(required(S / 'source-basis.json'))
    input_ids = load(required(S / 'input-identities.json'))
    need(note.get('base') == BASE and note.get('qualified_control_app_sha256') == CONTROL_APP,
         'source-basis baseline or control identity mismatch')
    need(len(original) == 2050 and len(final) == 2051, 'Rust source set size mismatch')
    need(set(final) == set(original) | {NEW_TEST}, 'new test path is absent or an unexpected path appeared')
    changed = {path for path in final if final[path] != original.get(path)}
    need(changed == CHANGED and original.get(NEW_TEST) is None,
         'candidate source differs from baseline outside the declared one-source/new-test change')
    need(len(input_ids) == 6, 'expected six guarded inputs')
    audit = load(required(S / 'formatted-overlay-audit.json'))
    need(set(audit.get('changed', [])) == CHANGED and audit.get('nonowned_rust_unchanged') is True
         and audit.get('inputs_unchanged') is True, 'formatted source overlay audit failed')
    source = S / 'source'
    actual = {str(path.relative_to(source)): sha(path) for path in source.rglob('*.rs')}
    need(actual == final, 'private formatted source tree differs from frozen build source map')
    for path, expected_hash in input_ids.items():
        need(sha(source / path) == expected_hash, f'private input identity changed: {path}')
    for path, expected_hash in final.items():
        need(sha(source / path) == expected_hash, f'private source identity changed: {path}')
    return original, final, input_ids


def verify_build_binaries():
    bins = load(required(S / 'binaries.json'))
    need(set(bins) == {'deadpan-app', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track'},
         'release binary manifest is incomplete')
    for name, row in bins.items():
        for field in ('source', 'executed'):
            executable = required(Path(row[field]))
            need(sha(executable) == row['sha256'], f'{name} {field} hash mismatch')
    need(bins['deadpan-app'].get('freshly_built') is True,
         'candidate app was not built in this private source tree')
    need(bins['deadpan-app']['sha256'] != CONTROL_APP,
         'prototype app unexpectedly equals its qualified control binary')
    prior = load(required(PRIOR / 'release-native-binaries.json'))
    control = prior['deadpan-app']
    need(control.get('sha256') == CONTROL_APP and sha(Path(control['executed'])) == CONTROL_APP,
         'qualified control app does not match the pinned source evidence')
    measure_manifest = load(required(S / 'binaries.json'))
    need(measure_manifest == bins, 'measurement binary manifest differs from release build')
    control_sources = load(required(PRIOR / 'release-native-source-identities.json'))
    need(control_sources == load(required(S / 'original-source-identities.json')),
         'qualified control source map differs from prototype baseline')
    need(load(required(PRIOR / 'release-native-lock.json')) == load(required(S / 'input-identities.json')),
         'qualified control inputs differ from prototype baseline')
    perf = required(PERF)
    need(sha(perf) == 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166',
         'fixture-generation helper identity changed')
    return bins, control


def verify_measurement(bins, control):
    commands = load(required(S / 'commands.json'))
    expected_commands = []
    for pair in range(1, 6):
        order = ('control', 'prototype') if pair % 2 else ('prototype', 'control')
        for kind in order:
            key = f'{pair}-{kind}'
            expected_commands.extend([key + '-make', key])
    need([row.get('name') for row in commands] == expected_commands
         and len(commands) == 20 and all(row.get('exit') == 0 for row in commands),
         'measurement must contain exactly twenty successful alternating make/run commands')

    trials = load(required(S / 'trials.json'))
    summary = load(required(S / 'summary.json'))
    expected_pairs = {(pair, kind) for pair in range(1, 6) for kind in ('control', 'prototype')}
    need(len(trials) == 10 and {(row.get('pair'), row.get('kind')) for row in trials} == expected_pairs,
         'measurement must contain exactly five control/prototype pairs')
    need(summary.get('n_per_kind') == 5 and summary.get('prototype_only') is True
         and summary.get('no_percentile_or_target_pass') is True,
         'summary lacks prototype-only diagnostic scope')

    for row in trials:
        pair, kind = row['pair'], row['kind']
        key = f'{pair}-{kind}'
        expected_app = control['sha256'] if kind == 'control' else bins['deadpan-app']['sha256']
        need(row.get('checks') == 7 and row.get('kestrel_checks') == 1
             and row.get('source_and_binary_guards') is True
             and row.get('authored_inputs_unchanged') is True and row.get('empty_wal') is True
             and len(row.get('load', [])) == 3 and row['load'][0] <= 3.0,
             f'timing row guard failed: {key}')
        need(row.get('binary_sha256') == expected_app, f'timing row app identity differs: {key}')
        before = load(required(S / f'{key}-input-before.json'))
        after = load(required(S / f'{key}-input-after.json'))
        before_authored = {name: value for name, value in before.items() if name not in EXCLUDED_ROOT_SIDECARS}
        after_authored = {name: value for name, value in after.items() if name not in EXCLUDED_ROOT_SIDECARS}
        need(before_authored == after_authored, f'authored fixture changed: {key}')
        need(before['project.sqlite']['sha256'] == after['project.sqlite']['sha256']
             and after.get('project.sqlite-wal', {}).get('bytes', 0) == 0,
             f'fixture database identity/WAL guard failed: {key}')
        report = load(required(S / f'{key}-report/report.json'))
        metadata = report.get('metadata', {})
        need(report.get('failed') is False and metadata.get('binary_sha256') == expected_app
             and metadata.get('build_profile') == 'release'
             and metadata.get('fixture_sha256') == before['project.sqlite']['sha256'] == after['project.sqlite']['sha256'],
             f'timing report binary/profile/fixture mismatch: {key}')
        pause = [scenario for scenario in report.get('scenarios', []) if scenario.get('name') == 'large-first-pause']
        kestrel = [scenario for scenario in report.get('scenarios', []) if scenario.get('name') == 'kestrel-shortcuts']
        need(len(pause) == 1 and len(pause[0].get('checks', [])) == 7
             and all(check.get('passed') is True for check in pause[0]['checks']),
             f'seven pause checks missing or failed: {key}')
        need(len(kestrel) == 1 and len(kestrel[0].get('checks', [])) == 1
             and all(check.get('passed') is True for check in kestrel[0]['checks']),
             f'one passing Kestrel check missing: {key}')
    metrics = set(trials[0].get('metrics', {}))
    need(metrics and all(set(row.get('metrics', {})) == metrics for row in trials),
         'timing metric sets differ between trials')
    for kind in ('control', 'prototype'):
        subset = [row for row in trials if row['kind'] == kind]
        need(len(subset) == 5 and set(summary.get('kinds', {}).get(kind, {})) == metrics,
             f'summary metric set/count mismatch: {kind}')
        for metric in metrics:
            values = [row['metrics'][metric] for row in subset]
            expected = {'min': min(values), 'median': statistics.median(values), 'max': max(values)}
            recorded = summary['kinds'][kind][metric]
            need(set(recorded) == set(expected)
                 and all(abs(float(recorded[name]) - expected[name]) < 1e-9 for name in expected),
                 f'summary values do not recompute: {kind}/{metric}')
    return {'commands': len(commands), 'trials': len(trials), 'metrics': sorted(metrics),
            'diagnostic_only': True, 'candidate_app_sha256': bins['deadpan-app']['sha256'],
            'control_app_sha256': control['sha256'], 'summary': summary}


def verify_all():
    focus = verify_focus_tests()
    original, final, inputs = verify_sources()
    bins, control = verify_build_binaries()
    measurement = verify_measurement(bins, control)
    independent = load(required(S / 'independent-review.json'))
    sealed = load(required(S / 'review-sealed-scope.json'))
    need(independent.get('status') == 'no_findings' and independent.get('findings') == [],
         'independent review has findings or is incomplete')
    need(sealed.get('status') == 'no_findings' and sealed.get('findings', []) == [],
         'sealed-scope review has findings or is incomplete')
    return {'baseline': BASE, 'original_rust_files': len(original), 'candidate_rust_files': len(final),
            'changed_sources': sorted(CHANGED), 'guarded_inputs': len(inputs), 'focus': focus,
            'measurement': measurement, 'reviews': ['no findings', 'no findings'],
            'core_schema': 58, 'sqlite_schema': 87}


def put_bytes(data, destination, kind):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if len(data) > LIMIT and kind in {'json', 'txt', 'md', 'log', 'stdout', 'stderr', 'html'}:
        output = Path(str(destination) + '.gz')
        with output.open('wb') as stream:
            with gzip.GzipFile(filename='', mode='wb', fileobj=stream, compresslevel=9, mtime=0) as zipped:
                zipped.write(data)
        with gzip.open(output, 'rb') as zipped:
            need(zipped.read() == data, f'compression verification failed: {destination}')
    else:
        destination.write_bytes(data)


def copy_file(source, destination):
    put_bytes(source.read_bytes(), destination, source.suffix.lstrip('.'))


def include(path):
    parts = path.parts
    if parts[0] in {'packaging-draft', 'source', 'target', 'bin', 'focused-bin'}:
        return False
    if any(part in {'target', 'bin', 'focused-bin'} for part in parts):
        return False
    if any(part.endswith(('.deadpan', '.tar', '.tar.gz', '.tar.zst', '.mp4')) for part in parts):
        return False
    return path.suffix in {'.json', '.py', '.md', '.txt', '.log', '.stdout', '.stderr', '.patch', '.html'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--finalize', action='store_true')
    args = parser.parse_args()
    need(args.finalize, 'root must explicitly run with --finalize after all prototype and measurement drivers are terminal')
    verified = verify_all()
    stage = DEST.with_name(DEST.name + '.staging')
    need(not DEST.exists() and not stage.exists(), 'destination or staging path already exists')
    stage.mkdir(parents=True)
    try:
        for path in sorted(S.rglob('*')):
            if path.is_file() and include(path.relative_to(S)):
                copy_file(path, stage / 'raw' / path.relative_to(S))
        # Preserve this exact packager as part of its reproducible provenance.
        put_bytes(Path(__file__).read_bytes(), stage / 'scripts/package_prototype.py', 'py')

        # Keep just the relevant source snapshots as text, never raw .rs files.
        original = load(S / 'original-source-identities.json')
        final = load(S / 'build-source-identities.json')
        original_selection = required(S / 'worker-notes/before' / (SELECTION + '.txt'))
        need(sha_bytes(original_selection.read_bytes()) == original[SELECTION],
             'original selection source snapshot differs from baseline map')
        put_bytes(original_selection.read_bytes(), stage / 'sources/original/selection.rs.txt', 'txt')
        proposed_selection = required(S / 'worker-notes/after' / (SELECTION + '.txt'))
        proposed_test = required(S / 'worker-notes/after' / (NEW_TEST + '.txt'))
        proposal_hashes = load(required(S / 'worker-notes/source-hashes.json'))['proposed']
        need(sha_bytes(proposed_selection.read_bytes()) == proposal_hashes[SELECTION]
             and sha_bytes(proposed_test.read_bytes()) == proposal_hashes[NEW_TEST],
             'proposed source snapshots differ from worker hash record')
        put_bytes(proposed_selection.read_bytes(), stage / 'sources/proposed/selection.rs.txt', 'txt')
        put_bytes(proposed_test.read_bytes(), stage / 'sources/proposed/semantic_refresh.rs.txt', 'txt')
        failed_test = required(S / 'first-compile-failure' / (NEW_TEST + '.txt'))
        put_bytes(failed_test.read_bytes(), stage / 'sources/historical-failed-test/semantic_refresh.rs.txt', 'txt')
        for source_path, label in ((SELECTION, 'selection.rs.txt'), (NEW_TEST, 'semantic_refresh.rs.txt')):
            source = required(S / 'source' / source_path)
            need(sha(source) == final[source_path], f'formatted final source differs from frozen map: {source_path}')
            put_bytes(source.read_bytes(), stage / 'sources/final' / label, 'txt')
        need(NEW_TEST not in original,
             'unexpected baseline test source exists; do not fabricate a before snapshot')
        (stage / 'sources/new-test-baseline.json').write_text(json.dumps(
            {'path': NEW_TEST, 'present_in_original_source_map': False,
             'historical_first_compile_attempt_retained': True}, indent=2) + '\n')

        # Keep the exact pre-adoption control manifest and its binary identity, not executable bytes.
        control_manifest = required(PRIOR / 'release-native-binaries.json')
        copy_file(control_manifest, stage / 'control/release-native-binaries.json')
        copy_file(required(PRIOR / 'release-native-source-identities.json'),
                  stage / 'control/release-native-source-identities.json')
        # Copy only the fixture-helper identity record; its executable is not retained.
        put_bytes(json.dumps({'path': str(PERF), 'sha256': sha(PERF)}, indent=2).encode() + b'\n',
                  stage / 'control/fixture-helper-identity.json', 'json')
        verified_path = stage / 'verified-gates.json'
        verified_path.write_text(json.dumps(verified, indent=2) + '\n')

        rows = []
        for path in sorted(stage.rglob('*')):
            if path.is_file() and path != stage / 'inventory.json':
                rows.append({'path': path.relative_to(stage).as_posix(), 'bytes': path.stat().st_size,
                             'sha256': sha(path)})
                if path.suffix == '.gz':
                    with gzip.open(path, 'rb') as zipped:
                        zipped.read()
        inventory = {'files': rows, 'total_files': len(rows),
                     'total_bytes': sum(row['bytes'] for row in rows)}
        (stage / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
        for row in rows:
            path = stage / row['path']
            need(path.is_file() and path.stat().st_size == row['bytes'] and sha(path) == row['sha256'],
                 f'inventory reread mismatch: {row["path"]}')
        inventory_hash = sha(stage / 'inventory.json')
        DEST.parent.mkdir(parents=True, exist_ok=True)
        stage.rename(DEST)
        print(json.dumps({'evidence': str(DEST), 'total_files': len(rows),
                          'total_bytes': inventory['total_bytes'],
                          'inventory_sha256': inventory_hash}, indent=2))
    except Exception:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
