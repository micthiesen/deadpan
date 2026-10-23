#!/usr/bin/env python3
"""Fail-closed draft packager for the current-pause route diagnostic.

Root must review this file and create PACKAGING-GO.json before execution.
This script has not been run and does not imply that qualification or timing
trials have completed.
"""
from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import os
import shutil
import tempfile
from pathlib import Path

S = Path('/tmp/deadpan-current-pause-route-profile-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-current-pause-route-profile'
RAW_LIMIT = 100_000
TRIALS = [f'rotation-{rotation}-{kind}'
          for rotation in range(1, 4)
          for kind in ('control', 'off', 'on')]


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def load(name: str):
    return json.loads((S / name).read_text(encoding='utf-8'))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def copy_payload(src: Path, stage: Path, rel: str) -> None:
    require(src.is_file(), f'missing required payload: {src}')
    target = stage / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    if src.stat().st_size > RAW_LIMIT and src.suffix.lower() in {'.json', '.txt', '.log', '.stderr', '.stdout'}:
        target = target.with_name(target.name + '.gz')
        with src.open('rb') as inp, target.open('wb') as raw:
            with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as out:
                shutil.copyfileobj(inp, out)
    else:
        shutil.copy2(src, target)


def verify_go_and_gates() -> dict:
    go = load('PACKAGING-GO.json')
    require(go.get('authorized') is True, 'Root PACKAGING-GO.json must authorize packaging')
    build = load('build-terminal.json')
    measure = load('measurement-terminal.json')
    require(build.get('status') == 'complete' and build.get('steps') == 7,
            'release build must be terminal with all seven steps successful')
    require(measure.get('status') == 'complete' and measure.get('trials') == 9
            and measure.get('commands') == 18,
            'measurement must be terminal with nine trials and eighteen commands')

    build_rows = load('build-receipt.json')
    command_rows = load('measurement-receipt.json')
    require([row.get('name') for row in build_rows] ==
            ['rustc', 'cargo', 'format', 'helper-unit', 'store-preparation', 'release-app', 'strict'],
            'build stage sequence mismatch')
    require(len(build_rows) == 7 and all(row.get('exit') == 0 for row in build_rows),
            'build receipt rows incomplete or failed')
    require(len(command_rows) == 18 and all(row.get('exit') == 0 for row in command_rows),
            'measurement command rows incomplete or failed')
    expected = load('expected-tests.json')
    require(len(expected.get('helper-unit', [])) == 8 and len(expected.get('store-preparation', [])) == 12,
            'test selection identity/count mismatch')
    for test_name, stdout_name in (('helper-unit', 'helper-unit.stdout'),
                                   ('store-preparation', 'store-preparation.stdout')):
        out = (S / stdout_name).read_text()
        import re
        actual = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', out, re.M)
        require(sorted(actual) == sorted(expected[test_name]), f'{test_name} executed tests mismatch')
    expected_order = [('control', 'off', 'on'), ('off', 'on', 'control'), ('on', 'control', 'off')]
    names = []
    for rotation, order in enumerate(expected_order, 1):
        for kind in order:
            stem = f'rotation-{rotation}-{kind}'
            names.extend([stem + '-make', stem])
    require([row.get('name') for row in command_rows] == names,
            'measurement command order differs from the reviewed rotated design')

    source_map = load('frozen-sources.json')
    require(len(source_map) == 2054, 'unexpected frozen source map size')
    base_sources = load('baseline-sources.json')
    audit = load('overlay-audit.json')
    require(audit.get('baseline_rust') == 2053 and audit.get('private_rust') == 2054
            and audit.get('helper_tests') == 8 and audit.get('store_tests') == 12,
            'overlay audit counts mismatch')
    changed = set(audit.get('changed', []))
    added = set(audit.get('new', []))
    require(len(changed) == 4 and added == {'crates/deadpan-core/src/pause_route_profile.rs'},
            'unexpected private overlay path set')
    require(set(base_sources) - changed == set(source_map) - added,
            'baseline/final source path sets differ outside the reviewed overlay')
    require(all(base_sources[path] == source_map[path] for path in set(base_sources) - changed),
            'non-overlay Rust source changed')
    live = {str(p.relative_to(S / 'source')): digest(p)
            for p in sorted((S / 'source').rglob('*.rs'))}
    require(live == source_map, 'private source tree differs from frozen source map')
    inputs = load('baseline-inputs.json')
    require(len(inputs) == 6, 'expected six pinned inputs')
    require(all(digest(S / 'source' / path) == value for path, value in inputs.items()),
            'pinned input mismatch')
    review = load('source-review.json')
    require(review.get('status') == 'no_findings' and not review.get('findings'),
            'final source review missing or has findings')
    require(review.get('source_map_sha256') == digest(S / 'frozen-sources.json'),
            'source review does not pin final source map')

    base = load('baseline.json')
    control = base['control']
    require(digest(Path(control['path'])) == control['sha256'], 'control executable hash mismatch')
    binaries = load('binaries.json')
    app = binaries['deadpan-app']
    require(app['sha256'] == build.get('app_sha256'), 'candidate app differs from terminal build')
    require(digest(Path(app['executed'])) == app['sha256'], 'candidate executable hash mismatch')
    for name, item in binaries.items():
        require(digest(Path(item['executed'])) == item['sha256'], f'{name} executable hash mismatch')

    trials = load('trials.json')
    require(len(trials) == 9, 'expected nine trial summaries')
    pairs = {(int(row['rotation']), row['kind']) for row in trials}
    require(pairs == {(r, k) for r in (1, 2, 3) for k in ('control', 'off', 'on')},
            'trial lane/rotation matrix is incomplete')
    for row in trials:
        kind = row['kind']
        expected_binary = control['sha256'] if kind == 'control' else app['sha256']
        require(row.get('binary_sha256') == expected_binary, 'trial app identity mismatch')
        require(row.get('checks') == 7 and row.get('kestrel_checks') == 1,
                'pause/Kestrel check counts mismatch')
        require(row.get('authored_input_unchanged') is True and row.get('wal_empty') is True,
                'trial authored-input or WAL guard failed')
        before = load(f"{TRIALS[(int(row['rotation']) - 1) * 3 + ('control','off','on').index(kind)]}-input-before.json")
        after = load(f"{TRIALS[(int(row['rotation']) - 1) * 3 + ('control','off','on').index(kind)]}-input-after.json")
        require({k: v for k, v in before.items() if k not in {'project.sqlite-shm', 'project.sqlite-wal'}} ==
                {k: v for k, v in after.items() if k not in {'project.sqlite-shm', 'project.sqlite-wal'}},
                'trial authored input maps differ')
        require(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0,
                'trial WAL is not empty')
        report = load(f"{TRIALS[(int(row['rotation']) - 1) * 3 + ('control','off','on').index(kind)]}-report/report.json")
        require(report.get('failed') is False, 'native report failed')
        meta = report.get('metadata', {})
        require(meta.get('binary_sha256') == expected_binary and meta.get('build_profile') == 'release',
                'native report binary/build identity mismatch')
        require(meta.get('fixture_sha256') == before['project.sqlite']['sha256'],
                'native report fixture identity mismatch')
        require(meta.get('cargo_lock_sha256_at_run') == inputs['Cargo.lock'],
                'native report Cargo.lock identity mismatch')
        require(meta.get('physical_display_measured') is False,
                'unexpected physical-display claim in report metadata')
        scenarios = report.get('scenarios', [])
        require([item.get('name') for item in scenarios] == ['kestrel-shortcuts', 'large-first-pause'],
                'native report scenario order mismatch')
        require(len(scenarios[0].get('checks', [])) == 1
                and len(scenarios[1].get('checks', [])) == 7
                and all(check.get('passed') is True
                        for scenario in scenarios for check in scenario.get('checks', [])),
                'native report does not pass seven pause and one Kestrel checks')
        if kind == 'on':
            parsed = load(f"{TRIALS[(int(row['rotation']) - 1) * 3 + 2]}-profile.json")
            selected = parsed['selected']
            require(parsed.get('block_count') == 1
                    and selected['phase_sum_ns'] == selected['elapsed_ns']
                    and len(selected['phases']) == 13,
                    'profile conservation/schema invalid')
            stderr = (S / f"{TRIALS[(int(row['rotation']) - 1) * 3 + 2]}.stderr").read_text()
            parser_spec = importlib.util.spec_from_file_location('pause_route_parser', S / 'check_profile.py')
            parser_module = importlib.util.module_from_spec(parser_spec)
            parser_spec.loader.exec_module(parser_module)
            reparsed = parser_module.parse_profile(stderr)
            require(reparsed == parsed, 'raw stderr does not reproduce saved profile parse')
        else:
            stderr = (S / f"{TRIALS[(int(row['rotation']) - 1) * 3 + ('control','off','on').index(kind)]}.stderr").read_text()
            require('DEADPAN_PAUSE_ROUTE_PROFILE' not in stderr,
                    'profile output found in control or gate-off lane')

    summary = load('summary.json')
    require(summary.get('n_per_kind') == 3 and summary.get('diagnostic_only') is True,
            'summary metadata mismatch')
    import statistics
    for kind in ('control', 'off', 'on'):
        lane = [row for row in trials if row['kind'] == kind]
        for metric in lane[0]['metrics']:
            values = [float(row['metrics'][metric]) for row in lane]
            expected = {'min': min(values), 'median': statistics.median(values), 'max': max(values)}
            require(summary['kinds'][kind][metric] == expected,
                    f'summary mismatch for {kind}/{metric}')
    on_rows = [row['profile']['selected'] for row in trials if row['kind'] == 'on']
    require(len(on_rows) == 3, 'expected three profiled trials')
    for index, phase in enumerate(('native_admission', 'semantic_plan', 'native_continuation',
                                   'store_entry_misc', 'store_prepare', 'host_runtime_bank',
                                   'store_write', 'sqlite_commit', 'store_cache_publication',
                                   'native_postcommit', 'workspace_refresh', 'native_finish',
                                   'native_publication')):
        vals = [row['phases'][index]['elapsed_ns'] for row in on_rows]
        expected = {'min_ns': min(vals), 'median_ns': statistics.median(vals), 'max_ns': max(vals)}
        require(summary['exclusive_phases'][phase] == expected,
                f'phase summary mismatch for {phase}')
    require(summary['worker_outer_ns'] == [row['elapsed_ns'] for row in on_rows],
            'outer worker durations differ from profiled trial data')
    return {'baseline': base, 'source_map': source_map, 'inputs': inputs,
            'review': review, 'binaries': binaries, 'trials': trials, 'summary': summary}


def gather(stage: Path) -> None:
    # Static setup, parser, plan, review, source maps, input maps, and receipts.
    top_files = [
        'PLAN-STATUS.md', 'plan.md', 'setup-history.json', 'baseline.json',
        'baseline-inputs.json', 'baseline-sources.json', 'frozen-sources.json',
        'overlay-audit.json', 'overlay.patch', 'expected-tests.json',
        'source-review.json', 'driver-pins.json', 'parser-notes.json',
        'check_profile.py', 'test_profile_parser.py', 'parser-test-receipt.json',
        'build.py', 'build-receipt.json', 'build-terminal.json', 'build-owner.json',
        'helper-unit.stdout', 'helper-unit.stderr', 'store-preparation.stdout',
        'store-preparation.stderr', 'format.stdout', 'format.stderr',
        'rustc.stdout', 'rustc.stderr', 'cargo.stdout', 'cargo.stderr',
        'strict.stdout', 'strict.stderr', 'cargo-rustc-info.json', 'test-binaries.json',
        'binaries.json', 'measure.py', 'measurement-owner.json',
        'measurement-receipt.json', 'measurement-terminal.json', 'trials.json',
        'summary.json', 'driver-review.json', 'review-followup.json',
        'host-observations-public.json', 'progress-artifact-checkpoint.json',
        'report-outline.md', 'retention-plan.json', 'PACKAGING-GO.json'
    ]
    for name in top_files:
        copy_payload(S / name, stage, f'run/{name}')
    for name in ('review-followup.json', 'setup-history.json'):
        if (S / name).exists() and name not in top_files:
            copy_payload(S / name, stage, f'run/{name}')

    # Preserve source snapshots only as text, never as Rust compile inputs.
    for root in ('source-retention/before', 'source-retention/after', 'implementer-notes'):
        base = S / root
        for src in sorted(base.rglob('*')):
            if not src.is_file():
                continue
            if src.suffix == '.txt' or src.name in {'HANDOFF.md'} or src.suffix in {'.json', '.patch'}:
                copy_payload(src, stage, f'{root}/{src.relative_to(base)}')
    # Trial reports, raw output, guarded input snapshots, preflight observations,
    # and parsed diagnostic blocks. The report JSON is compressed deterministically.
    for stem in TRIALS:
        for suffix in ('.stdout', '.stderr', '-input-before.json', '-input-after.json',
                       '-preflight.json', '-profile.json'):
            src = S / (stem + suffix)
            if src.exists():
                copy_payload(src, stage, f'trials/{src.name}')
        report = S / f'{stem}-report/report.json'
        copy_payload(report, stage, f'trials/{stem}-report/report.json')

    # Preserve correction and review histories as bounded textual evidence.
    history = S / 'history'
    if history.exists():
        for src in sorted(history.rglob('*')):
            if src.is_file() and src.suffix.lower() in {'.json', '.md', '.py', '.txt', '.patch', '.stdout', '.stderr'}:
                copy_payload(src, stage, f'history/{src.relative_to(history)}')

    # Bounded actual fixture evidence only; do not package raw project trees.
    for stem in TRIALS:
        fixture = S / f'{stem}.deadpan'
        if fixture.exists():
            require(fixture.is_dir(), f'fixture is not a directory: {fixture}')
            archive = stage / 'fixtures' / (fixture.name + '.tar.gz')
            archive.parent.mkdir(parents=True, exist_ok=True)
            # Refuse oversized fixtures rather than silently creating a large package.
            total = sum(p.stat().st_size for p in fixture.rglob('*') if p.is_file())
            require(total <= 50 * 1024 * 1024, f'fixture exceeds bounded retention threshold: {total}')
            import tarfile
            with tarfile.open(archive, 'w:gz') as tar:
                tar.add(fixture, arcname=fixture.name, recursive=True)


def main() -> None:
    require(not DEST.exists(), f'destination already exists: {DEST}')
    verified = verify_go_and_gates()
    parent = DEST.parent
    parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.current-pause-route-stage-', dir=parent))
    try:
        gather(stage)
        payload = []
        for path in sorted(p for p in stage.rglob('*') if p.is_file() and p.name != 'inventory.json'):
            payload.append({'path': str(path.relative_to(stage)), 'bytes': path.stat().st_size,
                            'sha256': digest(path)})
        inventory = {'format': 'deadpan-evidence-inventory-v1', 'total_files': len(payload),
                     'total_bytes': sum(item['bytes'] for item in payload), 'files': payload}
        inv_path = stage / 'inventory.json'
        inv_path.write_text(json.dumps(inventory, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        inv_sha = digest(inv_path)
        reread = json.loads(inv_path.read_text(encoding='utf-8'))
        require(reread == inventory, 'inventory reread mismatch')
        for item in payload:
            path = stage / item['path']
            require(path.stat().st_size == item['bytes'] and digest(path) == item['sha256'],
                    f'payload integrity failure: {item["path"]}')
        (stage / 'PACKAGE-SUMMARY.md').write_text(
            '# Current pause route diagnostic package\n\n'
            'Inventory values are generated from retained files after all checks.\n\n'
            f'- Payload files (excluding this summary and inventory): {len(payload)}\n'
            f'- Payload bytes: {sum(item["bytes"] for item in payload)}\n'
            f'- Inventory SHA-256: `{inv_sha}`\n'
            '- Diagnostic only; this package does not close a requirement or performance target.\n',
            encoding='utf-8')
        require(not DEST.exists(), 'destination appeared during staging')
        stage.rename(DEST)
        print(json.dumps({'destination': str(DEST), 'files': len(payload) + 2,
                          'bytes': sum(item['bytes'] for item in payload),
                          'inventory_sha256': inv_sha,
                          'trials': len(verified['trials'])}, sort_keys=True))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
