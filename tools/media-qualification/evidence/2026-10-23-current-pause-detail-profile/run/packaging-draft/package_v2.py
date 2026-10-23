#!/usr/bin/env python3
"""Reviewed-shape, fail-closed package draft. Do not run without Root's bound GO."""
from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import math
import os
import shutil
import statistics
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

S = Path('/tmp/deadpan-current-pause-detail-profile-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-current-pause-detail-profile'
LIMIT = 100_000
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}

METRICS = {'large_first_pause_input_to_commit_ms',
           'large_first_pause_input_to_offscreen_background_composition_ms'}
ROTATIONS = (('control', 'off', 'on'), ('off', 'on', 'control'), ('on', 'control', 'off'))
TRIALS = [f'rotation-{r}-{kind}' for r, order in enumerate(ROTATIONS, 1) for kind in order]


def sha(p: Path) -> str:
    h = hashlib.sha256()
    with p.open('rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def read(name: str):
    return json.loads((S / name).read_text(encoding='utf-8'))


def require(ok: bool, why: str) -> None:
    if not ok:
        raise RuntimeError(why)


def finite_number(x) -> bool:
    return type(x) in (int, float) and math.isfinite(x) and x >= 0


def put(src: Path, stage: Path, rel: str, compressed: list[dict], *, force_gzip: bool = False) -> None:
    require(src.is_file(), f'missing required payload: {src}')
    dst = stage / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    if (force_gzip or src.stat().st_size > LIMIT) and src.suffix.lower() in {'.json', '.txt', '.md', '.log', '.stdout', '.stderr', '.patch', '.py'}:
        dst = dst.with_name(dst.name + '.gz')
        with src.open('rb') as inp, dst.open('wb') as raw:
            with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as out:
                shutil.copyfileobj(inp, out)
        with gzip.open(dst, 'rb') as inp:
            h = hashlib.sha256(); n = 0
            for block in iter(lambda: inp.read(1 << 20), b''):
                h.update(block); n += len(block)
        require(n == src.stat().st_size and h.hexdigest() == sha(src), f'gzip roundtrip failed: {rel}')
        compressed.append({'source': str(src), 'stored': str(dst.relative_to(stage)),
                           'decoded_bytes': n, 'decoded_sha256': h.hexdigest(),
                           'stored_sha256': sha(dst)})
    else:
        shutil.copy2(src, dst)


def verify_source() -> tuple[dict, dict, dict, dict]:
    base = read('coarse-frozen-sources.json')
    final = read('frozen-sources.json')
    audit = read('overlay-audit.json')
    changed = set(audit['changed'])
    added = set(audit.get('new', []))
    expected_changed = {
        'crates/deadpan-app/src/project/service.rs',
        'crates/deadpan-core/src/compound.rs',
        'crates/deadpan-core/src/one_leaf_replay.rs',
        'crates/deadpan-core/src/pause_route_profile.rs',
        'crates/deadpan-core/src/semantic/planner.rs',
        'crates/deadpan-core/src/semantic/planner/content.rs',
        'crates/deadpan-core/src/semantic/planner/pause.rs',
        'crates/deadpan-store/src/compound.rs',
        'crates/deadpan-store/src/lib.rs',
    }
    require(len(base) == 2054 and len(final) == 2054, 'coarse/fine source-map count mismatch')
    require(changed == expected_changed and not added, 'fine overlay path set mismatch')
    require(set(base) == set(final), 'coarse/fine source path set differs')
    differing = {p for p in base if base[p] != final[p]}
    require(differing == changed, 'fine source bytes differ outside the nine reviewed paths')
    require(audit.get('source_map_sha256') == sha(S / 'frozen-sources.json')
            and audit.get('base_coarse_map_sha256') == sha(S / 'coarse-frozen-sources.json'),
            'overlay audit map binding mismatch')
    require(audit.get('nonowned_unchanged') == 2045 and audit.get('inputs_unchanged') == 6,
            'fine overlay audit scope counts mismatch')
    live = {str(p.relative_to(S / 'source')): sha(p) for p in sorted((S / 'source').rglob('*.rs'))}
    require(live == final, 'frozen private source does not match final map')
    inputs = read('baseline-inputs.json')
    require(len(inputs) == 6 and inputs == read('coarse-baseline-inputs.json')
            and all(sha(S / 'source' / p) == h for p, h in inputs.items()),
            'six pinned inputs mismatch')
    sr = read('source-review.json')
    require(sr.get('status') == 'no_findings' and not sr.get('findings'), 'final source review not clear')
    parser_review = read('parser-review.json')
    require(parser_review.get('status') == 'no_findings' and not parser_review.get('findings')
            and parser_review.get('source_map_sha256') == sha(S / 'frozen-sources.json'),
            'final V2 parser review not clear or not source-bound')
    for rel, digest in parser_review.get('reviewed_files', {}).items():
        require(sha(S / rel) == digest, f'parser review file changed: {rel}')
    require(sr.get('source_map_sha256') == sha(S / 'frozen-sources.json'), 'source review map mismatch')
    require(audit.get('patch_sha256') == sha(S / 'overlay.patch'), 'overlay patch hash mismatch')
    for rel, expected_hash in sr.get('reviewed_paths', {}).items():
        require(rel in final and final[rel] == expected_hash, f'source review path identity mismatch: {rel}')
    before_root, after_root = S / 'source-retention/before', S / 'source-retention/after'
    require(before_root.is_dir() and after_root.is_dir(), 'source retention snapshots missing')
    before_files = {p.relative_to(before_root).as_posix()[:-4] for p in before_root.rglob('*.rs.txt')}
    after_files = {p.relative_to(after_root).as_posix()[:-4] for p in after_root.rglob('*.rs.txt')}
    require(before_files == changed | added and after_files == changed | added,
            'source-retention snapshot path set mismatch')
    for rel in changed | added:
        after_copy = after_root / (rel + '.txt')
        require(after_copy.is_file() and sha(after_copy) == final[rel],
                f'final source text snapshot mismatch: {rel}')
        if rel in base:
            before_copy = before_root / (rel + '.txt')
            require(before_copy.is_file() and sha(before_copy) == base[rel],
                    f'baseline source text snapshot mismatch: {rel}')
        else:
            before_copy = before_root / (rel + '.txt')
            require(before_copy.is_file() and before_copy.stat().st_size == 0
                    and sha(before_copy) == hashlib.sha256(b'').hexdigest(),
                    f'new source before snapshot must be the freeze-produced empty file: {rel}')
    dr = read('driver-review.json')
    require(dr.get('status') == 'no_findings' and not dr.get('findings'), 'final driver review not clear')
    pins = read('driver-pins.json')
    require(bool(pins), 'driver pins are empty')
    reviewed_files = dr.get('reviewed_files', {})
    for rel, expected_hash in reviewed_files.items():
        require(rel in pins and pins[rel] == expected_hash, f'driver review/pin identity mismatch: {rel}')
    require(dr.get('source_map_sha256') == sha(S / 'frozen-sources.json'),
            'driver review source map mismatch')
    for rel, expected in pins.items():
        p = S / rel
        require(p.is_file() and sha(p) == expected, f'driver pin mismatch: {rel}')
    return final, inputs, sr, dr



def verify_tests() -> None:
    expected = read('expected-tests.json')
    require(len(expected.get('helper-unit', [])) == 8 and len(expected.get('store-preparation', [])) == 12,
            'expected test selection/count mismatch')
    for key, outname, binname, count in (
        ('helper-unit', 'helper-unit.stdout', 'helper-unit', 8),
        ('store-preparation', 'store-preparation.stdout', 'store-preparation', 12)):
        text = (S / outname).read_text()
        import re
        actual = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M)
        summaries = re.findall(r'^test result: ok\. (\d+) passed; 0 failed; 0 ignored;', text, re.M)
        require(sorted(actual) == sorted(expected[key]) and summaries == [str(count)],
                f'{key} raw test output mismatch')
        binary = read('test-binaries.json')[binname]
        retained = Path(binary['retained'])
        require(binary['sha256'] == sha(retained), f'{key} retained executable hash mismatch')
        executed = Path(binary['executed'])
        require(executed.is_file() and sha(executed) == binary['sha256'],
                f'{key} executed test binary no longer matches retained hash')
        cargo_artifacts = []
        for line in (S / outname).read_text().splitlines():
            if line.startswith('{'):
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if record.get('reason') == 'compiler-artifact' and record.get('profile', {}).get('test') and record.get('executable'):
                    cargo_artifacts.append(Path(record['executable']))
        if key == 'helper-unit':
            expected_executed = str(S / 'helper-test-bin')
            expected_retained = str(S / 'test-bin/helper-unit/helper-test-bin')
            compile_row = next(r for r in read('build-receipt.json') if r['name'] == 'helper-compile')
            require(compile_row['exit'] == 0 and binary['executed'] == expected_executed
                    and binary['retained'] == expected_retained
                    and compile_row.get('command') == [
                        'rustc', '--edition=2024', '--crate-name', 'pause_route_profile', '--test',
                        str(S / 'source/crates/deadpan-core/src/pause_route_profile.rs'),
                        '-C', 'opt-level=2', '-C', 'debug-assertions=no', '-C', 'overflow-checks=on',
                        '-F', 'unsafe_code', '-o', str(S / 'helper-test-bin')],
                    'standalone helper executable/compile receipt mismatch')
            require(sha(Path(expected_executed)) == binary['sha256'] == sha(Path(expected_retained)),
                    'standalone helper compile output/record/retained hashes differ')
            require(not cargo_artifacts, 'standalone rustc helper unexpectedly emitted a Cargo test artifact')
        else:
            require(len(cargo_artifacts) == 1 and sha(cargo_artifacts[0]) == binary['sha256'],
                    f'{key} Cargo artifact identity mismatch')
    parser_receipt = read('parser-test-receipt.json')
    require(parser_receipt.get('exit') == 0 and parser_receipt.get('tests') == 20,
            'parser synthetic test receipt mismatch')
    require(parser_receipt.get('command') == ['python', '-m', 'unittest', '-v', 'test_profile_parser.py'],
            'parser test command mismatch')
    parser_log = (S / 'parser-tests.log').read_text()
    require('Ran 20 tests' in parser_log and 'OK' in parser_log, 'parser test log does not show 20 passing cases')


def verify_trials() -> tuple[list, dict]:
    build_rows = read('build-receipt.json')
    build_term = read('build-terminal.json')
    require([r.get('name') for r in build_rows] ==
            ['rustc', 'cargo', 'format', 'helper-compile', 'helper-unit', 'store-preparation', 'release-app', 'strict'],
            'seven build stages/order mismatch')
    require(len(build_rows) == 8 and all(r.get('exit') == 0 for r in build_rows)
            and build_term.get('status') == 'complete' and build_term.get('steps') == 8,
            'build is not terminal and successful')
    build_commands = {
        'rustc': ['rustc', '--version', '--verbose'],
        'cargo': ['cargo', '--version'],
        'format': ['cargo', 'fmt', '--all', '--check'],
        'helper-compile': ['rustc', '--edition=2024', '--crate-name', 'pause_route_profile', '--test', str(S / 'source/crates/deadpan-core/src/pause_route_profile.rs'), '-C', 'opt-level=2', '-C', 'debug-assertions=no', '-C', 'overflow-checks=on', '-F', 'unsafe_code', '-o', str(S / 'helper-test-bin')],
        'helper-unit': [str(S / 'helper-test-bin')],
        'store-preparation': ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-store', '--test', 'compound_preparation', '--message-format=json-render-diagnostics'],
        'release-app': ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app', '--features', 'ui-harness', '--bin', 'deadpan-app'],
        'strict': ['cargo', 'clippy', '--locked', '--release', '-p', 'deadpan-core', '-p', 'deadpan-store', '-p', 'deadpan-app', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings'],
    }
    for row in build_rows:
        require(row.get('command') == build_commands[row['name']], f"build command mismatch: {row['name']}")
        require((S / f"{row['name']}.stdout").is_file() and (S / f"{row['name']}.stderr").is_file(),
                f"missing build command logs: {row['name']}")
        require(row.get('cwd') == str(S / 'source'), f"unexpected build cwd: {row['name']}")
    require((S / 'rustc.stdout').read_text().startswith('rustc 1.97.1 ')
            and (S / 'cargo.stdout').read_text().startswith('cargo 1.97.1 '),
            'toolchain outputs do not identify pinned Rust 1.97.1')
    measure_rows = read('measurement-receipt.json')
    terminal = read('measurement-terminal.json')
    expected_names = []
    for rotation, order in enumerate(ROTATIONS, 1):
        for kind in order:
            stem = f'rotation-{rotation}-{kind}'
            expected_names.extend([stem + '-make', stem])
    require([r.get('name') for r in measure_rows] == expected_names
            and all(r.get('exit') == 0 for r in measure_rows), 'measurement command receipt mismatch')
    control_path = read('baseline.json')['control']['path']
    app_path = read('binaries.json')['deadpan-app']['executed']
    perf_path = '/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf'
    for row in measure_rows:
        stem = row['name'][:-5] if row['name'].endswith('-make') else row['name']
        if row['name'].endswith('-make'):
            fixture = str(S / f'{stem}.deadpan')
            expected_command = [perf_path, 'make-large', fixture, '--beats', '10000']
            expected_gate = None
        else:
            kind = stem.rsplit('-', 1)[1]
            binary = control_path if kind == 'control' else app_path
            expected_command = [binary, '--ui-check', '--mode', 'performance', '--scenario',
                                'large-first-pause', '--project', str(S / f'{stem}.deadpan'),
                                '--output', str(S / f'{stem}-report')]
            expected_gate = '1' if kind == 'on' else None
        require(row.get('command') == expected_command, f"measurement command mismatch: {row['name']}")
        require(row.get('profile_gate') == expected_gate, f"measurement gate mismatch: {row['name']}")
        require((S / f"{row['name']}.stdout").is_file() and (S / f"{row['name']}.stderr").is_file(),
                f"missing measurement command logs: {row['name']}")
    require(terminal.get('status') == 'complete' and terminal.get('trials') == 9
            and terminal.get('commands') == 18, 'measurement not complete')
    base = read('baseline.json')
    control = base['control']
    require(sha(Path(control['path'])) == control['sha256'], 'control app hash mismatch')
    bins = read('binaries.json')
    app = bins['deadpan-app']
    require(app['sha256'] == build_term['app_sha256'] and sha(Path(app['executed'])) == app['sha256'],
            'candidate release app mismatch')
    for name, item in bins.items():
        require(sha(Path(item['executed'])) == item['sha256'], f'executed binary mismatch: {name}')
    qualified = json.loads(Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json').read_text())
    for name in ('deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track'):
        require(bins[name]['sha256'] == qualified[name]['sha256']
                and bins[name]['qualified_helper_reuse'] is True,
                f'qualified helper identity mismatch: {name}')
    inputs = read('baseline-inputs.json')
    parser_spec = importlib.util.spec_from_file_location('pause_profile_parser', S / 'check_profile.py')
    parser = importlib.util.module_from_spec(parser_spec)
    parser_spec.loader.exec_module(parser)
    trials = read('trials.json')
    require(len(trials) == 9, 'nine trials missing')
    expected_pairs = {(r, k) for r, order in enumerate(ROTATIONS, 1) for k in order}
    require({(int(t['rotation']), t['kind']) for t in trials} == expected_pairs, 'trial matrix mismatch')
    for trial in trials:
        r, kind = int(trial['rotation']), trial['kind']
        stem = f'rotation-{r}-{kind}'
        binary_hash = control['sha256'] if kind == 'control' else app['sha256']
        require(trial.get('binary_sha256') == binary_hash, f'{stem} binary hash mismatch')
        require(trial.get('checks') == 7 and trial.get('kestrel_checks') == 1,
                f'{stem} workflow check count mismatch')
        require(trial.get('authored_input_unchanged') is True and trial.get('wal_empty') is True,
                f'{stem} input/WAL guard flag missing')
        before, after = read(stem + '-input-before.json'), read(stem + '-input-after.json')
        require(set(before) ^ set(after) <= SIDECARS, f'{stem} unexpected filesystem path changes')
        require({p: v for p, v in before.items() if p not in SIDECARS} ==
                {p: v for p, v in after.items() if p not in SIDECARS}, f'{stem} authored input changed')
        require(before.get('project.sqlite-wal', {}).get('bytes', 0) == 0
                and after.get('project.sqlite-wal', {}).get('bytes', 0) == 0, f'{stem} WAL nonempty')
        preflight = read(stem + '-preflight.json')
        require(preflight and all(x.get('ps_exit') == 0 for x in preflight)
                and preflight[-1].get('load', [99])[0] <= 3
                and not preflight[-1].get('conflicting_processes'), f'{stem} quiet preflight failed')
        report = read(f'{stem}-report/report.json')
        require(report.get('failed') is False, f'{stem} native report failed')
        meta = report.get('metadata', {})
        require(meta.get('binary_sha256') == binary_hash and meta.get('build_profile') == 'release',
                f'{stem} report executable metadata mismatch')
        require(meta.get('fixture_sha256') == before['project.sqlite']['sha256'], f'{stem} fixture hash mismatch')
        require(meta.get('cargo_lock_sha256_at_run') == inputs['Cargo.lock'], f'{stem} Cargo.lock mismatch')
        require(meta.get('physical_display_measured') is False, f'{stem} display flag mismatch')
        scenarios = report.get('scenarios', [])
        require([x.get('name') for x in scenarios] == ['kestrel-shortcuts', 'large-first-pause'],
                f'{stem} report scenario list mismatch')
        require(len(scenarios[0].get('checks', [])) == 1 and len(scenarios[1].get('checks', [])) == 7
                and all(c.get('passed') is True for s in scenarios for c in s.get('checks', [])),
                f'{stem} scenario checks incomplete/failed')
        metric_rows = [x for x in scenarios[1].get('timings', [])
                       if x.get('name', '').startswith('large_first_pause')]
        require(len(metric_rows) == 2 and {x.get('name') for x in metric_rows} == METRICS,
                f'{stem} timing rows mismatch')
        raw_metrics = {}
        for item in metric_rows:
            value = item.get('summary', {}).get('completed', {}).get('p50_ms')
            require(finite_number(value), f'{stem} has invalid metric {item.get("name")}')
            raw_metrics[item['name']] = value
        require(set(trial.get('metrics', {})) == METRICS and trial['metrics'] == raw_metrics,
                f'{stem} summarized metrics differ from raw report')
        warning_count = sum(len(sc.get('findings', [])) for sc in scenarios)
        require(trial.get('warnings') == warning_count, f'{stem} warning count differs from report')
        stderr = (S / f'{stem}.stderr').read_text(encoding='utf-8')
        if kind == 'on':
            parsed = parser.parse_profile(stderr)
            saved = read(stem + '-profile.json')
            require(parsed == saved == trial.get('profile'), f'{stem} raw/saved/trial profile mismatch')
            selected = parsed['selected']
            phase_names = [p['name'] for p in selected.get('phases', [])]
            expected_names = parser.PHASES
            require(parsed.get('schema') == 'DEADPAN_PAUSE_ROUTE_PROFILE_V2'
                    and parsed.get('block_count') == 1 and phase_names == list(expected_names)
                    and len(phase_names) == 26
                    and selected.get('phase_sum_ns') == selected.get('elapsed_ns'),
                    f'{stem} profile conservation/schema mismatch')
            expected_counts = [1] * 13 + [4, 1, 1, 4, 1, 1, 1, 1, 3, 0, 1, 1, 1]
            require([p['count'] for p in selected['phases']] == expected_counts
                    and selected['phases'][22]['elapsed_ns'] == 0,
                    f'{stem} V2 expected counts/zero-count duration mismatch')
        else:
            require(trial.get('profile') is None and 'DEADPAN_PAUSE_ROUTE_PROFILE' not in stderr,
                    f'{stem} unexpected profiling data')
    summary = read('summary.json')
    expected_summary = {'diagnostic_only': True, 'n_per_kind': 3, 'kinds': {},
                        'phase_accounting': 'exclusive; exact conservation; timer/helper overhead included',
                        'no_percentile_target_or_memory_claim': True}
    for kind in ('control', 'off', 'on'):
        lane = [t for t in trials if t['kind'] == kind]
        expected_summary['kinds'][kind] = {
            metric: {'min': min(t['metrics'][metric] for t in lane),
                     'median': statistics.median(t['metrics'][metric] for t in lane),
                     'max': max(t['metrics'][metric] for t in lane)} for metric in METRICS}
    on = [t['profile']['selected'] for t in trials if t['kind'] == 'on']
    expected_summary['exclusive_phases'] = {
        phase: {'min_ns': min(b['phases'][i]['elapsed_ns'] for b in on),
                'median_ns': statistics.median(b['phases'][i]['elapsed_ns'] for b in on),
                'max_ns': max(b['phases'][i]['elapsed_ns'] for b in on)}
        for i, phase in enumerate(parser.PHASES)}
    expected_summary['worker_outer_ns'] = [b['elapsed_ns'] for b in on]
    require(summary == expected_summary, 'full summary differs from recomputation')
    return trials, summary


def safe_fixture_archive(stem: str, after: dict, stage: Path) -> dict:
    root = S / f'{stem}.deadpan'
    require(root.is_dir() and not root.is_symlink(), f'fixture missing or linked: {root}')
    current = {}
    for p in sorted(root.rglob('*')):
        rel = p.relative_to(root).as_posix()
        require(rel and not PurePosixPath(rel).is_absolute() and '..' not in PurePosixPath(rel).parts,
                f'unsafe fixture path: {rel}')
        require(not p.is_symlink(), f'fixture symlink refused: {rel}')
        if p.is_dir():
            continue
        require(p.is_file(), f'fixture special file refused: {rel}')
        require(p.stat().st_nlink == 1, f'fixture hard link refused: {rel}')
        if rel in SIDECARS:
            continue
        current[rel] = {'bytes': p.stat().st_size, 'sha256': sha(p)}
    expected = {k: v for k, v in after.items() if k not in SIDECARS}
    require(current == expected, f'{stem} current fixture differs from guarded after map')
    require(sum(v['bytes'] for v in current.values()) <= 50 * 1024 * 1024,
            f'{stem} fixture archive exceeds 50 MiB')
    out = stage / 'fixtures' / (stem + '.tar.gz')
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('wb') as raw:
        with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
            with tarfile.open(fileobj=gz, mode='w') as tar:
                for rel, meta in current.items():
                    info = tarfile.TarInfo(rel)
                    info.size = meta['bytes']; info.mode = 0o600; info.mtime = 0
                    info.uid = info.gid = 0; info.uname = info.gname = ''
                    with (root / rel).open('rb') as f:
                        tar.addfile(info, f)
    observed = {}
    with tarfile.open(out, 'r:gz') as tar:
        members = tar.getmembers()
        require(len(members) == len(current), f'{stem} archive member count mismatch')
        for member in members:
            relpath = PurePosixPath(member.name)
            require(member.isfile() and not relpath.is_absolute() and '..' not in relpath.parts,
                    f'{stem} unsafe archive member')
            f = tar.extractfile(member)
            require(f is not None, f'{stem} archive member unreadable')
            h = hashlib.sha256(); size = 0
            for block in iter(lambda: f.read(1 << 20), b''):
                h.update(block); size += len(block)
            observed[member.name] = {'bytes': size, 'sha256': h.hexdigest()}
    require(observed == current, f'{stem} archive payload mismatch')
    return {'path': str(out.relative_to(stage)), 'files': len(current),
            'bytes': sum(x['bytes'] for x in current.values()), 'verified': True}


def main() -> None:
    require(not DEST.exists(), 'evidence destination already exists')
    go = read('PACKAGING-GO.json')
    self_hash = sha(Path(__file__))
    final_review = read('packager-final-review.json')
    require(final_review.get('status') == 'no_findings' and not final_review.get('findings')
            and final_review.get('script_sha256') == self_hash,
            'final independent packager review does not approve these exact bytes')
    require(go.get('authorized') is True and go.get('packager_sha256') == self_hash
            and go.get('packager_review_sha256') == sha(S / 'packager-final-review.json'),
            'GO does not bind this packager and its final review')
    require(go.get('source_map_sha256') == sha(S / 'frozen-sources.json')
            and go.get('source_review_sha256') == sha(S / 'source-review.json')
            and go.get('driver_review_sha256') == sha(S / 'driver-review.json')
            and go.get('driver_pins_sha256') == sha(S / 'driver-pins.json')
            and go.get('parser_review_sha256') == sha(S / 'parser-review.json')
            and go.get('parser_test_receipt_sha256') == sha(S / 'parser-test-receipt.json')
            and go.get('host_observations_sha256') == sha(S / 'host-observations.json')
            and go.get('freeze_provenance_sha256') == sha(S / 'freeze-provenance.md'),
            'GO identity bindings mismatch')
    require(go.get('build_terminal_sha256') == sha(S / 'build-terminal.json')
            and go.get('measurement_terminal_sha256') == sha(S / 'measurement-terminal.json'),
            'GO terminal receipt bindings mismatch')
    require(go.get('retain_fixture_archives') is True,
            'GO must explicitly authorize bounded fixture archives')
    source_map, inputs, sr, dr = verify_source()
    require((S / 'host-observations.json').is_file(), 'sanitized public host observations missing')
    require(read('driver-pins.json').get('host-observations.json') == sha(S / 'host-observations.json'),
            'public host observations do not match driver pin')
    verify_tests()
    trials, summary = verify_trials()

    stage = Path(tempfile.mkdtemp(prefix='.current-pause-detail-stage-', dir=DEST.parent))
    compressed = []
    archive_rows = []
    try:
        names = [
            'scope.md', 'freeze-provenance.md', 'coarse-plan.md', 'coarse-summary.json', 'parser-notes.json', 'parser-review.json',
            'baseline.json', 'baseline-inputs.json', 'coarse-baseline.json', 'coarse-baseline-inputs.json',
            'coarse-baseline-sources.json', 'coarse-frozen-sources.json', 'frozen-sources.json',
            'overlay-audit.json', 'overlay.patch', 'expected-tests.json', 'expected-helper-tests.json',
            'source-review.json', 'driver-review.json', 'driver-pins.json', 'driver-preparation.json',
            'packager-final-review.json',
            'check_profile.py', 'test_profile_parser.py', 'parser-tests.log', 'parser-test-receipt.json',
            'build.py', 'measure.py', 'packaging-draft/package.py',
            'build-receipt.json', 'build-terminal.json', 'build-owner.json',
            'measurement-receipt.json', 'measurement-terminal.json', 'measurement-owner.json',
            'trials.json', 'summary.json', 'test-binaries.json', 'binaries.json',
            'cargo-rustc-info.json', 'host-observations.json', 'PACKAGING-GO.json',
            'rustc.stdout', 'rustc.stderr', 'cargo.stdout', 'cargo.stderr',
            'format.stdout', 'format.stderr', 'helper-compile.stdout', 'helper-compile.stderr',
            'helper-unit.stdout', 'helper-unit.stderr',
            'store-preparation.stdout', 'store-preparation.stderr', 'release-app.stdout',
            'release-app.stderr', 'strict.stdout', 'strict.stderr',
        ]
        for name in names:
            put(S / name, stage, f'run/{name}', compressed)
        put(Path(__file__), stage, 'run/packaging-draft/package_v2.py', compressed)
        for review_path in sorted(S.glob('*review*.json')):
            if review_path.name not in names:
                put(review_path, stage, f'run/reviews/{review_path.name}', compressed)
        for name, review in (('source-review.json', sr), ('driver-review.json', dr)):
            require(json.loads((stage / 'run' / name).read_text()) == review, f'{name} copy mismatch')
        # Preserve actual parent-captured build/measurement driver logs using their producer names.
        for prefix in ('build-driver', 'measurement-driver'):
            matches = sorted([p for p in S.glob(prefix + '.*') if p.is_file()])
            require(any(p.suffix == '.stdout' for p in matches) and any(p.suffix == '.stderr' for p in matches),
                    f'missing captured {prefix} stdout/stderr')
            for p in matches:
                if p.suffix in {'.stdout', '.stderr'}:
                    put(p, stage, f'run/{p.name}', compressed)
        # Preserve all corrected setup/review history with its supplied provenance, text only.
        for src in sorted((S / 'history').rglob('*')):
            if src.is_file() and src.suffix.lower() in {'.json', '.md', '.py', '.txt', '.patch', '.stdout', '.stderr'}:
                put(src, stage, f'history/{src.relative_to(S / "history")}', compressed)
        # Keep only reviewed changed-source snapshots in plain-text suffixes; never copy the private source tree.
        for root in ('source-retention/before', 'source-retention/after', 'implementer-notes'):
            base = S / root
            for src in sorted(base.rglob('*')):
                if src.is_file() and (src.name == 'HANDOFF.md' or src.suffix in {'.txt', '.json', '.patch'}):
                    put(src, stage, f'{root}/{src.relative_to(base)}', compressed)
        for row in read('measurement-receipt.json'):
            command_name = row['name']
            if command_name.endswith('-make'):
                for suffix in ('.stdout', '.stderr'):
                    put(S / (command_name + suffix), stage, f'trials/{command_name + suffix}', compressed)
        for stem in TRIALS:
            for suffix in ('.stdout', '.stderr', '-input-before.json', '-input-after.json', '-preflight.json'):
                put(S / (stem + suffix), stage, f'trials/{stem + suffix}', compressed)
            put(S / f'{stem}-report/report.json', stage,
                f'trials/{stem}-report/report.json', compressed, force_gzip=True)
            if stem.endswith('-on'):
                put(S / f'{stem}-profile.json', stage, f'trials/{stem}-profile.json', compressed)
            after = read(stem + '-input-after.json')
            archive_rows.append(safe_fixture_archive(stem, after, stage))
        require(sum(row['bytes'] for row in archive_rows) <= 100 * 1024 * 1024,
                'combined fixture retention exceeds 100 MiB bound')
        compression_receipt = {'verified_gzip_payloads': compressed}
        (stage / 'compression-verification.json').write_text(
            json.dumps(compression_receipt, indent=2, sort_keys=True) + '\n')
        (stage / 'fixture-archive-verification.json').write_text(
            json.dumps({'archives': archive_rows}, indent=2, sort_keys=True) + '\n')
        payload = []
        for p in sorted(x for x in stage.rglob('*') if x.is_file() and x.name not in {'inventory.json', 'PACKAGE-SUMMARY.md'}):
            payload.append({'path': p.relative_to(stage).as_posix(), 'bytes': p.stat().st_size, 'sha256': sha(p)})
        inventory = {'format': 'deadpan-evidence-inventory-v1', 'total_files': len(payload),
                     'total_bytes': sum(x['bytes'] for x in payload), 'files': payload}
        inv = stage / 'inventory.json'
        inv.write_text(json.dumps(inventory, indent=2, sort_keys=True) + '\n')
        inv_hash = sha(inv)
        require(json.loads(inv.read_text()) == inventory, 'inventory reread mismatch')
        for row in payload:
            p = stage / row['path']
            require(p.stat().st_size == row['bytes'] and sha(p) == row['sha256'],
                    f'payload reread mismatch: {row["path"]}')
        (stage / 'PACKAGE-SUMMARY.md').write_text(
            '# Current pause detail diagnostic evidence\n\n'
            'Diagnostic only. This package does not establish a mainline improvement, p95, target pass, or product gate closure.\n\n'
            f'- Payload files: {len(payload)}\n- Payload bytes: {sum(x["bytes"] for x in payload)}\n'
            f'- Inventory SHA-256: `{inv_hash}`\n', encoding='utf-8')
        require(not DEST.exists(), 'destination appeared during package staging')
        stage.rename(DEST)
        print(json.dumps({'destination': str(DEST), 'payload_files': len(payload),
                          'payload_bytes': sum(x['bytes'] for x in payload),
                          'inventory_sha256': inv_hash, 'trials': len(trials),
                          'profiled_trials': sum(t['kind'] == 'on' for t in trials)}, sort_keys=True))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise


if __name__ == '__main__':
    main()
