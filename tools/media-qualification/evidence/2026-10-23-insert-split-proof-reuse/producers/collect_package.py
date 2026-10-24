from pathlib import Path
import hashlib, json, statistics

S = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23')
def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def read(name):
    return json.loads((S / name).read_text())

build = read('build-receipt.json')
assert [r['name'] for r in build] == read('expected-build-stages.json')
assert len(build) == 9 and all(r['exit'] == 0 and r['validation'] == 'passed' for r in build)
assert read('build-terminal.json')['status'] == 'complete'
assert read('build-terminal.json')['app_sha256'] == read('binaries.json')['deadpan-app']['sha256']
assert build[4]['tests']['passed_per_summary'] == [724]
assert len(build[4]['tests']['executed_tests']) == 724
assert build[5]['tests']['passed_per_summary'] and sum(build[5]['tests']['passed_per_summary']) == 22
assert build[6]['tests']['passed_per_summary'] == [12]
assert read('measurement-terminal.json') == {'status': 'complete', 'trials': 10, 'commands': 20}
trials, commands = read('trials.json'), read('measurement-receipt.json')
assert len(trials) == 10 and len(commands) == 20 and all(r['exit'] == 0 for r in commands)
assert sum(r['checks'] for r in trials) == 70 and sum(r['kestrel_checks'] for r in trials) == 10
assert sum(r['warnings'] for r in trials) == 20
assert all(r['authored_input_unchanged'] and r['wal_empty'] for r in trials)
for name, row in read('binaries.json').items():
    assert sha(row['executed']) == row['sha256']
control = read('baseline.json')['control']
assert sha(control['path']) == control['sha256']
assert len({(r['rotation'], r['kind']) for r in trials}) == 10
deltas = {name: [] for name in ['commit', 'composition']}
metric_names = {'commit': 'large_first_pause_input_to_commit_ms',
                'composition': 'large_first_pause_input_to_offscreen_background_composition_ms'}
for rotation in range(1, 6):
    pair = {r['kind']: r for r in trials if r['rotation'] == rotation}
    assert set(pair) == {'control', 'candidate'}
    for kind, row in pair.items():
        expected_hash = control['sha256'] if kind == 'control' else read('binaries.json')['deadpan-app']['sha256']
        assert row['binary_sha256'] == expected_hash
        report = json.loads((S / f'rotation-{rotation}-{kind}-report/report.json').read_text())
        assert report['failed'] is False
        assert report['metadata']['binary_sha256'] == expected_hash
        assert report['metadata']['gpu_readback_in_timing_run'] is False
        assert report['metadata']['physical_display_measured'] is False
        case = report['scenarios'][1]
        assert len(case['checks']) == 7 and all(c['passed'] for c in case['checks'])
        measured = {r['name']: r['summary']['completed'] for r in case['timings']
                    if r['name'] in metric_names.values()}
        assert set(measured) == set(metric_names.values())
        assert all(v['n'] == 1 for v in measured.values())
        assert row['metrics'] == {name: value['p50_ms'] for name, value in measured.items()}
    for name, key in metric_names.items():
        deltas[name].append(pair['control']['metrics'][key] - pair['candidate']['metrics'][key])
summary = read('summary.json')
assert statistics.median(deltas['commit']) == summary['paired_commit_reduction_median_ms']
assert statistics.median(deltas['composition']) == summary['paired_composition_reduction_median_ms']
media = read('media-resume-terminal.json')
prior = read('media-terminal.json')
assert media['status'] == 'complete' and media['tests'] == 1 and media['archive_recompiled'] is False
assert prior['status'] == 'failed' and prior['stage'] == 'list'
assert prior['archive_sha256'] == media['archive_sha256'] == sha(S / 'media-one-leaf.tar.zst')
assert read('media-resume-receipt.json')[-1]['exit'] == 0
assert [r['exit'] for r in read('media-receipt.json')] == [0, 0, 96]
assert media['evidence']['canonical_history_pcm_checks'] is True
assert media['evidence']['verified_pictures_per_export'] == 45
assert media['evidence']['verified_samples_per_export'] == 72072
assert len(media['evidence']['exports']) == 2
for row in media['evidence']['exports']:
    assert sha(row['movie']) == row['movie_sha256']
for row in read('media-resume-binaries.json')['records']:
    assert all(sha(path) == row['sha256'] for path in row['paths'])
promotion = read('main-promotion-source-check.json')
assert promotion['main_matches_exact_pretested_rust_map'] and promotion['six_inputs_match']
assert promotion['source_map_sha256'] == sha(S / 'frozen-sources.json')
audit = {'status': 'passed', 'root_semantic_audit': True, 'build_stages': 9,
         'core_unit': 724, 'core_integration': 22, 'store': 12,
         'native_trials': 10, 'native_commands': 20, 'native_checks': 70,
         'kestrel_checks': 10, 'layout_warnings': 20,
         'paired_commit_saved_ms': statistics.median(deltas['commit']),
         'paired_composition_saved_ms': statistics.median(deltas['composition']),
         'media_selected_tests': 1, 'verified_exports': 2,
         'verified_pictures_per_export': 45, 'verified_samples_per_export': 72072,
         'original_media_setup_failure_retained': True, 'archive_recompiled': False,
         'main_equals_pretested_source': True, 'no_percentile_or_target_claim': True}
with (S / 'production-semantic-audit.json').open('x') as out:
    out.write(json.dumps(audit, indent=2) + '\n')

files, destinations = [], set()
def add(source, destination):
    source = Path(source)
    assert source.is_file() and not source.is_symlink(), source
    assert not destination.endswith('.rs')
    mode = 'identity'
    if source.stat().st_size > 100000 and source.suffix.lower() not in {'.mp4', '.gz', '.zst', '.png', '.jpg'}:
        destination += '.gz'
        mode = 'gzip'
    assert destination not in destinations
    destinations.add(destination)
    row = {'source': str(source), 'destination': destination,
           'sha256': sha(source), 'bytes': source.stat().st_size, 'encoding': mode}
    files.append(row)
    return {key: row[key] for key in ['source', 'destination', 'sha256', 'bytes']}

excluded = {'CURRENT-ROOT-STATE.json', 'approved-package-manifest.json', 'PACKAGING-GO.json', 'packager-final-review.json'}
for path in sorted(S.iterdir()):
    if path.name in excluded or not path.is_file():
        continue
    if path.suffix == '.json':
        add(path, 'protocol/' + path.name)
    elif path.suffix in {'.py', '.patch', '.txt'}:
        add(path, 'producers/' + path.name)
    elif path.suffix in {'.stdout', '.stderr'}:
        add(path, 'logs/' + path.name)
for path in sorted((S / 'packaging-draft').glob('*.json')):
    add(path, 'reviews/' + path.name)
for path in sorted((S / 'packaging-draft').glob('*.py')):
    if path.name != 'package_v2.py':
        add(path, 'producers/packaging-draft/' + path.name)
inputs = read('baseline-inputs.json')
for rel, digest in inputs.items():
    path = S / 'source' / rel
    assert sha(path) == digest
    add(path, 'inputs/' + rel)
for report in sorted(S.glob('rotation-*-report/report.json')):
    add(report, 'native/' + report.parent.name + '/report.json')
ledger = {row['source']: row for row in files}
fixtures = []
for trial in trials:
    stem = f"rotation-{trial['rotation']}-{trial['kind']}"
    root = S / (stem + '.deadpan')
    allowed = {}
    for name in ['project.sqlite-shm', 'project.sqlite-wal']:
        path = root / name
        if path.exists():
            assert path.is_file() and not path.is_symlink()
            if name.endswith('-wal'):
                assert path.stat().st_size == 0
            allowed[name] = {'bytes': path.stat().st_size}
    def ref(suffix):
        row = ledger[str(S / (stem + suffix))]
        return {key: row[key] for key in ['source', 'destination', 'sha256', 'bytes']}
    fixtures.append({'name': stem, 'root': str(root),
                     'before_map': ref('-input-before.json'), 'after_map': ref('-input-after.json'),
                     'archive_destination': 'fixtures/' + stem + '.tar.gz', 'allowed_sidecars': allowed})
inventory = read('media-resume-evidence-inventory.json')
media_root = Path(inventory['root'])
assert set(inventory['files']) == {str(p.relative_to(media_root)) for p in media_root.rglob('*') if p.is_file()}
for rel, metadata in inventory['files'].items():
    path = media_root / rel
    assert path.stat().st_size == metadata['bytes'] and sha(path) == metadata['sha256']
    add(path, 'media/evidence/' + rel)
manifest = {'schema': 'deadpan-insert-split-proof-package-manifest-1',
            'source_map_sha256': sha(S / 'frozen-sources.json'),
            'binary_manifest_sha256': sha(S / 'binaries.json'),
            'build_terminal_sha256': sha(S / 'build-terminal.json'),
            'measurement_terminal_sha256': sha(S / 'measurement-terminal.json'),
            'media_completion': {
                'terminal_path': str(S / 'media-resume-terminal.json'),
                'terminal_sha256': sha(S / 'media-resume-terminal.json'),
                'receipt_path': str(S / 'media-resume-receipt.json'),
                'receipt_sha256': sha(S / 'media-resume-receipt.json'),
                'archive_path': str(S / 'media-one-leaf.tar.zst'),
                'archive_sha256': sha(S / 'media-one-leaf.tar.zst'),
                'historical_failure': {
                    'terminal_path': str(S / 'media-terminal.json'),
                    'terminal_sha256': sha(S / 'media-terminal.json'),
                    'receipt_path': str(S / 'media-receipt.json'),
                    'receipt_sha256': sha(S / 'media-receipt.json')}},
            'files': files, 'fixtures': fixtures}
with (S / 'approved-package-manifest.json').open('x') as out:
    out.write(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'files': len(files), 'fixtures': len(fixtures),
                  'source_bytes_before_compression': sum(x['bytes'] for x in files),
                  'manifest_sha256': sha(S / 'approved-package-manifest.json')}))
