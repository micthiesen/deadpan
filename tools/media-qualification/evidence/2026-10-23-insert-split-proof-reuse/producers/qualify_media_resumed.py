from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import time

S = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23')
SOURCE = S / 'source'
TARGET = S / 'target'
MAIN = Path('/Users/michael/Code/deadpan')
ARCHIVE = S / 'media-one-leaf.tar.zst'
EXTRACT = S / 'media-resume-one-leaf-extracted'
RESULTS = S / 'media-resume-one-leaf-fixture.json'
CLI = TARGET / 'release/deadpan-cli'
STAGES = ['rustc', 'cargo', 'format', 'core-unit-build', 'core-unit-all',
          'core-integration', 'store-preparation', 'release-app', 'strict']
ENV = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_TERM_COLOR='never',
           DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',
           DEADPAN_REQUIRE_PHYSICAL_MAC='1', DEADPAN_REQUIRE_SYNTHETIC_WORKER='1',
           DEADPAN_PREVIEW_EXPORT_KEEP='1', DEADPAN_PREVIEW_EXPORT_RESULTS=str(RESULTS))
ENV.pop('DEADPAN_PAUSE_ROUTE_PROFILE', None)
STATE = {'stage': 'preflight', 'archive_sha256': 'b2ae4159642e2b74c830724e9aa5a789eaeea562563f747ad00a669b189d20c5'}
OWNED = False


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(name):
    return json.loads((S / name).read_text())


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def exclusive(name, value):
    with (S / name).open('x') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def guard():
    static = read('media-static-pins.json')
    assert sha(S / 'frozen-sources.json') == static['source_map_sha256']
    sources = read('frozen-sources.json')
    assert len(sources) == static['source_count'] == 2053
    assert {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')} == sources
    assert sha(S / 'qualified-base-sources.json') == static['qualified_base_map_sha256']
    main_paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=MAIN, text=True).splitlines()
    assert {name: sha(MAIN / name) for name in main_paths} == read('qualified-base-sources.json')
    inputs = read('baseline-inputs.json')
    assert len(inputs) == 6
    assert all(sha(SOURCE / name) == digest and sha(MAIN / name) == digest
               for name, digest in inputs.items())
    review = read('source-review.json')
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == sha(S / 'frozen-sources.json')
    build = read('build-receipt.json')
    terminal = read('build-terminal.json')
    assert read('expected-build-stages.json') == STAGES
    assert [row['name'] for row in build] == STAGES
    assert all(row['exit'] == 0 and row['validation'] == 'passed' for row in build)
    assert terminal['status'] == 'complete' and terminal['steps'] == 9
    assert terminal['stage_names'] == STAGES
    assert terminal['app_sha256'] == read('binaries.json')['deadpan-app']['sha256']
    assert sha(read('binaries.json')['deadpan-app']['executed']) == terminal['app_sha256']
    control = read('baseline.json')['control']
    assert control['sha256'] == static['qualified_control_sha256']
    assert sha(control['path']) == control['sha256']
    pins = read('driver-pins.json')
    assert all(sha(S / name) == digest for name, digest in pins.items())
    # A separate freeze avoids changing the already running build's pin file.
    media_pins = read('media-driver-pins.json')
    assert {'qualify_media.py', 'media-static-pins.json', 'driver-pins.json',
            'frozen-sources.json', 'qualified-base-sources.json', 'baseline-inputs.json',
            'expected-build-stages.json', 'expected-tests.json', 'source-review.json'} <= media_pins.keys()
    assert all(sha(S / name) == digest for name, digest in media_pins.items())
    for entry in [static['fixture'], static['nextest_config']]:
        assert sha(S / entry['path']) == entry['sha256']
    for name, digest in static['qualified_helper_sha256'].items():
        row = read('binaries.json')[name]
        assert row['freshly_built'] is False and row['qualified_helper_reuse'] is True
        assert Path(row['executed']).resolve() == (S / 'bin' / name).resolve()
        assert row['sha256'] == digest == sha(row['executed'])
    resume = read('media-resume-static-manifest.json')
    assert resume['required_source_map_sha256'] == static['source_map_sha256']
    assert resume['original_script'] == {
        'path': 'qualify_media.py',
        'sha256': '4fcae22c9000f9cd8686924e8b1df79f74cbd9f79a7b0d42a29db09ba466e50c'}
    assert sha(S / resume['original_script']['path']) == resume['original_script']['sha256']
    assert resume['archive_sha256'] == STATE['archive_sha256']
    assert all(sha(S / name) == digest for name, digest in resume['original_artifacts'].items())
    assert resume['original_artifacts']['media-one-leaf.tar.zst'] == STATE['archive_sha256']
    failed = read('media-terminal.json')
    prior = read('media-receipt.json')
    assert failed['status'] == 'failed' and failed['stage'] == 'list'
    assert failed['exception'] == 'AssertionError' and failed['message'] == 'list'
    assert failed['archive_sha256'] == STATE['archive_sha256']
    assert [row['stage'] for row in prior] == ['hardware', 'archive', 'list']
    assert [row['exit'] for row in prior] == [0, 0, 96]
    assert all(row['cwd'] == str(SOURCE) for row in prior)
    assert prior[1]['command'] == [
        'cargo', 'nextest', 'archive', '--locked', '--release', '-p', 'deadpan-cli',
        '--test', 'preview_export', '--features', 'synthetic-worker',
        '--archive-file', str(ARCHIVE), '--zstd-level=-1']
    assert prior[2]['command'][:3] == ['cargo', 'nextest', 'list']
    assert str(S / 'media-one-leaf-extracted') in prior[2]['command']
    failure_log = (S / 'media-list.stderr').read_text()
    assert 'error canonicalizing destination directory' in failure_log
    assert str(S / 'media-one-leaf-extracted') in failure_log
    assert 'No such file or directory (os error 2)' in failure_log
    assert not any((S / name).exists() for name in [
        'media-run.stdout', 'media-run.stderr', 'media-one-leaf-fixture.json',
        'media-binaries.json', 'media-evidence-inventory.json'])
    resume_pins = read('media-resume-driver-pins.json')
    assert {'qualify_media_resumed.py', 'media-resume-static-manifest.json',
            'media-driver-pins.json', 'qualify_media.py', 'media-static-pins.json',
            'media-owner.json', 'media-terminal.json', 'media-receipt.json'} <= resume_pins.keys()
    assert all(sha(S / name) == digest for name, digest in resume_pins.items())
    assert ARCHIVE.is_file() and sha(ARCHIVE) == STATE['archive_sha256']


def checked_binaries(records):
    for row in records:
        for path in row['paths']:
            assert sha(path) == row['sha256'], (path, 'binary changed')


def run(stage, command, rows, records=()):
    STATE['stage'] = stage
    guard()
    checked_binaries(records)
    started = time.monotonic()
    with (S / f'media-resume-{stage}.stdout').open('x') as out, (S / f'media-resume-{stage}.stderr').open('x') as err:
        result = subprocess.run(command, cwd=SOURCE, env=ENV, stdout=out, stderr=err)
    row = {'stage': stage, 'command': command, 'cwd': str(SOURCE),
           'exit': result.returncode, 'seconds': time.monotonic() - started}
    rows.append(row)
    save('media-resume-receipt.json', rows)
    print(json.dumps(row), flush=True)
    guard()
    checked_binaries(records)
    assert result.returncode == 0, stage
    return (S / f'media-resume-{stage}.stdout').read_text(), (S / f'media-resume-{stage}.stderr').read_text()


def identical_copy(source, dest, digest):
    assert source.is_file() and sha(source) == digest
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        assert dest.is_file() and sha(dest) == digest, ('refuse different existing binary', dest)
    else:
        shutil.copy2(source, dest)
    assert sha(dest) == digest


def validate_evidence():
    evidence = json.loads(RESULTS.read_text())
    assert evidence['schema'] == 'deadpan-one-leaf-media-parity-1'
    root = Path(evidence['root']).resolve(strict=True)
    assert root.is_dir() and Path(evidence['evidence_dir']).resolve(strict=True) == root
    assert evidence['capability_present'] is True and evidence['callback_counts'] == [1, 1]
    assert evidence['pictures'] == 45 and evidence['samples'] == 72072 and evidence['channels'] == 2
    canonical = evidence['canonical_comparison']
    for name in ['document_bytes_equal', 'commit_undo_redo_bytes_equal', 'register_banks_equal',
                 'authored_limited_and_limiter_gain_bits_equal', 'partition_and_cold_reads_equal',
                 'source_pcm_nonzero', 'pause_interior_silent']:
        assert canonical[name] is True, name
    for name in ['before_document', 'after_document', 'before_pcm', 'after_pcm']:
        path = Path(canonical[name]).resolve(strict=True)
        assert path.is_relative_to(root) and path.is_file(), name
    history = evidence['history']
    assert history['undo_redo_reopen_checked'] is True and history['full_replay_checked'] is True
    assert history['receipt_shortcut_used'] is False
    exports = evidence['exports']
    assert len(exports) == 2 and {row['leg'] for row in exports} == {'cached', 'uncached'}
    for row in exports:
        movie, package = Path(row['movie']).resolve(strict=True), Path(row['package']).resolve(strict=True)
        assert movie.is_relative_to(root) and movie.is_file() and movie.stat().st_size > 0
        assert package.is_relative_to(root) and package.is_dir()
        assert isinstance(row['revision'], str) and row['revision']
        report = row['verification']
        assert report['passed'] is True
        assert len(report['pictures']) == 45
        assert report['summary']['pictures_checked'] == 45
        assert report['summary']['pictures_failed'] == report['summary']['nonzero_offsets'] == 0
        assert len(report['audio']) == 1
        audio = report['audio'][0]
        assert audio['passed'] is True and audio['project_samples'] == [0, 72072]
        assert audio['flags'] == [] and audio['uncovered_samples'] == 0
        assert audio['blocks']['low_snr_blocks'] == 0
        assert audio['offset_status'] == 'verified_zero' and audio['measured_offset_samples'] == 0
        retained = json.loads((root / f"one-leaf-{row['leg']}-verification.json").read_text())
        assert retained == report
    assert json.loads((root / 'one-leaf-media-summary.json').read_text()) == evidence
    inventory = {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
                 for p in sorted(root.rglob('*')) if p.is_file()}
    save('media-resume-evidence-inventory.json', {'root': str(root), 'files': inventory,
                                         'external_receipt_sha256': sha(RESULTS)})
    return {'root': str(root), 'exports': [{'leg': row['leg'], 'movie': row['movie'],
             'movie_sha256': sha(row['movie']), 'package': row['package'], 'revision': row['revision']}
            for row in exports], 'canonical_history_pcm_checks': True,
            'verified_pictures_per_export': 45, 'verified_samples_per_export': 72072}


def main():
    global OWNED
    for path in [EXTRACT, RESULTS, S / 'media-resume-receipt.json',
                 S / 'media-resume-terminal.json', S / 'media-resume-binaries.json',
                 S / 'media-resume-evidence-inventory.json', S / 'media-resume-hardware.json',
                 S / 'media-resume-retained-bin', S / 'media-resume-extract-created.json'] + [
                     S / f'media-resume-{stage}.{extension}'
                     for stage in ['hardware', 'list', 'run']
                     for extension in ['stdout', 'stderr']]:
        assert not path.exists(), ('refuse existing media qualification output', path)
    exclusive('media-resume-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                 'script': str(Path(__file__)), 'source': str(SOURCE),
                                 'target': str(TARGET), 'one_test_only': True})
    OWNED = True
    guard()
    static = read('media-static-pins.json')
    full_name = static['test']
    expression = f'binary(=preview_export) & test(={full_name})'
    rows = []
    out, _ = run('hardware', ['sysctl', '-n', 'hw.model', 'hw.memsize', 'hw.physicalcpu',
                             'hw.logicalcpu', 'kern.hv_vmm_present'], rows)
    hardware = out.splitlines()
    assert len(hardware) == 5 and hardware[-1] == '0', 'physical Mac required'
    assert all(int(value) > 0 for value in hardware[1:4])
    save('media-resume-hardware.json', {'model': hardware[0], 'memsize': int(hardware[1]),
                               'physicalcpu': int(hardware[2]), 'logicalcpu': int(hardware[3]),
                               'hypervisor_present': False})
    # Resume the retained successful archive; never compile or replace it.
    assert sha(ARCHIVE) == STATE['archive_sha256']
    EXTRACT.mkdir()
    assert EXTRACT.is_dir() and not any(EXTRACT.iterdir())
    exclusive('media-resume-extract-created.json', {
        'path': str(EXTRACT), 'fresh_directory': True, 'initially_empty': True,
        'reused_archive': str(ARCHIVE), 'archive_sha256': STATE['archive_sha256']})
    reuse = ['--archive-file', str(ARCHIVE), '--extract-to', str(EXTRACT),
             '--extract-overwrite', '--workspace-remap', str(SOURCE)]
    output, _ = run('list', ['cargo', 'nextest', 'list', *reuse, '-E', expression,
                            '--message-format', 'json'], rows)
    listing = json.loads(output)
    suites = listing['rust-suites']
    assert len(suites) == 1
    suite = next(iter(suites.values()))
    assert suite['package-name'] == 'deadpan-cli' and suite['binary-name'] == 'preview_export'
    assert suite['kind'] == 'test' and suite['status'] == 'listed'
    matched = [(name, row) for name, row in suite['testcases'].items()
               if row.get('filter-match', {}).get('status') == 'matches']
    assert len(matched) == 1 and matched[0][0] == full_name and matched[0][1]['ignored'] is False
    test_path = Path(suite['binary-path']).resolve(strict=True)
    extract_target = Path(listing['rust-build-meta']['target-directory']).resolve(strict=True)
    assert extract_target.is_relative_to(EXTRACT.resolve(strict=True))
    assert test_path.is_relative_to(extract_target / 'release/deps')
    cli_rows = [row for records in listing['rust-build-meta']['non-test-binaries'].values()
                for row in records if row['name'] == 'deadpan-cli' and row['kind'] == 'bin-exe']
    assert len(cli_rows) == 1
    extract_cli = (extract_target / cli_rows[0]['path']).resolve(strict=True)
    assert extract_cli.is_relative_to(extract_target / 'release')
    assert sha(CLI) == sha(extract_cli), 'archive CLI differs from compile-time CLI'
    records = [{'role': 'release_preview_export_test', 'paths': [str(test_path)], 'sha256': sha(test_path),
                'fresh_source_archive_build': True},
               {'role': 'release_cli', 'paths': [str(CLI), str(extract_cli)],
                'sha256': sha(CLI), 'fresh_source_archive_build': True,
                'actual_cli_basis': 'unchanged recipes::cli_path uses compile-time CARGO_BIN_EXE_deadpan-cli'}]
    for row in records:
        original = Path(row['paths'][0])
        retained = S / 'media-resume-retained-bin' / original.name
        assert not retained.exists(), ('refuse retained binary overwrite', retained)
        identical_copy(original, retained, row['sha256'])
        row['paths'].append(str(retained))
    for name, digest in static['qualified_helper_sha256'].items():
        qualified = S / 'bin' / name
        paths = [CLI.parent / name, extract_cli.parent / name, S / 'media-resume-retained-bin' / name]
        for dest in paths:
            identical_copy(qualified, dest, digest)
        records.append({'role': name, 'paths': [str(qualified)] + list(map(str, paths)),
                        'sha256': digest, 'fresh_source_archive_build': False,
                        'qualified_helper_reuse': True})
    save('media-resume-binaries.json', {'archive': str(ARCHIVE), 'archive_sha256': STATE['archive_sha256'],
                               'matched_tests': [name for name, _ in matched],
                               'selected_tests': [full_name], 'nextest_test_binary': str(test_path),
                               'expected_compile_time_cli': str(CLI), 'records': records})
    out, err = run('run', ['cargo', 'nextest', 'run', *reuse, '-E', expression,
                          '--run-ignored', 'all', '--test-threads', '1', '--no-fail-fast'], rows, records)
    combined = out + '\n' + err
    assert re.search(r'^\s*Starting 1 test across 1 binary(?:\s|\()', combined, re.M)
    passes = re.findall(r'^\s*PASS\s+\[[^]]+\]\s+\(1/1\)\s+deadpan-cli::preview_export\s+(\S+)\s*$', combined, re.M)
    assert passes == [full_name], passes
    summaries = re.findall(r'^\s*Summary\s+\[[^]]+\]\s+1 test run: 1 passed(?:, [0-9]+ skipped)?\s*$', combined, re.M)
    assert len(summaries) == 1, 'one successful nextest result required; zero tests is failure'
    assert 'DEADPAN_PAUSE_ROUTE_PROFILE' not in combined
    STATE['stage'] = 'evidence-validation'
    evidence = validate_evidence()
    guard()
    checked_binaries(records)
    assert [row['stage'] for row in rows] == ['hardware', 'list', 'run']
    assert all(row['exit'] == 0 for row in rows)
    exclusive('media-resume-terminal.json', {'status': 'complete', 'commands': len(rows),
              'tests': 1, 'selected_test': full_name, 'archive_sha256': STATE['archive_sha256'],
              'test_sha256': records[0]['sha256'], 'cli_sha256': records[1]['sha256'],
              'source_map_sha256': sha(S / 'frozen-sources.json'),
              'reused_archive': str(ARCHIVE), 'archive_recompiled': False,
              'original_failed_terminal_sha256': sha(S / 'media-terminal.json'),
              'original_receipt_sha256': sha(S / 'media-receipt.json'),
              'physical_mac_required': True, 'synthetic_worker_feature_required': True,
              'qualified_helper_reuse': True, 'evidence': evidence})
    print(json.dumps({'terminal': True, 'tests': 1, 'exports': 2}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        failure = {'status': 'failed', **STATE, 'exception': type(error).__name__, 'message': str(error)}
        if OWNED:
            try:
                exclusive('media-resume-terminal.json', failure)
            except FileExistsError:
                pass
        else:
            exclusive(f'refused-media-resume-{time.time_ns()}.json', failure)
        raise
