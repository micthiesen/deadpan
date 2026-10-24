"""Reviewed, manifest-driven copier. Root supplies the final immutable manifest, review, and GO."""
from pathlib import Path, PurePosixPath
import ctypes
import gzip
import hashlib
import io
import json
import os
import shutil
import stat
import tarfile
import tempfile

S = Path('/tmp/deadpan-single-leaf-net-prototype-v2-2026-10-24')
INITIAL = Path('/tmp/deadpan-single-leaf-net-prototype-2026-10-24')
REPO = Path('/Users/michael/Code/deadpan')
BASE_SOURCE = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23/source')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-24-single-leaf-net-prototype'
MANIFEST = S / 'approved-package-manifest.json'
REVIEW = S / 'packager-final-review.json'
GO = S / 'PACKAGING-GO.json'
SCRIPT = Path(__file__).absolute()
ENVELOPE = {
    'approval/approved-package-manifest.json': MANIFEST,
    'approval/packager-final-review.json': REVIEW,
    'approval/PACKAGING-GO.json': GO,
    'approval/package.py': SCRIPT,
}
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
MAX_FIXTURE_BYTES = 200_000_000
TERMINALS = {
    'build': S / 'build-terminal.json',
    'measurement': S / 'measurement-retry-terminal.json',
    'media': S / 'media-retry-terminal.json',
    'memory': S / 'memory-retry-terminal.json',
}


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    return digest_bytes(Path(path).read_bytes())


def read(path):
    return json.loads(Path(path).read_text())


def safe_relative(text):
    rel = PurePosixPath(text)
    assert text and not rel.is_absolute() and '..' not in rel.parts and '\\' not in text
    return rel


def resolve_regular(path):
    p = Path(path)
    assert not p.is_symlink(), ('symlink source refused', p)
    q = p.resolve(strict=True)
    assert q.is_file() and stat.S_ISREG(q.lstat().st_mode), ('nonregular source refused', q)
    return q


def verify_payload(item):
    src = resolve_regular(item['source'])
    raw = src.read_bytes()
    assert len(raw) == item['bytes'] and digest_bytes(raw) == item['sha256'], src
    return raw


def destination_path(root, text):
    rel = safe_relative(text)
    path = root.joinpath(*rel.parts)
    assert path.resolve().is_relative_to(root.resolve())
    return path


def snapshot(root):
    root_path = Path(root)
    assert not root_path.is_symlink(), ('authored fixture root symlink', root_path)
    root = root_path.resolve(strict=True)
    assert root.is_dir()
    rows = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise AssertionError(('fixture symlink refused', path))
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise AssertionError(('fixture special file refused', path))
        rel = path.relative_to(root).as_posix()
        rows[rel] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
    return rows


def approved_map(ref, payloads):
    # A map is always an ordinary payload in the explicit approved manifest.
    matches = [item for item in payloads
               if item['source'] == ref['source']
               and item['destination'] == ref['destination']
               and item['bytes'] == ref['bytes']
               and item['sha256'] == ref['sha256']]
    assert len(matches) == 1, ('fixture map not uniquely approved', ref)
    raw = verify_payload(matches[0])
    data = json.loads(raw)
    return data.get('files', data)


def verify_fixture(item, payloads):
    root = Path(item['root'])
    before = approved_map(item['before_map'], payloads)
    after = approved_map(item['after_map'], payloads)
    allowed = item.get('allowed_sidecars', {})
    assert set(allowed) <= SIDECARS
    actual = snapshot(root)
    for name, expected in allowed.items():
        row = actual.get(name)
        assert row is not None and row['bytes'] == expected['bytes'], (item['name'], name, row)
        if name == 'project.sqlite-wal':
            assert row['bytes'] == 0
    before_authored = {k: v for k, v in before.items() if k not in allowed}
    after_authored = {k: v for k, v in after.items() if k not in allowed}
    actual_authored = {k: v for k, v in actual.items() if k not in allowed}
    assert before_authored == after_authored == actual_authored, item['name']
    assert set(actual) - set(before) <= set(allowed), item['name']
    return actual_authored


def add_payload(stage, item):
    raw = verify_payload(item)
    out = destination_path(stage, item['destination'])
    out.parent.mkdir(parents=True, exist_ok=True)
    encoding = item.get('encoding', 'identity')
    if encoding == 'identity':
        stored = raw
    elif encoding == 'gzip':
        stored = gzip.compress(raw, compresslevel=9, mtime=0)
        assert gzip.decompress(stored) == raw
    else:
        raise AssertionError(('unknown encoding', encoding))
    out.write_bytes(stored)
    return {'path': item['destination'], 'source_sha256': item['sha256'],
            'source_bytes': len(raw), 'stored_sha256': digest(out),
            'stored_bytes': out.stat().st_size, 'encoding': encoding}


def add_fixture(stage, item, payloads):
    files = verify_fixture(item, payloads)
    total = 0
    archive_rel = item['archive_destination']
    archive = destination_path(stage, archive_rel)
    archive.parent.mkdir(parents=True, exist_ok=True)
    raw_path = archive.with_suffix('.tar')
    with tarfile.open(raw_path, 'w') as tar:
        for rel, meta in sorted(files.items()):
            source = Path(item['root']).resolve(strict=True).joinpath(*safe_relative(rel).parts)
            raw = source.read_bytes()
            assert len(raw) == meta['bytes'] and digest_bytes(raw) == meta['sha256']
            total += len(raw)
            assert total <= MAX_FIXTURE_BYTES, (item['name'], total)
            info = tarfile.TarInfo(f"{item['name']}/{rel}")
            info.size = len(raw)
            info.mode = 0o600
            tar.addfile(info, io.BytesIO(raw))
    tar_bytes = raw_path.read_bytes()
    compressed = gzip.compress(tar_bytes, compresslevel=9, mtime=0)
    raw_path.unlink()
    assert gzip.decompress(compressed) == tar_bytes
    archive.write_bytes(compressed)
    seen = {}
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar.getmembers():
            name = PurePosixPath(member.name)
            assert member.isfile() and not name.is_absolute() and '..' not in name.parts
            assert name.parts and name.parts[0] == item['name']
            f = tar.extractfile(member)
            assert f is not None
            content = f.read()
            rel = name.relative_to(item['name']).as_posix()
            assert rel in files
            assert len(content) == files[rel]['bytes'] and digest_bytes(content) == files[rel]['sha256']
            seen[rel] = {'bytes': len(content), 'sha256': digest_bytes(content)}
    assert seen == files
    return {'name': item['name'], 'archive': archive_rel,
            'archive_sha256': digest(archive), 'authored_files': len(seen),
            'authored_bytes': total, 'excluded_sidecars': item.get('allowed_sidecars', {})}


def validate_terminal_records(manifest):
    terminal_hashes = manifest['terminal_sha256']
    assert set(terminal_hashes) == set(TERMINALS)
    records = {}
    for kind, path in TERMINALS.items():
        assert digest(path) == terminal_hashes[kind], (kind, 'terminal changed')
        record = read(path)
        assert record.get('status') == 'complete', (kind, record)
        records[kind] = record
    assert records['build']['steps'] == 9
    assert records['build']['stage_names'] == json.loads((S/'expected-build-stages.json').read_text())
    assert records['build']['app_sha256'] == manifest['candidate_app_sha256']
    assert records['measurement'] == {
        'status':'complete','trials':18,'commands':36,'workflow_checks':126,'kestrel_checks':18}
    assert records['media']['status'] == 'complete' and records['media']['tests'] == 1
    assert records['media']['selected_test'] == read(S/'media-retry-static-pins.json')['test']
    assert records['media']['source_map_sha256'] == manifest['candidate_source_map_sha256']
    assert records['media']['physical_mac_required'] is True
    assert records['media']['synthetic_worker_feature_required'] is True
    assert records['memory']['status'] == 'complete' and records['memory']['trials'] == 6
    assert records['memory']['summary_sha256'] == digest(S/'memory-retry-summary.json')
    # Check trial tables against the terminal source and executable identities.
    timing = read(S/'retry-trials.json')
    assert len(timing) == 18 and all(row['source_map_sha256'] == manifest['candidate_source_map_sha256'] for row in timing)
    assert all(row['checks'] == 7 and row['kestrel_checks'] == 1 and row['authored_input_unchanged'] and row['wal_empty'] for row in timing)
    expected_apps = {'control': manifest['control_app_sha256'],
                     'baseline': manifest['matched_profile_app_sha256'],
                     'candidate': manifest['candidate_app_sha256']}
    assert {row['kind'] for row in timing} == set(expected_apps)
    assert all(row['binary_sha256'] == expected_apps[row['kind']] for row in timing)
    memory = read(S/'memory-retry-trials.json')
    assert len(memory) == 6 and all(row['profile_gate_absent'] and row['profile_output_absent'] and row['authored_paths_bytes_unchanged'] and row['wal_empty'] for row in memory)
    assert all(row['binary_sha256'] == expected_apps[row['kind']] for row in memory)
    assert {row['kind'] for row in memory} == {'baseline','candidate'}
    observer = read(S/'observer-provenance.json')
    assert digest(S/'observer-provenance.json') == manifest['observer_provenance_sha256']
    assert digest(observer['observer']) == observer['observer_sha256'] == manifest['observer_sha256']
    assert all(row['observer_sha256'] == manifest['observer_sha256'] for row in memory)
    assert all(row['checks'] == {'large-first-pause': 7, 'kestrel-shortcuts': 1}
               and row['fixture_sha256'] for row in memory)
    memory_summary = read(S/'memory-retry-summary.json')
    assert memory_summary['trials'] == 6 and memory_summary['n_per_kind'] == 3
    assert memory_summary['lane'] == 'owned-observer-memory-only'
    assert memory_summary['exact_peak'] is False and memory_summary['memory_ceiling'] is False
    assert memory_summary['sampled_whole_main_app_lifetime_lower_bound_only'] is True
    binaries = read(S/'binaries.json')
    app = binaries['deadpan-app']
    assert app['sha256'] == records['build']['app_sha256'] == manifest['candidate_app_sha256']
    assert digest(app['executed']) == app['sha256']
    for name, expected in read(S/'media-static-pins.json')['qualified_helper_sha256'].items():
        row = binaries[name]
        assert row['sha256'] == expected and digest(row['executed']) == expected
    baseline = read(S/'baseline.json')
    assert digest(baseline['control']['path']) == baseline['control']['sha256'] == manifest['control_app_sha256']
    assert digest(baseline['matched_profile']['path']) == baseline['matched_profile']['sha256'] == manifest['matched_profile_app_sha256']
    media_bins = read(S/'media-retry-binaries.json')
    assert media_bins['archive_sha256'] == records['media']['archive_sha256']
    assert media_bins['selected_tests'] == [records['media']['selected_test']]
    assert media_bins['records'][0]['sha256'] == records['media']['test_sha256']
    assert media_bins['records'][1]['sha256'] == records['media']['cli_sha256']
    for row in media_bins['records']:
        for path in row['paths']:
            assert digest(path) == row['sha256'], path
    assert manifest['binary_manifest_sha256'] == digest(S/'binaries.json')
    assert manifest['source_review_sha256'] == digest(S/'source-review.json')
    assert manifest['analysis_review_sha256'] == digest(S/'analysis-review.json')
    source_review = read(S/'source-review.json')
    assert source_review['status'] == 'no_findings' and source_review['source_map_sha256'] == manifest['candidate_source_map_sha256']
    analysis_review = read(S/'analysis-review.json')
    assert analysis_review['status'] == 'no_findings' and analysis_review['findings'] == []
    return records


def validate_inputs(manifest, review, go, raw_envelope):
    assert manifest['schema'] == 'deadpan-single-leaf-net-evidence-1'
    assert review['status'] == 'no_findings' and review['findings'] == []
    assert review['script_sha256'] == digest_bytes(raw_envelope['approval/package.py'])
    assert review['manifest_sha256'] == digest_bytes(raw_envelope['approval/approved-package-manifest.json'])
    terminal_records = validate_terminal_records(manifest)
    pins = {
        'package_script_sha256': digest(SCRIPT),
        'approved_manifest_sha256': digest(MANIFEST),
        'packager_review_sha256': digest_bytes(raw_envelope['approval/packager-final-review.json']),
        'source_map_sha256': manifest['candidate_source_map_sha256'],
        'binary_manifest_sha256': manifest['binary_manifest_sha256'],
        'build_terminal_sha256': manifest['terminal_sha256']['build'],
        'measurement_terminal_sha256': manifest['terminal_sha256']['measurement'],
        'media_terminal_sha256': manifest['terminal_sha256']['media'],
        'memory_terminal_sha256': manifest['terminal_sha256']['memory'],
    }
    assert go['status'] == 'approved_for_packaging'
    assert all(go.get(key) == value for key, value in pins.items()), pins
    payloads = manifest['payloads']
    payload_by_dest = {row['destination']: row for row in payloads}
    candidate_map = read(S/'frozen-sources.json')
    base_map = read(S/'qualified-base-sources.json')
    assert digest(S/'frozen-sources.json') == manifest['candidate_source_map_sha256']
    assert digest(S/'qualified-base-sources.json') == manifest['qualified_base_source_map_sha256']
    live_base = {str(p.relative_to(BASE_SOURCE)): digest(p) for p in BASE_SOURCE.rglob('*.rs')}
    assert live_base == base_map
    live_candidate = {str(p.relative_to(S/'source')): digest(p) for p in (S/'source').rglob('*.rs')}
    assert live_candidate == candidate_map
    inputs = read(S/'baseline-inputs.json')
    assert len(inputs) == 6 and digest(S/'baseline-inputs.json') == manifest['input_map_sha256']
    for rel, expected in inputs.items():
        src = S/'source'/rel
        assert digest(src) == expected
        dest = 'inputs/'+rel
        assert dest in payload_by_dest and payload_by_dest[dest]['sha256'] == expected
    differences = sorted(path for path in set(candidate_map) | set(base_map)
                         if candidate_map.get(path) != base_map.get(path))
    declared = {row['path']: row for row in manifest['source_changes']}
    assert sorted(declared) == differences
    for path in differences:
        row = declared[path]
        assert row['before_sha256'] == base_map.get(path)
        assert row['after_sha256'] == candidate_map.get(path)
        assert row['after_destination'] in payload_by_dest
        assert payload_by_dest[row['after_destination']]['sha256'] == candidate_map[path]
        if base_map.get(path) is None:
            assert row['before_destination'] is None
        else:
            assert row['before_destination'] in payload_by_dest
            assert payload_by_dest[row['before_destination']]['sha256'] == base_map[path]
    initial_map = read(INITIAL/'frozen-sources.json')
    initial_differences = sorted(path for path in set(initial_map) | set(base_map)
                                 if initial_map.get(path) != base_map.get(path))
    initial_rows = {row['path']: row for row in manifest['initial_attempt_source_snapshots']}
    assert sorted(initial_rows) == [path for path in initial_differences if path in initial_map]
    for path, row in initial_rows.items():
        assert row['sha256'] == initial_map[path]
        dest = row['destination']
        assert dest in payload_by_dest and payload_by_dest[dest]['sha256'] == initial_map[path]
        source = INITIAL/'source'/path
        assert digest(source) == initial_map[path]
    fixtures = manifest['fixtures']
    payload_dests = [row['destination'] for row in payloads]
    fixture_dests = [row['archive_destination'] for row in fixtures]
    reserved = set(ENVELOPE) | {'package-manifest.json', 'inventory.json'}
    assert len(set(payload_dests)) == len(payload_dests)
    assert len(set(fixture_dests)) == len(fixture_dests)
    assert len({row['name'] for row in fixtures}) == len(fixtures)
    assert not (set(payload_dests) & set(fixture_dests))
    assert not (set(payload_dests) | set(fixture_dests)) & reserved
    assert not reserved & (set(payload_dests) | set(fixture_dests))
    for rel in payload_dests + fixture_dests:
        safe_relative(rel)
    all_destinations = payload_dests + fixture_dests + list(reserved)
    parts = [PurePosixPath(path).parts for path in all_destinations]
    for i, left in enumerate(parts):
        for right in parts[i + 1:]:
            assert not (len(left) < len(right) and right[:len(left)] == left), (left, right)
            assert not (len(right) < len(left) and left[:len(right)] == right), (left, right)
    for row in payloads:
        safe_relative(row['destination'])
        verify_payload(row)
    return payloads


def inventory(root):
    result = []
    total_bytes = 0
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise AssertionError(('staged symlink refused', path))
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise AssertionError(('staged special file refused', path))
        total_bytes += path.stat().st_size
        result.append({'path': path.relative_to(root).as_posix(),
                       'bytes': path.stat().st_size, 'sha256': digest(path)})
    return {'files': result, 'total_files': len(result), 'total_bytes': total_bytes}


def main():
    assert not DEST.exists(), ('refuse existing evidence destination', DEST)
    envelope = {}
    for rel, src in ENVELOPE.items():
        envelope[rel] = verify_payload({
            'source': str(src), 'bytes': src.stat().st_size, 'sha256': digest(src)})
    manifest = json.loads(envelope['approval/approved-package-manifest.json'])
    review = json.loads(envelope['approval/packager-final-review.json'])
    go = json.loads(envelope['approval/PACKAGING-GO.json'])
    payloads = validate_inputs(manifest, review, go, envelope)
    fixtures = manifest['fixtures']
    assert fixtures, 'approved fixture ledger must be nonempty'
    before = {item['name']: verify_fixture(item, payloads) for item in fixtures}
    parent = DEST.parent
    parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.single-leaf-net-stage-', dir=parent))
    try:
        copied = [add_payload(stage, item) for item in payloads]
        archived = [add_fixture(stage, item, payloads) for item in fixtures]
        for rel, data in envelope.items():
            out = destination_path(stage, rel)
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(data)
        meta = {'schema': 'deadpan-single-leaf-net-package-1',
                'manifest_sha256': digest(MANIFEST),
                'source_map_sha256': manifest['candidate_source_map_sha256'],
                'candidate_app_sha256': manifest['candidate_app_sha256'],
                'terminal_sha256': manifest['terminal_sha256'],
                'copied_payloads': copied, 'fixtures': archived}
        (stage / 'package-manifest.json').write_text(json.dumps(meta, indent=2, sort_keys=True) + '\n')
        # Recheck approved inputs and all fixture snapshots immediately before publication.
        assert digest(MANIFEST) == go['approved_manifest_sha256']
        assert digest(SCRIPT) == go['package_script_sha256']
        for item in payloads:
            verify_payload(item)
        for item in fixtures:
            assert verify_fixture(item, payloads) == before[item['name']]
        for rel, src in ENVELOPE.items():
            assert digest(src) == digest_bytes(envelope[rel]), ('approval envelope changed', rel)
        listing = inventory(stage)
        (stage / 'inventory.json').write_text(json.dumps(listing, indent=2, sort_keys=True) + '\n')
        final_inventory = inventory(stage)
        reread = read(stage / 'inventory.json')
        # Inventory excludes itself by design; every other stored file is independently re-read.
        rows = {r['path']: r for r in reread['files']}
        actual = {r['path']: r for r in final_inventory['files'] if r['path'] != 'inventory.json'}
        assert rows == actual
        assert not DEST.exists()
        libc = ctypes.CDLL(None, use_errno=True)
        renameatx_np = libc.renameatx_np
        renameatx_np.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        renameatx_np.restype = ctypes.c_int
        result = renameatx_np(-2, os.fsencode(stage), -2, os.fsencode(DEST), 0x00000004)
        if result != 0:
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code), str(DEST))
    except BaseException:
        shutil.rmtree(stage, ignore_errors=True)
        raise
    inv = read(DEST / 'inventory.json')
    print(json.dumps({'destination': str(DEST), 'files': inv['total_files'],
                      'bytes': inv['total_bytes'], 'inventory_sha256': digest(DEST/'inventory.json'),
                      'fixtures': len(read(MANIFEST)['fixtures'])}))


if __name__ == '__main__':
    main()
