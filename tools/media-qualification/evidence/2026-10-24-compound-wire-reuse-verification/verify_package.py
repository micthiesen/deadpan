"""Independent read-only verifier for the published production wire-count package."""
from pathlib import Path, PurePosixPath
import gzip
import hashlib
import io
import json
import stat
import tarfile
import unicodedata

S = Path('/tmp/deadpan-compound-wire-reuse-production-2026-10-24')
DRAFT = S / 'packaging-draft'
D = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-24-compound-wire-reuse')
OUT = S / 'independent-verification-receipt.json'
EXPECTED = {
    'inventory': 'eb326525ca16652869ae889695c155bb5848e611bc2ca445ed5bbedfd100684c',
    'manifest': '74266fa01a8980ef552754ebb7502a2f816f732f4597426fd1b9bf7a6be34974',
    'script': 'b20d849e87b77e174c86302edec6462827f867b3ed02cca8bf346606304b53bf',
    'review': 'a76837722f9c56ad9ffba7c5d7428d2d63524e85ea3e27a88cf66893e74f4d91',
    'go': '032837737c377775e9185f09a96f783e27e1c3f58378267ad92358bd1bfc16ee',
}
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def safe(text):
    p = PurePosixPath(text)
    assert text and text != '.' and not p.is_absolute() and '..' not in p.parts
    assert '\\' not in text and p.as_posix() == text
    assert unicodedata.normalize('NFC', text) == text
    return p


def snapshot(root):
    root = Path(root)
    assert root.is_dir() and not root.is_symlink()
    rows = {}
    for p in sorted(root.rglob('*')):
        assert not p.is_symlink(), p
        mode = p.lstat().st_mode
        assert stat.S_ISREG(mode) or stat.S_ISDIR(mode), p
        if stat.S_ISREG(mode):
            name = p.relative_to(root).as_posix()
            safe(name)
            rows[name] = {'bytes': p.stat().st_size, 'sha256': digest(p)}
    return rows


def main():
    assert not OUT.exists()
    before_inventory = (D / 'inventory.json').read_bytes()
    assert digest_bytes(before_inventory) == EXPECTED['inventory']
    initial = snapshot(D)
    listing = json.loads(before_inventory)
    rows = {x['path']: {'bytes': x['bytes'], 'sha256': x['sha256']} for x in listing['files']}
    assert len(rows) == len(listing['files']) == listing['total_files'] == 253
    assert rows == {name: row for name, row in initial.items() if name != 'inventory.json'}
    assert sum(x['bytes'] for x in rows.values()) == listing['total_bytes'] == 10_713_173
    assert len(initial) == 254

    env = {
        'approval/approved-package-manifest.json': DRAFT / 'approved-package-manifest.json',
        'approval/packager-final-review.json': DRAFT / 'packager-final-review.json',
        'approval/PACKAGING-GO.json': DRAFT / 'PACKAGING-GO.json',
        'approval/static-packaging-plan.md': DRAFT / 'static-packaging-plan.md',
        'approval/package.py': DRAFT / 'package.py',
    }
    for name, source in env.items():
        assert (D / name).read_bytes() == source.read_bytes()
    assert digest(D / 'approval/approved-package-manifest.json') == EXPECTED['manifest']
    assert digest(D / 'approval/package.py') == EXPECTED['script']
    assert digest(D / 'approval/packager-final-review.json') == EXPECTED['review']
    assert digest(D / 'approval/PACKAGING-GO.json') == EXPECTED['go']
    manifest = read(D / 'approval/approved-package-manifest.json')
    review = read(D / 'approval/packager-final-review.json')
    go = read(D / 'approval/PACKAGING-GO.json')
    assert review['status'] == 'no_findings' and review['findings'] == []
    assert review['script_sha256'] == EXPECTED['script']
    assert review['manifest_sha256'] == EXPECTED['manifest']
    assert go['status'] == 'approved_for_packaging'
    go_pins = {
        'package_script_sha256': EXPECTED['script'],
        'approved_manifest_sha256': EXPECTED['manifest'],
        'packager_review_sha256': EXPECTED['review'],
        'static_packaging_plan_sha256': manifest['static_packaging_plan_sha256'],
        'source_map_sha256': manifest['candidate_source_map_sha256'],
        'binary_manifest_sha256': manifest['binary_manifest_sha256'],
        'qualified_helper_manifest_sha256': manifest['qualified_helper_manifest_sha256'],
        'source_review_sha256': manifest['source_review_sha256'],
        'analysis_review_sha256': manifest['analysis_review_sha256'],
        'input_map_sha256': manifest['input_map_sha256'],
        'candidate_app_sha256': manifest['candidate_app_sha256'],
        'control_app_sha256': manifest['control_app_sha256'],
        **{name + '_terminal_sha256': value for name, value in manifest['terminal_sha256'].items()},
    }
    assert all(go[key] == value for key, value in go_pins.items())
    assert digest(D / 'approval/static-packaging-plan.md') == manifest['static_packaging_plan_sha256']

    payloads = manifest['payloads']
    fixtures = manifest['fixtures']
    assert len(payloads) == 237 and len(fixtures) == 10
    by = {x['destination']: x for x in payloads}
    assert len(by) == len(payloads)
    expected_paths = set(by) | {x['archive_destination'] for x in fixtures} | set(env) | {'package-manifest.json', 'inventory.json'}
    assert set(initial) == expected_paths
    parts = [tuple(c.casefold() for c in safe(name).parts) for name in expected_paths]
    assert len(set(parts)) == len(parts)
    assert all(not any(len(b) > len(a) and b[:len(a)] == a for b in parts) for a in parts)
    decoded = {}
    for dest, item in by.items():
        assert type(item['bytes']) is int and 0 <= item['bytes'] <= 25_000_000
        source = Path(item['source'])
        assert source.is_file() and not source.is_symlink() and stat.S_ISREG(source.lstat().st_mode)
        assert source.stat().st_size == item['bytes']
        path = D / dest
        # Encoding is explicitly selected from the ledger, never guessed from suffix.
        if item['encoding'] == 'gzip':
            with gzip.open(path, 'rb') as f:
                raw = f.read(item['bytes'] + 1)
        else:
            assert item['encoding'] == 'identity'
            raw = path.read_bytes()
        assert len(raw) == item['bytes'] and digest_bytes(raw) == item['sha256']
        assert raw == source.read_bytes()
        decoded[dest] = raw
    assert sum(x['encoding'] == 'gzip' for x in payloads) == 31

    package = read(D / 'package-manifest.json')
    assert package['manifest_sha256'] == EXPECTED['manifest']
    assert package['source_map_sha256'] == manifest['candidate_source_map_sha256']
    assert package['candidate_app_sha256'] == manifest['candidate_app_sha256']
    assert package['static_packaging_plan_sha256'] == manifest['static_packaging_plan_sha256']
    assert package['terminal_sha256'] == manifest['terminal_sha256']
    saved = {x['path']: x for x in package['payloads']}
    assert len(saved) == len(payloads) and set(saved) == set(by)
    for name, item in by.items():
        rec = saved[name]
        assert rec['source_bytes'] == item['bytes'] and rec['source_sha256'] == item['sha256']
        assert rec['stored_bytes'] == initial[name]['bytes'] and rec['stored_sha256'] == initial[name]['sha256']
        assert rec['encoding'] == item['encoding']

    fixtures_by_name = {x['name']: x for x in package['fixtures']}
    expected_fixtures = {f'rotation-{i}-{kind}' for i in range(1, 6) for kind in ('control', 'candidate')}
    assert {x['name'] for x in fixtures} == set(fixtures_by_name) == expected_fixtures
    fixture_bytes = 0
    member_count = 0
    for item in fixtures:
        assert Path(item['root']) == S / (item['name'] + '.deadpan')
        maps = {}
        for key in ('before_map', 'after_map'):
            ref = item[key]
            assert all(by[ref['destination']][field] == ref[field] for field in ('source', 'bytes', 'sha256'))
            maps[key] = json.loads(decoded[ref['destination']])
        before, after = maps['before_map'], maps['after_map']
        actual = snapshot(item['root'])
        authored = lambda value: {name: meta for name, meta in value.items() if name not in SIDECARS}
        files = authored(actual)
        assert authored(before) == authored(after) == files
        assert set(item['allowed_sidecars']) == SIDECARS
        assert set(actual) - set(before) <= SIDECARS
        assert set(files) == {'.writer.lock', 'manifest.json', 'project.sqlite'}
        for name, pin in item['allowed_sidecars'].items():
            assert pin['after'] == after[name] == actual[name]
            assert pin['after']['bytes'] == (32768 if name.endswith('-shm') else 0)
            if name in before:
                assert pin['before'] == before[name]
            else:
                assert 'before' not in pin
        archive = D / item['archive_destination']
        with gzip.open(archive, 'rb') as f:
            tar_raw = f.read(25_000_001)
        assert len(tar_raw) <= 25_000_000
        found = {}
        with tarfile.open(fileobj=io.BytesIO(tar_raw), mode='r:') as tar:
            for member in tar.getmembers():
                path = safe(member.name)
                assert member.isfile() and member.type in (tarfile.REGTYPE, tarfile.AREGTYPE)
                assert path.parts[0] == item['name'] and len(path.parts) > 1
                rel = PurePosixPath(*path.parts[1:]).as_posix()
                assert rel in files and rel not in found and rel not in SIDECARS
                assert member.size == files[rel]['bytes'] <= 25_000_000
                raw = tar.extractfile(member).read()
                assert len(raw) == member.size and digest_bytes(raw) == files[rel]['sha256']
                assert raw == (Path(item['root']) / rel).read_bytes()
                found[rel] = {'bytes': len(raw), 'sha256': digest_bytes(raw)}
        assert found == files and len(found) == 3
        subtotal = sum(x['bytes'] for x in files.values())
        assert subtotal <= 25_000_000
        fixture_bytes += subtotal
        member_count += len(found)
        record = fixtures_by_name[item['name']]
        assert record['archive'] == item['archive_destination']
        assert record['sha256'] == digest(archive)
        assert record['authored_files'] == 3 and record['authored_bytes'] == subtotal
        assert record['separate_sqlite_sidecars'] == item['allowed_sidecars']
    assert fixture_bytes == 44_892_780 and member_count == 30

    j = lambda name: json.loads(decoded[name])
    candidate = j('production/frozen-sources.json')
    baseline = j('production/baseline-sources.json')
    assert len(candidate) == len(baseline) == 2053 and set(candidate) == set(baseline)
    changes = {name for name in candidate if candidate[name] != baseline[name]}
    assert changes == {x['path'] for x in manifest['source_changes']}
    assert changes == {'crates/deadpan-core/src/compound.rs', 'crates/deadpan-core/src/one_leaf_replay.rs'}
    for item in manifest['source_changes']:
        assert item['before_sha256'] == baseline[item['path']]
        assert item['after_sha256'] == candidate[item['path']]
        assert digest_bytes(decoded[item['before_destination']]) == baseline[item['path']]
        assert digest_bytes(decoded[item['after_destination']]) == candidate[item['path']]
        assert by[item['before_destination']]['encoding'] == by[item['after_destination']]['encoding'] == 'identity'
    inputs = j('production/baseline-inputs.json')
    assert len(inputs) == 6
    for name, expected in inputs.items():
        assert digest_bytes(decoded['inputs/' + name]) == expected
    before_promotion = j('production/main-before-promotion.json')
    assert before_promotion['rust_sources'] == baseline
    promotion = j('production/main-promotion-source-check.json')
    assert promotion['main_rust_sources'] == 2053 and promotion['all_bytes_equal_qualified_candidate'] is True
    assert promotion['candidate_source_map_sha256'] == manifest['candidate_source_map_sha256']
    assert promotion['inputs'] == inputs and set(promotion['only_owned_paths_changed']) == changes

    binary_manifest = j('production/binaries.json')
    assert digest_bytes(decoded['production/binaries.json']) == manifest['binary_manifest_sha256']
    assert binary_manifest['deadpan-app']['sha256'] == manifest['candidate_app_sha256']
    for row in binary_manifest.values():
        assert digest(row['executed']) == row['sha256']
    baseline_record = j('production/baseline.json')
    assert digest(baseline_record['control']['path']) == manifest['control_app_sha256']
    for name, field in [('source-review.json', 'source_review_sha256'), ('analysis-review.json', 'analysis_review_sha256')]:
        data = decoded['production/' + name]
        rec = json.loads(data)
        assert digest_bytes(data) == manifest[field] and rec['status'] == 'no_findings' and not rec['findings']
    for name, expected in manifest['terminal_sha256'].items():
        data = decoded['production/' + name + '-terminal.json']
        assert digest_bytes(data) == expected and json.loads(data)['status'] == 'complete'

    media = j('production/media-evidence-inventory.json')
    assert len(media['files']) == len(manifest['media_files']) == 35
    assert snapshot(media['root']) == media['files']
    for item in manifest['media_files']:
        raw = decoded[item['destination']]
        meta = media['files'][item['relative_path']]
        assert len(raw) == item['bytes'] == meta['bytes']
        assert digest_bytes(raw) == item['sha256'] == meta['sha256']
    media_terminal = j('production/media-terminal.json')
    movies = []
    for row in media_terminal['evidence']['exports']:
        rel = Path(row['movie']).resolve().relative_to(Path(media['root']).resolve()).as_posix()
        movie = decoded['media-evidence/' + rel]
        assert digest_bytes(movie) == row['movie_sha256']
        v = j('media-evidence/one-leaf-' + row['leg'] + '-verification.json')
        assert v['passed'] and v['failures'] == []
        assert len(v['pictures']) == 45 and all(p['passed'] for p in v['pictures'])
        assert v['summary']['pictures_checked'] == 45 and v['summary']['pictures_failed'] == 0
        assert len(v['audio']) == 1 and v['audio'][0]['passed']
        assert v['audio'][0]['project_samples'] == [0, 72072]
        assert v['audio'][0]['offset_status'] == 'verified_zero' and v['audio'][0]['measured_offset_samples'] == 0
        movies.append({'leg': row['leg'], 'sha256': digest_bytes(movie), 'bytes': len(movie)})
    assert len(movies) == 2 and {x['leg'] for x in movies} == {'cached', 'uncached'}

    after = snapshot(D)
    assert after == initial and (D / 'inventory.json').read_bytes() == before_inventory
    result = {
        'status': 'passed',
        'scope': 'Independent offline package byte and metadata verification. No app/build/test/media decoding or Git/Main writes.',
        'verifier_sha256': digest(__file__),
        'destination': str(D),
        'inventory_sha256_before': digest_bytes(before_inventory),
        'inventory_sha256_after': digest(D / 'inventory.json'),
        'primary_tree_unchanged': True,
        'files_including_inventory': len(initial),
        'inventoried_payloads': listing['total_files'],
        'inventoried_bytes': listing['total_bytes'],
        'all_primary_bytes_including_inventory': sum(x['bytes'] for x in initial.values()),
        'approved_decoded_payloads': len(payloads),
        'decoded_source_bytes': sum(x['bytes'] for x in payloads),
        'gzip_payloads_decoded_by_ledger': 31,
        'fixture_archives': 10,
        'fixture_authored_members': member_count,
        'fixture_authored_bytes': fixture_bytes,
        'sqlite_runtime_sidecars_checked_but_excluded_from_fixture_tars': sorted(SIDECARS),
        'media_files': 35,
        'movies': movies,
        'source_changes': sorted(changes),
        'unchanged_source_paths': 2051,
        'four_plain_source_snapshots_checked': True,
        'six_inputs_checked': True,
        'binary_and_review_bindings_checked': True,
        'main_promotion_receipts_checked_without_reading_mutable_Main': True,
        'approval_hashes': {name: digest(D / name) for name in env},
        'all_raw_payloads_equal_approved_current_sources': True,
        'no_supplement_directory_created': True,
    }
    with OUT.open('x') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
