#!/usr/bin/env python3
"""Fail-closed package draft for V3; run only after Root's hash-bound packaging GO."""
from __future__ import annotations

import gzip
import hashlib
import json
import math
import os
import shutil
import statistics
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

S = Path('/tmp/deadpan-insert-split-proof-prototype-v3-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-insert-split-proof-prototype'
LIMIT = 100_000
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
APPROVED: dict[str, tuple[int, str]] = {}
METRICS = {
    'large_first_pause_input_to_commit_ms',
    'large_first_pause_input_to_offscreen_background_composition_ms'
}
ORDERS = [
    ['control', 'baseline', 'candidate'], ['baseline', 'candidate', 'control'],
    ['candidate', 'control', 'baseline'], ['control', 'candidate', 'baseline'],
    ['candidate', 'baseline', 'control'], ['baseline', 'control', 'candidate']
]


def sha(path: Path) -> str:
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def read(name: str):
    return json.loads((S / name).read_text(encoding='utf-8'))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def finite_nonnegative(value) -> bool:
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def put(src: Path, stage: Path, rel: str, compressed: list[dict], force_gzip: bool = False) -> None:
    src = Path(src)
    require(src.is_file() and not src.is_symlink(), f'missing/unsafe payload: {src}')
    key = str(src.resolve(strict=True))
    require(key in APPROVED, f'payload has no pre-copy approved digest: {src}')
    approved = APPROVED[key]
    require((src.stat().st_size, sha(src)) == approved, f'approved payload changed before copy: {src}')
    dst = stage / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    compressible = src.suffix.lower() in {'.json', '.txt', '.md', '.log', '.stdout', '.stderr', '.patch', '.py'}
    if compressible and (force_gzip or src.stat().st_size > LIMIT):
        dst = dst.with_name(dst.name + '.gz')
        with src.open('rb') as inp, dst.open('xb') as raw:
            with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as out:
                shutil.copyfileobj(inp, out)
        h, n = hashlib.sha256(), 0
        with gzip.open(dst, 'rb') as inp:
            for block in iter(lambda: inp.read(1 << 20), b''):
                h.update(block); n += len(block)
        require(n == approved[0] and h.hexdigest() == approved[1], f'gzip roundtrip/approved digest mismatch: {rel}')
        compressed.append({'source': str(src), 'stored': str(dst.relative_to(stage)),
                           'decoded_bytes': n, 'decoded_sha256': h.hexdigest(),
                           'stored_sha256': sha(dst)})
    else:
        shutil.copy2(src, dst)
        require(dst.stat().st_size == approved[0] and sha(dst) == approved[1],
                f'copied payload differs from approved digest: {rel}')


def copy_if(src: Path, stage: Path, rel: str, compressed: list[dict]) -> None:
    if src.is_file():
        put(src, stage, rel, compressed)


def verify_source() -> tuple[dict, dict]:
    base = read('baseline-sources.json')
    current = read('frozen-sources.json')
    inputs = read('baseline-inputs.json')
    provenance = read('baseline-provenance.json')
    require(len(base) == len(current) == 2054, 'V3/C Rust source map size mismatch')
    require(provenance.get('source_map_sha256') == sha(S / 'baseline-sources.json')
            and provenance.get('unchanged_rust_files') == 2054, 'C baseline source provenance mismatch')
    require(provenance.get('qualified_control_sha256') == read('baseline.json')['control']['sha256'],
            'C qualified control provenance mismatch')
    require(sha(Path(provenance['matched_diagnostic_app'])) == provenance['matched_diagnostic_app_sha256'],
            'C diagnostic baseline binary mismatch')
    require(set(base) == set(current), 'V3/C source paths differ')
    delta = {p for p in base if base[p] != current[p]}
    require(delta == {'crates/deadpan-core/src/insert_time.rs'}, f'unexpected source delta: {sorted(delta)}')
    require(len(inputs) == 6 and inputs == read('baseline-inputs.json'), 'six-input map malformed')
    live = {str(p.relative_to(S / 'source')): sha(p)
            for p in sorted((S / 'source').rglob('*.rs'))}
    require(live == current, 'current private source differs from frozen map')
    matched_root = Path(provenance['source'])
    baseline_live = {str(p.relative_to(matched_root)): sha(p) for p in sorted(matched_root.rglob('*.rs'))}
    require(baseline_live == base, 'V3 baseline Rust map does not match its retained C source root')
    for rel, digest in inputs.items():
        require(sha(S / 'source' / rel) == digest and sha(matched_root / rel) == digest,
                f'pinned input differs from qualified C baseline: {rel}')
    review = read('source-review.json')
    require(review.get('status') == 'no_findings' and not review.get('findings')
            and review.get('source_map_sha256') == sha(S / 'frozen-sources.json'),
            'final source review missing, non-clear, or not bound to V3 map')
    require(review.get('reviewed_path_hashes') == {'crates/deadpan-core/src/insert_time.rs': current['crates/deadpan-core/src/insert_time.rs']},
            'source review does not bind the single changed file')
    require(review.get('patch_sha256') in (None, sha(S / 'prototype.patch')),
            'source review patch binding mismatch')
    require((S / 'source/crates/deadpan-core/src/insert_time.rs').is_file(), 'changed source absent')
    require(current['crates/deadpan-core/src/insert_time.rs'] == sha(S / 'source/crates/deadpan-core/src/insert_time.rs'),
            'changed Rust file does not match final source map')
    return current, inputs


def verify_build() -> dict:
    expected = read('expected-build-stages.json')
    receipt = read('build-receipt.json')
    terminal = read('build-terminal.json')
    require(len(expected) == 10 and [r.get('name') for r in receipt] == expected,
            'V3 build stage sequence mismatch')
    require(all(r.get('exit') == 0 and r.get('validation') == 'passed' for r in receipt),
            'V3 build contains incomplete or failed stage')
    require(terminal.get('status') == 'complete' and terminal.get('steps') == 10
            and terminal.get('stage_names') == expected, 'V3 build is not complete')
    binaries = read('binaries.json')
    require(set(binaries) == {'deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track'},
            'final application/helper binary manifest set mismatch')
    for name, row in binaries.items():
        require(set(row) == {'source','executed','sha256','freshly_built','qualified_helper_reuse'},
                f'unexpected final binary metadata shape: {name}')
        require(type(row['sha256']) is str and len(row['sha256']) == 64, f'invalid binary digest: {name}')
        require(Path(row['source']).is_file() and sha(Path(row['source'])) == row['sha256'],
                f'live binary source changed: {name}')
        require(Path(row['executed']).is_file() and sha(Path(row['executed'])) == row['sha256'],
                f'executed binary changed: {name}')
        require(row['freshly_built'] == (name == 'deadpan-app')
                and row['qualified_helper_reuse'] == (name != 'deadpan-app'),
                f'binary origin flags mismatch: {name}')
    require(terminal.get('app_sha256') == binaries['deadpan-app']['sha256'],
            'V3 terminal app hash mismatch')
    return receipt

def verify_tests() -> None:
    expected = read('expected-tests.json')
    checks = {
        'core-split-proof': (6, expected['core-split-proof']),
        'core-wide-reference': (1, expected['core-wide-reference']),
        'store-preparation': (12, expected['store-preparation'])
    }
    import re
    for stage, (count, names) in checks.items():
        stdout = (S / (stage + '.stdout')).read_text()
        found = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', stdout, re.M)
        summaries = re.findall(r'^test result: ok\. (\d+) passed; 0 failed; 0 ignored;', stdout, re.M)
        require(sorted(found) == sorted(names) and summaries == [str(count)], f'{stage} test output mismatch')
    integration = (S / 'core-integration.stdout').read_text()
    totals = re.findall(r'^test result: ok\. (\d+) passed; 0 failed; 0 ignored;', integration, re.M)
    names = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', integration, re.M)
    require(len(totals) == 3 and len(names) == sum(map(int, totals))
            and sum(map(int, totals)) == 22 and all(int(x) >= 1 for x in totals),
            'Core integration output does not contain three binaries and 22 passing tests')
    bins = read('test-binaries.json')
    expected_binary_stages = {'core-unit-build', 'core-split-proof', 'core-wide-reference', 'core-integration', 'store-preparation'}
    require(set(bins) == expected_binary_stages, 'test-binary manifest stage set mismatch')
    for stage, stage_record in bins.items():
        records = stage_record.values() if isinstance(stage_record, dict) and 'sha256' not in stage_record else [stage_record]
        for record in records:
            require(sha(Path(record['retained'])) == record['sha256'], f'retained binary mismatch: {stage}')
            if record.get('executed'):
                require(sha(Path(record['executed'])) == record['sha256'], f'executed binary mismatch: {stage}')
    for stage, (_, _) in checks.items():
        require(stage in bins and bins[stage], f'missing recorded executable(s): {stage}')
        entries = bins[stage].values() if isinstance(bins[stage], dict) and 'sha256' not in bins[stage] else [bins[stage]]
        for record in entries:
            require(sha(Path(record['retained'])) == record['sha256'], f"retained binary mismatch: {stage}")
            if record.get('executed'):
                require(sha(Path(record['executed'])) == record['sha256'], f"executed binary mismatch: {stage}")


def verify_active_pins() -> dict:
    pins = read('driver-pins.json')
    required = {'build.py','measure.py','check_profile.py',
                'expected-build-stages.json','expected-tests.json','frozen-sources.json',
                'baseline-sources.json','baseline-inputs.json','baseline-provenance.json',
                'source-review.json','prototype.patch','previous-attempts.json','harness-review.json'}
    require(required <= set(pins), f'active driver pin map missing entries: {sorted(required - set(pins))}')
    for rel, digest in pins.items():
        pp = PurePosixPath(rel)
        require(not pp.is_absolute() and '..' not in pp.parts, f'unsafe active pin path: {rel}')
        path = S / rel
        require(path.is_file() and not path.is_symlink() and sha(path) == digest,
                f'active pinned driver/protocol changed: {rel}')
    return pins


def verify_review_and_go() -> tuple[dict, dict, dict]:
    pins = verify_active_pins()
    harness = read('harness-review.json')
    require(harness.get('status') == 'no_findings' and not harness.get('findings'),
            'V3 root carry-forward harness review is not clear')
    expected_reviewed = {
        'build.py': sha(S / 'build.py'), 'measure.py': sha(S / 'measure.py'),
        'expected-build-stages.json': sha(S / 'expected-build-stages.json'),
        'expected-tests.json': sha(S / 'expected-tests.json'),
        'frozen-sources.json': sha(S / 'frozen-sources.json')
    }
    require(harness.get('reviewed') == expected_reviewed,
            'V3 harness review does not bind exact drivers/manifests/source map')
    require(harness.get('source_review_inherited_byte_identical') == sha(S / 'source-review.json'),
            'V3 inherited source review byte identity mismatch')
    source_review = read('source-review.json')
    require(source_review.get('status') == 'no_findings' and not source_review.get('findings')
            and source_review.get('source_map_sha256') == sha(S / 'frozen-sources.json'),
            'V3 source review is not clear/source-bound')
    analysis_review = read('analysis-review.json')
    require(analysis_review.get('status') == 'no_findings' and not analysis_review.get('findings')
            and analysis_review.get('script_sha256') == sha(S / 'analyse_pairs.py'),
            'paired-analysis review is not clear/bound to its producer')
    package_review = read('packager-final-review.json')
    require(package_review.get('status') == 'no_findings' and not package_review.get('findings')
            and package_review.get('script_sha256') == sha(Path(__file__)),
            'final packager review is not clear/bound to this exact script')
    go = read('PACKAGING-GO.json')
    required = {
        'status': 'go', 'packager_sha256': sha(Path(__file__)),
        'packager_review_sha256': sha(S / 'packager-final-review.json'),
        'source_map_sha256': sha(S / 'frozen-sources.json'),
        'source_review_sha256': sha(S / 'source-review.json'),
        'harness_review_sha256': sha(S / 'harness-review.json'),
        'analysis_review_sha256': sha(S / 'analysis-review.json'),
        'build_terminal_sha256': sha(S / 'build-terminal.json'),
        'measurement_terminal_sha256': sha(S / 'measurement-terminal.json'),
        'baseline_provenance_sha256': sha(S / 'baseline-provenance.json'),
        'input_map_sha256': sha(S / 'baseline-inputs.json'),
        'driver_pins_sha256': sha(S / 'driver-pins.json'),
        'binaries_manifest_sha256': sha(S / 'binaries.json'),
        'paired_analysis_sha256': sha(S / 'paired-analysis.json')
    }
    for key, value in required.items():
        require(go.get(key) == value, f'Root packaging GO is missing/mismatched {key}')
    main_ref = go.get('current_main_reference')
    require(isinstance(main_ref, dict), 'GO must bind the qualified current Main reference')
    for key in ('source_map_path','source_map_sha256','provenance_path','provenance_sha256',
                'binary_manifest_path','binary_manifest_sha256','code_commit','documentation_commit'):
        require(main_ref.get(key), f'GO current Main reference missing {key}')
    for key in ('source_map_path','provenance_path','binary_manifest_path'):
        rel = PurePosixPath(main_ref[key])
        require(not rel.is_absolute() and '..' not in rel.parts, f'unsafe Main reference path: {key}')
        path = S / rel
        require(path.is_file() and sha(path) == main_ref[key.replace('_path','_sha256')],
                f'GO current Main reference file mismatch: {key}')
    main_map = json.loads((S / main_ref['source_map_path']).read_text())
    require(len(main_map) == 2053, 'qualified Main Rust map must contain the 2053 production files')
    main_provenance = json.loads((S / main_ref['provenance_path']).read_text())
    require(main_provenance.get('copied_reference_sha256') == main_ref['source_map_sha256']
            and main_provenance.get('qualified_code_commit') == main_ref['code_commit']
            and main_provenance.get('main_documentation_commit_at_check') == main_ref['documentation_commit']
            and main_provenance.get('rust_files') == 2053
            and main_provenance.get('current_main_visible_rust_map_matches') is True
            and main_provenance.get('six_inputs_match') is True,
            'qualified Main source/provenance/commit identities disagree')
    binary_manifest = json.loads((S / main_ref['binary_manifest_path']).read_text())
    qualified_control = read('baseline.json')['control']
    require(binary_manifest.get('deadpan-app', {}).get('sha256') == qualified_control['sha256']
            and binary_manifest.get('deadpan-app', {}).get('executed') == qualified_control['path'],
            'qualified Main release binary manifest does not match pinned control')
    return harness, go, pins

def parse_metrics(report: dict, binary: dict, inputs: dict, before: dict) -> tuple[dict, int]:
    require(report.get('failed') is False, 'performance report failed')
    metadata = report['metadata']
    require(metadata.get('binary_sha256') == binary['sha256'] and metadata.get('build_profile') == 'release',
            'performance report binary/profile mismatch')
    require(metadata.get('fixture_sha256') == before['project.sqlite']['sha256']
            and metadata.get('cargo_lock_sha256_at_run') == inputs['Cargo.lock'], 'fixture/lock mismatch')
    require(metadata.get('physical_display_measured') is False
            and metadata.get('gpu_readback_in_timing_run') is False, 'unexpected display/readback claim')
    require(str(metadata.get('rust', '')).startswith('rustc 1.97.1 '), 'report Rust version mismatch')
    scenarios = report['scenarios']
    require([x['name'] for x in scenarios] == ['kestrel-shortcuts', 'large-first-pause'], 'scenario order mismatch')
    kestrel, pause = scenarios
    require(len(kestrel['checks']) == 1 and kestrel['checks'][0]['passed'] is True, 'Kestrel check failed')
    require(len(pause['checks']) == 7 and all(c['passed'] is True for c in pause['checks']), 'pause checks failed')
    timing = [r for r in pause['timings'] if r['name'].startswith('large_first_pause')]
    require(len(timing) == 2 and {r['name'] for r in timing} == METRICS, 'timing metric set mismatch')
    values = {r['name']: r['summary']['completed']['p50_ms'] for r in timing}
    require(all(finite_nonnegative(v) for v in values.values()), 'invalid timing value')
    warnings = sum(len(scenario.get('findings', [])) for scenario in scenarios)
    return values, warnings


def authored_snapshot(root: Path) -> dict:
    root = root.resolve(strict=True)
    result = {}
    for path in sorted(root.rglob('*')):
        rel = path.relative_to(root).as_posix()
        require(not path.is_symlink(), f'symlink in fixture: {rel}')
        if path.is_dir():
            continue
        require(path.is_file(), f'non-regular fixture member: {rel}')
        if rel in SIDECARS:
            if rel == 'project.sqlite-wal':
                require(path.stat().st_size == 0, 'root SQLite WAL is not empty')
            continue
        result[rel] = {'bytes': path.stat().st_size, 'sha256': sha(path)}
        if APPROVED:
            key = str(path.resolve(strict=True))
            require(key in APPROVED and APPROVED[key] == (path.stat().st_size, result[rel]['sha256']),
                    f'fixture member differs from approved pre-copy digest: {rel}')
    return result


def safe_fixture_archive(root: Path, dst: Path, expected: dict) -> dict:
    root = root.resolve(strict=True)
    require(root.is_dir() and not root.is_symlink(), f'unsafe fixture root: {root}')
    require(authored_snapshot(root) == expected, 'fixture changed before archive creation')
    members = []
    with tarfile.open(dst, 'w:gz') as tar:
        for path in sorted(root.rglob('*')):
            rel = path.relative_to(root).as_posix()
            pp = PurePosixPath(rel)
            require(not pp.is_absolute() and '..' not in pp.parts, f'unsafe fixture member: {rel}')
            require(not path.is_symlink(), f'symlink in fixture: {rel}')
            if path.is_dir():
                continue
            require(path.is_file(), f'non-regular fixture member: {rel}')
            if rel in SIDECARS:
                if rel == 'project.sqlite-wal':
                    require(path.stat().st_size == 0, 'root SQLite WAL is not empty')
                continue
            expected_item = expected.get(rel)
            require(expected_item == {'bytes': path.stat().st_size, 'sha256': sha(path)},
                    f'authored fixture member changed: {rel}')
            require(path.stat().st_size <= 100_000_000, f'oversized fixture member: {rel}')
            tar.add(path, arcname=rel, recursive=False)
            members.append({'path': rel, **expected_item})
    require({m['path']: {'bytes': m['bytes'], 'sha256': m['sha256']} for m in members} == expected,
            'archive member set does not equal authored fixture snapshot')
    require(members and sum(x['bytes'] for x in members) <= 50_000_000, 'fixture archive exceeds bound')
    with tarfile.open(dst, 'r:gz') as tar:
        infos = tar.getmembers()
        require(all(i.isfile() and not i.issym() and not i.islnk() for i in infos), 'unsafe tar member type')
        require(sorted(i.name for i in infos) == sorted(x['path'] for x in members), 'archive member list mismatch')
        for info in infos:
            data = tar.extractfile(info).read()
            item = next(x for x in members if x['path'] == info.name)
            require(len(data) == item['bytes'] and hashlib.sha256(data).hexdigest() == item['sha256'],
                    f'fixture archive member hash mismatch: {info.name}')
    require(authored_snapshot(root) == expected, 'fixture changed during archive creation')
    return {'members': members, 'archive_sha256': sha(dst), 'archive_bytes': dst.stat().st_size}

def verify_measurements() -> tuple[list[dict], dict]:
    rows = read('measurement-receipt.json')
    terminal = read('measurement-terminal.json')
    require(terminal.get('status') == 'complete' and terminal.get('trials') == 18 and terminal.get('commands') == 36,
            'measurement not terminal with required counts')
    names = []
    for rotation, order in enumerate(ORDERS, 1):
        for kind in order:
            stem = f'rotation-{rotation}-{kind}'
            names += [stem + '-make', stem]
    require([r.get('name') for r in rows] == names and all(r.get('exit') == 0 for r in rows),
            'measurement command order/exit mismatch')
    control = read('baseline.json')['control']
    c = read('baseline-provenance.json')
    app = read('binaries.json')['deadpan-app']
    bins = {'control': {'executed': control['path'], 'sha256': control['sha256']},
            'baseline': {'executed': c['matched_diagnostic_app'], 'sha256': c['matched_diagnostic_app_sha256']},
            'candidate': {'executed': app['executed'], 'sha256': app['sha256']}}
    require(sha(Path(control['path'])) == control['sha256'], 'qualified production control binary changed')
    require(sha(Path(c['matched_diagnostic_app'])) == c['matched_diagnostic_app_sha256'],
            'matched C diagnostic baseline binary changed')
    require(sha(Path(app['executed'])) == app['sha256'], 'final candidate executable changed')
    # Recheck all active pins immediately before importing the trace parser.
    pins = verify_active_pins()
    require(sha(S / 'check_profile.py') == pins['check_profile.py'], 'profile parser pin changed before import')
    import importlib.util
    spec = importlib.util.spec_from_file_location('v3_profile_parser', S / 'check_profile.py')
    parser = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(parser)
    producer_trials = read('trials.json')
    derived_trials, package_trials = [], []
    expected_profs = {'baseline': 6, 'candidate': 6}
    all_authored = {}
    for i in range(0, len(rows), 2):
        make, run = rows[i:i+2]
        stem = make['name'][:-5]
        require(run['name'] == stem, 'make/run pairing mismatch')
        rotation = int(stem.split('-')[1])
        kind = stem.rsplit('-', 1)[1]
        fixture = S / f'{stem}.deadpan'
        before = read(stem + '-input-before.json')
        after = read(stem + '-input-after.json')
        require(before.get('project.sqlite') and after.get('project.sqlite'), f'fixture DB snapshots missing: {stem}')
        before_authored = {k:v for k,v in before.items() if k not in SIDECARS}
        after_authored = {k:v for k,v in after.items() if k not in SIDECARS}
        require(before_authored == after_authored, f'fixture authored paths changed: {stem}')
        require(before.get('project.sqlite-wal', {}).get('bytes', 0) == 0
                and after.get('project.sqlite-wal', {}).get('bytes', 0) == 0,
                f'root WAL nonempty: {stem}')
        live_authored = authored_snapshot(fixture)
        require(live_authored == before_authored, f'current fixture differs from both captured maps: {stem}')
        report_path = S / f'{stem}-report/report.json'
        report = json.loads(report_path.read_text())
        metrics, warnings = parse_metrics(report, bins[kind], read('baseline-inputs.json'), before)
        stderr = (S / f'{stem}.stderr').read_text()
        profile = None
        if kind in ('baseline', 'candidate'):
            profile = parser.parse_profile(stderr)
            require(profile['block_count'] == 1, f'profile block count mismatch: {stem}')
            selected = profile['selected']
            require([r['name'] for r in selected['phases']] == list(parser.PHASES), f'profile phase names/order mismatch: {stem}')
            require(all(r['count'] == parser.EXPECTED_COUNTS[r['name']] for r in selected['phases']),
                    f'profile phase count mismatch: {stem}')
            require(sum(r['elapsed_ns'] for r in selected['phases']) == selected['elapsed_ns'],
                    f'profile conservation mismatch: {stem}')
            zero_rows = [r for r in selected['phases'] if r['name'] == 'store_checkpoint_wire']
            require(len(zero_rows) == 1 and zero_rows[0]['count'] == 0 and zero_rows[0]['elapsed_ns'] == 0,
                    f'zero-count checkpoint phase mismatch: {stem}')
            require(read(stem + '-profile.json') == profile, f'parsed profile differs from raw stderr: {stem}')
            expected_profs[kind] -= 1
        else:
            require('DEADPAN_PAUSE_ROUTE_PROFILE_V3' not in stderr, f'control emitted profile trace: {stem}')
        check_count = len(report['scenarios'][1]['checks'])
        kestrel_count = len(report['scenarios'][0]['checks'])
        authored_unchanged = before_authored == after_authored == live_authored
        wal_empty = after.get('project.sqlite-wal', {}).get('bytes', 0) == 0
        derived = {'rotation': rotation, 'kind': kind, 'binary_sha256': bins[kind]['sha256'],
                   'metrics': metrics, 'profile': profile, 'checks': check_count,
                   'kestrel_checks': kestrel_count, 'warnings': warnings,
                   'authored_input_unchanged': authored_unchanged, 'wal_empty': wal_empty}
        derived_trials.append(derived)
        package_trials.append({**derived, 'report_sha256': sha(report_path), 'fixture': str(fixture),
                               'input_before': before_authored, 'input_after': after_authored})
        all_authored[stem] = before_authored
    require(expected_profs == {'baseline': 0, 'candidate': 0}, f'profile lane counts mismatch: {expected_profs}')
    require(producer_trials == derived_trials, 'producer trials.json differs from raw reports, stderr and input snapshots')
    require(sum(r['checks'] for r in derived_trials) == 126
            and sum(r['kestrel_checks'] for r in derived_trials) == 18,
            'raw reports do not sum to 126 workflow and 18 Kestrel checks')
    summary = read('summary.json')
    require(summary.get('n_per_kind') == 6 and summary.get('diagnostic_only') is True,
            'measurement summary does not state six diagnostic runs per lane')
    for kind in ('control', 'baseline', 'candidate'):
        selected = [r for r in package_trials if r['kind'] == kind]
        require(len(selected) == 6, f'{kind} trial count mismatch')
        for metric in METRICS:
            values = [r['metrics'][metric] for r in selected]
            expected = {'min': min(values), 'median': statistics.median(values), 'max': max(values)}
            require(summary['kinds'][kind][metric] == expected, f'{kind} summary mismatch for {metric}')
    require(summary.get('no_percentile_target_or_memory_claim') is True,
            'measurement summary omitted scope limits')
    for kind in ('baseline', 'candidate'):
        lane = [t['profile']['selected'] for t in package_trials if t['kind'] == kind]
        recorded = summary['profile_lanes'][kind]
        require(recorded['worker_outer_ns'] == [b['elapsed_ns'] for b in lane],
                f'{kind} outer profile durations mismatch')
        for index, phase in enumerate(parser.PHASES):
            vals = [b['phases'][index]['elapsed_ns'] for b in lane]
            expected = {'min_ns': min(vals), 'median_ns': statistics.median(vals), 'max_ns': max(vals)}
            require(recorded['exclusive_phases'][phase] == expected,
                    f'{kind} phase summary mismatch: {phase}')
    require(sum(t['checks'] for t in package_trials) == 126 and sum(t['kestrel_checks'] for t in package_trials) == 18,
            'reported check counts do not recompute')
    paired = read('paired-analysis.json')
    analysis_review = read('analysis-review.json')
    require(analysis_review.get('status') == 'no_findings' and not analysis_review.get('findings')
            and analysis_review.get('script_sha256') == sha(S / 'analyse_pairs.py'),
            'paired-analysis review is not clear/bound to its producer')
    for rel, digest in analysis_review.get('reviewed_inputs', {}).items():
        require(sha(S / rel) == digest, f'paired-analysis review input changed: {rel}')
    require(paired.get('diagnostic_only') is True and paired.get('n_pairs') == 6
            and paired.get('trials_sha256') == sha(S / 'trials.json'),
            'paired-analysis metadata/trials binding mismatch')
    expected_pairs = []
    metrics = {'input_to_commit_ms': 'large_first_pause_input_to_commit_ms',
               'input_to_offscreen_composition_ms': 'large_first_pause_input_to_offscreen_background_composition_ms'}
    for rotation in range(1, 7):
        group = {r['kind']: r for r in package_trials if r['rotation'] == rotation}
        require(set(group) == {'control', 'baseline', 'candidate'}, f'paired-analysis rotation incomplete: {rotation}')
        values = {}
        for kind in ('baseline', 'candidate'):
            row = group[kind]
            phases = {r['name']: r['elapsed_ns'] for r in row['profile']['selected']['phases']}
            values[kind] = {name: row['metrics'][metric] for name, metric in metrics.items()}
            values[kind]['worker_ms'] = row['profile']['selected']['elapsed_ns'] / 1e6
            values[kind]['legacy_insertion_including_input_ms'] = (phases['leaf_insert_known'] + phases['leaf_input_durations']) / 1e6
            values[kind]['pure_leaf_region_ms'] = sum(phases[name] for name in [
                'semantic_leaf_apply', 'leaf_insert_known', 'leaf_input_durations', 'leaf_net_construction']) / 1e6
        differences = {name: values['baseline'][name] - values['candidate'][name] for name in values['baseline']}
        reductions = {name: 100 * differences[name] / values['baseline'][name] for name in differences}
        require(all(type(v) in (int, float) and math.isfinite(v) for v in differences.values())
                and all(type(v) in (int, float) and math.isfinite(v) for v in reductions.values()),
                'invalid paired arithmetic value')
        expected_pairs.append({'rotation': rotation, 'values': values,
                               'baseline_minus_candidate_ms': differences,
                               'relative_reduction_percent': reductions})
    require(paired.get('pairs') == expected_pairs, 'paired-analysis pairs do not recompute from guarded trials')
    def stat_rows(key):
        names = expected_pairs[0][key]
        return {name: {'minimum': min(pair[key][name] for pair in expected_pairs),
                       'median': statistics.median(pair[key][name] for pair in expected_pairs),
                       'maximum': max(pair[key][name] for pair in expected_pairs)} for name in names}
    require(paired.get('paired_difference_ms') == stat_rows('baseline_minus_candidate_ms')
            and paired.get('paired_relative_reduction_percent') == stat_rows('relative_reduction_percent'),
            'paired-analysis aggregate summary does not recompute')
    return package_trials, summary



def freeze_approved_inputs(trials: list[dict], main_ref: dict) -> dict[str, tuple[int, str]]:
    """Freeze every copied or guard-only input before staging; later mutations abort publication."""
    APPROVED.clear()
    paths: set[Path] = set()
    top = [
        'BUILD-GO.json','baseline.json','baseline-inputs.json','baseline-provenance.json',
        'baseline-sources.json','frozen-sources.json','expected-build-stages.json','expected-tests.json',
        'driver-pins.json','build.py','measure.py','analyse_pairs.py','check_profile.py','prototype.patch',
        'source-review.json','harness-review.json','analysis-review.json','build-owner.json','build-receipt.json',
        'build-terminal.json','measurement-owner.json','measurement-receipt.json','measurement-terminal.json',
        'test-binaries.json','binaries.json','summary.json','trials.json','PACKAGING-GO.json',
        'paired-analysis.json','design-correction.md','packager-final-review.json'
    ]
    paths.update(S / name for name in top)
    paths.add(Path(__file__))
    # Freeze every actual active pin, including any producer-specific protocol
    # files not otherwise listed above. The analysis producer is separately
    # reviewed and GO-bound; it is not required to appear in the immutable 14 pins.
    for rel in read('driver-pins.json'):
        pp = PurePosixPath(rel)
        require(not pp.is_absolute() and '..' not in pp.parts, f'unsafe pinned path: {rel}')
        paths.add(S / rel)
    for rel in read('analysis-review.json').get('reviewed_inputs', {}):
        pp = PurePosixPath(rel)
        require(not pp.is_absolute() and '..' not in pp.parts, f'unsafe analysis-review path: {rel}')
        paths.add(S / rel)
    for key in ('source_map_path','provenance_path','binary_manifest_path'):
        rel = PurePosixPath(main_ref[key])
        require(not rel.is_absolute() and '..' not in rel.parts, f'unsafe qualified Main path: {key}')
        paths.add(S / rel)
    for stage_name in ['rustc','cargo','format','core-unit-build','core-split-proof','core-wide-reference',
                       'core-integration','store-preparation','release-app','strict']:
        paths.add(S / f'{stage_name}.stdout'); paths.add(S / f'{stage_name}.stderr')
    changed = 'crates/deadpan-core/src/insert_time.rs'
    paths.add(S / 'source' / changed)
    baseline_root = Path(read('baseline-provenance.json')['source'])
    paths.add(baseline_root / changed)
    for rel in read('baseline-inputs.json'):
        paths.add(S / 'source' / rel); paths.add(baseline_root / rel)
    # Freeze both full Rust roots whose maps were validated, including unchanged paths.
    paths.update((S / 'source').rglob('*.rs'))
    paths.update(baseline_root.rglob('*.rs'))
    v1 = Path('/tmp/deadpan-insert-split-proof-prototype-2026-10-23')
    v2 = Path('/tmp/deadpan-insert-split-proof-prototype-v2-2026-10-23')
    history = [
        (v1, ['build-receipt.json','build-terminal.json','core-unit-build.stdout','core-unit-build.stderr',
              'build.py','source-review.json','driver-review.json','driver-pins.json','build-final-review-v2.json',
              'measure-final-review-v3.json','expected-tests.json','frozen-sources.json']),
        (v2, ['previous-attempt.json','build-receipt.json','build-terminal.json','core-wide-reference.stdout',
              'core-wide-reference.stderr','wide-oracle-discovery.json','build.py','build-final-review.json',
              'measure-final-review.json','source-review.json','driver-pins.json','expected-tests.json',
              'frozen-sources.json','test-binaries.json'])]
    for root, names in history:
        paths.update(root / name for name in names)
        rust = root / 'source/crates/deadpan-core/src/insert_time.rs'
        if rust.is_file(): paths.add(rust)
    v2_review = v2 / 'measure-final-review.json'
    paths.add(v2_review)
    for trial in trials:
        stem = f"rotation-{trial['rotation']}-{trial['kind']}"
        for suffix in ['-make.stdout','-make.stderr','.stdout','.stderr','-preflight.json',
                       '-input-before.json','-input-after.json']:
            paths.add(S / (stem + suffix))
        if trial['kind'] in ('baseline','candidate'):
            paths.add(S / f'{stem}-profile.json')
        paths.add(S / f'{stem}-report/report.json')
        fixture = S / f'{stem}.deadpan'
        require(fixture.is_dir() and not fixture.is_symlink(), f'missing/unsafe authored fixture {stem}')
        paths.update(p for p in fixture.rglob('*') if p.is_file())
        require(not any(p.is_symlink() for p in fixture.rglob('*')), f'symlink in fixture {stem}')
    # Guard all final and comparison executables without retaining their bytes in the package.
    for row in read('binaries.json').values():
        paths.add(Path(row['source'])); paths.add(Path(row['executed']))
    baseline = read('baseline.json')['control']
    provenance = read('baseline-provenance.json')
    paths.add(Path(baseline['path'])); paths.add(Path(provenance['matched_diagnostic_app']))
    for stage_rows in read('test-binaries.json').values():
        items = stage_rows.values() if isinstance(stage_rows, dict) and 'sha256' not in stage_rows else [stage_rows]
        for row in items:
            for field in ('built','retained','executed'):
                if row.get(field): paths.add(Path(row[field]))
    for path in sorted(paths, key=lambda p: str(p)):
        require(path.is_file() and not path.is_symlink(), f'approved input missing/unsafe: {path}')
        resolved = str(path.resolve(strict=True))
        APPROVED[resolved] = (path.stat().st_size, sha(path))
    return dict(APPROVED)


def verify_live_path_sets(source_map: dict, trials: list[dict]) -> None:
    """Reject post-validation additions, removals, or edits in source and fixtures."""
    current_root = S / 'source'
    current = {str(p.relative_to(current_root)): sha(p)
               for p in sorted(current_root.rglob('*.rs'))}
    require(current == source_map, 'full candidate Rust path/hash set changed before publication')
    baseline_root = Path(read('baseline-provenance.json')['source'])
    baseline_map = read('baseline-sources.json')
    baseline_live = {str(p.relative_to(baseline_root)): sha(p)
                     for p in sorted(baseline_root.rglob('*.rs'))}
    require(baseline_live == baseline_map, 'full matched C baseline Rust path/hash set changed before publication')
    for trial in trials:
        root = Path(trial['fixture'])
        require(authored_snapshot(root) == trial['input_before'],
                f"full authored fixture path/hash set changed before publication: rotation-{trial['rotation']}-{trial['kind']}")


def verify_approved_inputs(approved: dict[str, tuple[int, str]]) -> None:
    require(approved == APPROVED, 'approved input registry changed')
    for raw, expected in approved.items():
        path = Path(raw)
        require(path.is_file() and not path.is_symlink()
                and (path.stat().st_size, sha(path)) == expected,
                f'approved input changed during packaging: {raw}')

def main() -> None:
    require(not DEST.exists(), f'destination already exists; refusing overwrite: {DEST}')
    require((S / 'PACKAGING-GO.json').is_file(), 'Root packaging GO is required')
    # Freeze the pre-validation bytes first. Full validators then run against this
    # identity set, and the same set is rechecked immediately before publication.
    preliminary_go = read('PACKAGING-GO.json')
    main_ref = preliminary_go.get('current_main_reference')
    require(isinstance(main_ref, dict), 'Root packaging GO must bind qualified Main reference')
    planned_trials = [{'rotation': rotation, 'kind': kind}
                      for rotation, order in enumerate(ORDERS, 1) for kind in order]
    approved = freeze_approved_inputs(planned_trials, main_ref)
    source_map, inputs = verify_source()
    build_rows = verify_build()
    harness_review, go, pins = verify_review_and_go()
    require(go['current_main_reference'] == main_ref, 'qualified Main reference changed after pre-validation freeze')
    verify_tests()
    trials, summary = verify_measurements()
    require([{k: t[k] for k in ('rotation','kind')} for t in trials] == planned_trials,
            'validated trial set differs from pre-validation freeze plan')
    verify_approved_inputs(approved)
    stage = Path(tempfile.mkdtemp(prefix='.insert-split-v4-', dir=DEST.parent))
    compressed = []
    try:
        # Copy only pinned/necessary top-level evidence; never copy source trees, targets, or executables.
        top = [
            'BUILD-GO.json', 'baseline.json', 'baseline-inputs.json', 'baseline-provenance.json',
            'baseline-sources.json', 'frozen-sources.json', 'expected-build-stages.json', 'expected-tests.json',
            'driver-pins.json', 'build.py', 'measure.py', 'analyse_pairs.py', 'check_profile.py',
            'prototype.patch', 'source-review.json', 'harness-review.json', 'analysis-review.json', 'build-owner.json',
            'build-receipt.json', 'build-terminal.json', 'measurement-owner.json', 'measurement-receipt.json',
            'measurement-terminal.json', 'test-binaries.json', 'binaries.json', 'summary.json', 'trials.json',
            'PACKAGING-GO.json', 'paired-analysis.json', 'design-correction.md', 'packager-final-review.json'
        ]
        for name in top:
            copy_if(S / name, stage, 'protocol/' + name, compressed)
        put(Path(__file__), stage, 'protocol/packaging-draft/package_v4.py', compressed)
        for key, relpath in [('source_map_path','source-map.json'), ('provenance_path','provenance.json'),
                             ('binary_manifest_path','binary-manifest.json')]:
            put(S / main_ref[key], stage, 'qualified-current-main/' + relpath, compressed)
        for stage_name in ['rustc','cargo','format','core-unit-build','core-split-proof','core-wide-reference',
                           'core-integration','store-preparation','release-app','strict']:
            for ext in ('stdout','stderr'):
                put(S / f'{stage_name}.{ext}', stage, f'build/logs/{stage_name}.{ext}', compressed)
        # Preserve the exact single changed Rust source as text, not an executable/source tree.
        changed = 'crates/deadpan-core/src/insert_time.rs'
        current_copy = S / 'source' / changed
        put(current_copy, stage, f'sources/after/{changed}.rs.txt', compressed)
        for rel, digest in inputs.items():
            source_input = S / 'source' / rel
            require(sha(source_input) == digest, f'source input changed during copy: {rel}')
            put(source_input, stage, f'inputs/{rel}', compressed)
        old = read('baseline-sources.json')
        # The baseline file is retrieved from the actual qualified repository root recorded in provenance.
        provenance = read('baseline-provenance.json')
        baseline_root = Path(provenance['source'])
        old_file = baseline_root / changed
        require(old_file.is_file() and sha(old_file) == old[changed], 'qualified baseline source snapshot unavailable/mismatched')
        put(old_file, stage, f'sources/before/{changed}.rs.txt', compressed)
        # Historical failed attempts remain explicitly separated from V3 success evidence.
        v1 = Path('/tmp/deadpan-insert-split-proof-prototype-2026-10-23')
        v2 = Path('/tmp/deadpan-insert-split-proof-prototype-v2-2026-10-23')
        for root, prefix, names in [
            (v1, 'history/v1-e0659', ['build-receipt.json','build-terminal.json','core-unit-build.stdout',
             'core-unit-build.stderr','build.py','source-review.json','driver-review.json','driver-pins.json',
             'build-final-review-v2.json','measure-final-review-v3.json',
             'expected-tests.json','frozen-sources.json']),
            (v2, 'history/v2-zero-manifest', ['previous-attempt.json','build-receipt.json',
             'build-terminal.json','core-wide-reference.stdout','core-wide-reference.stderr','wide-oracle-discovery.json',
             'build.py','build-final-review.json','measure-final-review.json','source-review.json','driver-pins.json',
             'expected-tests.json','frozen-sources.json','test-binaries.json'])]:
            for name in names:
                require((root / name).is_file(), f'missing historical provenance file: {root / name}')
                put(root / name, stage, f'{prefix}/{name}', compressed)
        # Explicitly document the worker-created V2 review hash versus the earlier frozen pin.
        v2_review = v2 / 'measure-final-review.json'
        review_pin = {'file': 'v2/measure-final-review.json', 'current_sha256': sha(v2_review),
                      'prior_frozen_reference_sha256': '092e4058e28945f5c0679431ee59fa278c32f762ae112eac182a5ea6bf67762a',
                      'note': 'The current file was written during an earlier agent response after the V2 root had frozen its prior review pin. This package records the current bytes and the mismatch; it does not rewrite or treat the current file as the frozen prior review.'}
        (stage / 'history/v2-review-pin-discrepancy.json').write_text(json.dumps(review_pin, indent=2)+'\n')
        require(sha(v2_review) != review_pin['prior_frozen_reference_sha256'], 'expected historical review pin discrepancy is absent')
        # Preserve exact historical test source snapshots without retaining test executables.
        for ver, root in [('v1', v1), ('v2', v2)]:
            baseline_file = root / 'source/crates/deadpan-core/src/insert_time.rs'
            if baseline_file.is_file():
                put(baseline_file, stage, f'history/{ver}/insert_time.rs.txt', compressed)
        # Per-trial receipts/logs, before/after maps, reports and parsed profiles.
        for trial in trials:
            stem, kind = f"rotation-{trial['rotation']}-{trial['kind']}", trial['kind']
            for suffix in ['-make.stdout','-make.stderr','.stdout','.stderr','-preflight.json',
                           '-input-before.json','-input-after.json','-profile.json']:
                copy_if(S / (stem + suffix), stage, f'trials/{stem}/{stem}{suffix}', compressed)
            report = S / f'{stem}-report/report.json'
            put(report, stage, f'trials/{stem}/report.json', compressed, force_gzip=True)
            fixture = S / f'{stem}.deadpan'
            archive = stage / f'fixtures/{stem}.tar.gz'
            archive.parent.mkdir(parents=True, exist_ok=True)
            fixture_info = safe_fixture_archive(fixture, archive, trial['input_before'])
            require(sha(fixture / 'project.sqlite') == read(stem + '-input-before.json')['project.sqlite']['sha256'],
                    f'fixture archive source identity mismatch: {stem}')
            # Record a manifest beside each archive after validating all included bytes.
            (stage / f'fixtures/{stem}.manifest.json').write_text(json.dumps(fixture_info, indent=2)+'\n')
        # Honest historical failure account and static review notes.
        require(read('previous-attempts.json').get('v3_source_identical_to_v2') is True,
                'V3/V2 source relationship record mismatch')
        previous = read('previous-attempts.json')
        (stage / 'history/previous-attempts.json').write_text(json.dumps(previous, indent=2)+'\n')
        (stage / 'README.md').write_text(
            '# Insert-split proof V3 prototype evidence\n\n'
            'Private diagnostic prototype evidence. This package records six rotated runs per lane. '
            'It does not establish a performance target, percentile, or production qualification. '
            'No claim that the candidate is faster is required or made. V1 and V2 failed attempts are retained separately. '
            'The inventory lists every other retained file, including its own README excluded from the inventory itself.\n')
        # Final inventory excludes only itself. Hash every retained file and reread every digest.
        entries = []
        for path in sorted(stage.rglob('*')):
            if path.is_file() and path.name != 'inventory.json':
                entries.append({'path': path.relative_to(stage).as_posix(), 'bytes': path.stat().st_size, 'sha256': sha(path)})
        for item in entries:
            path = stage / item['path']
            require(path.stat().st_size == item['bytes'] and sha(path) == item['sha256'],
                    f'package reread mismatch: {item["path"]}')
        inv = {'format': 'deadpan-evidence-inventory-v1', 'files': entries,
               'total_files': len(entries), 'total_bytes': sum(x['bytes'] for x in entries),
               'compressed_payloads': compressed}
        inv_path = stage / 'inventory.json'
        inv_path.write_text(json.dumps(inv, indent=2, sort_keys=True)+'\n')
        reread = json.loads(inv_path.read_text())
        require(reread == inv, 'inventory JSON reread mismatch')
        for item in entries:
            require(sha(stage / item['path']) == item['sha256'], f'final payload reread mismatch: {item["path"]}')
        inv_hash = sha(inv_path)
        verify_live_path_sets(source_map, trials)
        verify_approved_inputs(approved)
        if DEST.exists():
            raise RuntimeError('destination appeared during package creation; refusing overwrite')
        os.rename(stage, DEST)
        print(json.dumps({'status':'created','destination':str(DEST),'total_files':inv['total_files'],
                          'total_bytes':inv['total_bytes'],'inventory_sha256':inv_hash,
                          'build_stages':len(build_rows),'trials':len(trials),
                          'n_per_lane':6,'diagnostic_only':True},sort_keys=True))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
