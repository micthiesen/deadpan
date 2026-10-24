"""Independent read-only verification of the approved wire prototype evidence copy."""
from pathlib import Path, PurePosixPath
import gzip
import hashlib
import io
import json
import stat
import tarfile

S = Path('/tmp/deadpan-compound-wire-reuse-prototype-2026-10-24')
D = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-24-compound-wire-reuse-prototype')
RECEIPT = S / 'independent-verification-receipt.json'
SCRIPT_SHA = '3d313cb6cc85a4447332b520cfec20c8d5ac3e03cd5a7a1c07fea2d2836f1702'
MANIFEST_SHA = '0a7d5e90c4827da77c20f2d275b060e45ed84d8eb25c14d0d91f4a0af6bf3820'
INVENTORY_SHA = '338756b4d89157f1e72270c6ca0bd3269240c7ef204a2db2453eb2fcce68e2e4'
SIDECARS = {
    'project.sqlite-shm': {'bytes': 32768, 'sha256': 'fd4c9fda9cd3f9ae7c962b0ddf37232294d55580e1aa165aa06129b8549389eb'},
    'project.sqlite-wal': {'bytes': 0, 'sha256': 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'},
}


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def json_read(path):
    return json.loads(Path(path).read_text())


def safe_name(text):
    p = PurePosixPath(text)
    assert text and not p.is_absolute() and '..' not in p.parts and '\\' not in text
    assert p.as_posix() == text and text != '.'
    return p


def tree(root):
    assert root.is_dir() and not root.is_symlink()
    result = {}
    for p in sorted(root.rglob('*')):
        assert not p.is_symlink(), p
        mode = p.lstat().st_mode
        assert stat.S_ISREG(mode) or stat.S_ISDIR(mode), p
        if stat.S_ISREG(mode):
            name = p.relative_to(root).as_posix()
            safe_name(name)
            result[name] = {'bytes': p.stat().st_size, 'sha256': sha(p)}
    return result


def raw_payload(item):
    p = D / item['destination']
    maximum = item['bytes']
    assert 0 <= maximum <= 250_000_000
    if item.get('encoding', 'identity') == 'gzip':
        assert p.stat().st_size <= 250_000_000
        with gzip.open(p, 'rb') as f:
            raw = f.read(maximum + 1)
    else:
        assert item.get('encoding', 'identity') == 'identity'
        raw = p.read_bytes()
    assert len(raw) == maximum and sha_bytes(raw) == item['sha256'], item['destination']
    source = Path(item['source'])
    assert source.is_file() and not source.is_symlink()
    assert raw == source.read_bytes(), item['source']
    return raw


def main():
    assert not RECEIPT.exists()
    inventory_bytes = (D / 'inventory.json').read_bytes()
    assert sha_bytes(inventory_bytes) == INVENTORY_SHA
    initial = tree(D)
    inv = json.loads(inventory_bytes)
    rows = inv['files']
    declared = {row['path']: {'bytes': row['bytes'], 'sha256': row['sha256']} for row in rows}
    assert len(declared) == len(rows)
    assert declared == {p: v for p, v in initial.items() if p != 'inventory.json'}
    assert inv['total_files'] == len(declared) == 359
    assert inv['total_bytes'] == sum(row['bytes'] for row in declared.values()) == 15_332_794
    assert len(initial) == 360

    envelope = {
        'approval/approved-package-manifest.json': S / 'approved-package-manifest.json',
        'approval/packager-final-review.json': S / 'packager-final-review.json',
        'approval/PACKAGING-GO.json': S / 'PACKAGING-GO.json',
        'approval/package.py': S / 'packaging-draft/package.py',
        'approval/static-packaging-plan.md': S / 'packaging-draft/static-packaging-plan.md',
    }
    for name, source in envelope.items():
        assert (D / name).read_bytes() == source.read_bytes(), name
    assert sha(D / 'approval/package.py') == SCRIPT_SHA
    assert sha(D / 'approval/approved-package-manifest.json') == MANIFEST_SHA
    manifest = json_read(D / 'approval/approved-package-manifest.json')
    review = json_read(D / 'approval/packager-final-review.json')
    go = json_read(D / 'approval/PACKAGING-GO.json')
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['script_sha256'] == SCRIPT_SHA and review['manifest_sha256'] == MANIFEST_SHA
    assert go['status'] == 'approved_for_packaging'
    pins = {
        'package_script_sha256': SCRIPT_SHA,
        'manifest_sha256': MANIFEST_SHA,
        'packager_review_sha256': sha(D / 'approval/packager-final-review.json'),
        'candidate_source_map_sha256': manifest['candidate_source_map_sha256'],
        'binary_manifest_sha256': manifest['binary_manifest_sha256'],
        'analysis_sha256': manifest['analysis_sha256'],
        'analysis_review_sha256': manifest['analysis_review_sha256'],
        **{name + '_terminal_sha256': value for name, value in manifest['terminal_sha256'].items()},
    }
    assert all(go[key] == value for key, value in pins.items())
    assert manifest['static_packaging_plan_sha256'] == sha(D / 'approval/static-packaging-plan.md')

    payloads = manifest['payloads']
    fixtures = manifest['fixtures']
    assert len(payloads) == 335 and len(fixtures) == 18
    by = {item['destination']: item for item in payloads}
    assert len(by) == len(payloads)
    expected_paths = set(by) | {item['archive_destination'] for item in fixtures} | set(envelope) | {'package-manifest.json', 'inventory.json'}
    assert set(initial) == expected_paths
    normalized = [safe_name(name).parts for name in expected_paths]
    assert all(not any(len(b) > len(a) and b[:len(a)] == a for b in normalized) for a in normalized)
    decoded = {}
    for item in payloads:
        assert not set(Path(item['source']).resolve().parts) & {'bin', 'test-bin', '.git', 'target'}
        decoded[item['destination']] = raw_payload(item)
        if item['category'] == 'source-text':
            assert item['destination'].endswith('.rs.txt') and item['encoding'] == 'identity'

    package = json_read(D / 'package-manifest.json')
    assert package['approved_manifest_sha256'] == MANIFEST_SHA
    copied = {item['path']: item for item in package['payloads']}
    assert len(copied) == len(payloads) and set(copied) == set(by)
    for name, item in by.items():
        row = copied[name]
        assert row['source_bytes'] == item['bytes'] and row['source_sha256'] == item['sha256']
        assert row['stored_bytes'] == initial[name]['bytes'] and row['stored_sha256'] == initial[name]['sha256']
        assert row['category'] == item['category'] and row['encoding'] == item['encoding']

    members_count = 0
    fixture_bytes = 0
    archives = {item['name']: item for item in package['fixtures']}
    assert len(archives) == len(fixtures)
    for item in fixtures:
        assert item['allowed_sidecars'] == SIDECARS
        before = json.loads(decoded[item['before_map']['destination']])
        after = json.loads(decoded[item['after_map']['destination']])
        actual = tree(Path(item['root']))
        authored = lambda value: {name: meta for name, meta in value.items() if name not in SIDECARS}
        assert authored(before) == authored(after) == authored(actual)
        assert set(actual) - set(before) == set(SIDECARS)
        for sidecar, pin in SIDECARS.items():
            assert after[sidecar] == actual[sidecar] == pin
        files = authored(actual)
        assert set(files) == {'.writer.lock', 'manifest.json', 'project.sqlite'}
        ar = D / item['archive_destination']
        with gzip.open(ar, 'rb') as f:
            tar_bytes = f.read(110_000_001)
        assert len(tar_bytes) <= 110_000_000
        reproduced = {}
        with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode='r:') as t:
            for member in t.getmembers():
                name = safe_name(member.name)
                assert member.isfile() and member.type in (tarfile.REGTYPE, tarfile.AREGTYPE)
                assert name.parts[0] == item['name'] and len(name.parts) > 1
                rel = PurePosixPath(*name.parts[1:]).as_posix()
                assert rel in files and rel not in reproduced and rel not in SIDECARS
                assert member.size == files[rel]['bytes'] <= 100_000_000
                content = t.extractfile(member).read()
                assert len(content) == member.size and sha_bytes(content) == files[rel]['sha256']
                assert content == (Path(item['root']) / rel).read_bytes()
                reproduced[rel] = {'bytes': len(content), 'sha256': sha_bytes(content)}
        assert reproduced == files and len(reproduced) == 3
        members_count += len(reproduced)
        subtotal = sum(row['bytes'] for row in files.values())
        assert subtotal <= 100_000_000
        fixture_bytes += subtotal
        recorded = archives[item['name']]
        assert recorded['archive'] == item['archive_destination'] and recorded['archive_sha256'] == sha(ar)
        assert recorded['archive_bytes'] == ar.stat().st_size
        assert recorded['authored_files'] == 3 and recorded['authored_bytes'] == subtotal
        assert recorded['omitted_runtime_sidecars'] == SIDECARS
    assert members_count == 54

    def decoded_json(name):
        return json.loads(decoded[name])

    candidate = decoded_json('records/frozen-sources.json.gz')
    baseline = decoded_json('records/baseline-sources.json.gz')
    qualified = decoded_json('records/qualified-base-sources.json.gz')
    assert (len(candidate), len(baseline), len(qualified)) == (2054, 2054, 2053)
    for key, old in [('source_changes', baseline), ('qualified_overlay', qualified)]:
        expected = {name for name in set(old) | set(candidate) if old.get(name) != candidate.get(name)}
        assert len(manifest[key]) == len(expected) and {row['path'] for row in manifest[key]} == expected
        for row in manifest[key]:
            assert row['before_sha256'] == old.get(row['path'])
            assert row['after_sha256'] == candidate[row['path']]
            assert sha_bytes(decoded[row['after_destination']]) == candidate[row['path']]
            if row['before_destination']:
                assert sha_bytes(decoded[row['before_destination']]) == old[row['path']]
            else:
                assert row['path'] not in old
    assert len(manifest['source_changes']) == 2 and len(manifest['qualified_overlay']) == 14
    assert sum(row['before_destination'] is None for row in manifest['qualified_overlay']) == 1
    inputs = decoded_json('records/baseline-inputs.json')
    assert len(inputs) == 6
    for name, expected in inputs.items():
        assert sha_bytes(decoded['inputs/' + name]) == expected

    media = decoded_json('records/media-evidence-inventory.json')
    media_payloads = [item for item in payloads if item['category'] == 'media-evidence']
    assert len(media_payloads) == 35
    assert {item['destination'][6:]: {'bytes': item['bytes'], 'sha256': item['sha256']} for item in media_payloads} == media['files']
    assert tree(Path(media['root'])) == media['files']
    terminal = decoded_json('records/media-terminal.json')
    movies = []
    for row in manifest['media_exports']:
        name = 'media/exports-' + row['leg'] + '/one-leaf-' + row['leg'] + '-media.mp4'
        assert sha(D / name) == row['sha256']
        actual = next(value for value in terminal['evidence']['exports'] if value['leg'] == row['leg'])
        assert actual['movie_sha256'] == row['sha256']
        verify = decoded_json('media/one-leaf-' + row['leg'] + '-verification.json')
        assert verify['passed'] and not verify['failures']
        assert len(verify['pictures']) == 45 and all(p['passed'] for p in verify['pictures'])
        assert verify['summary']['pictures_checked'] == 45 and verify['summary']['pictures_failed'] == 0
        assert len(verify['audio']) == 1 and verify['audio'][0]['passed']
        assert verify['audio'][0]['project_samples'] == [0, 72072]
        assert verify['audio'][0]['offset_status'] == 'verified_zero' and verify['audio'][0]['measured_offset_samples'] == 0
        movies.append({'leg': row['leg'], 'sha256': row['sha256'], 'bytes': (D / name).stat().st_size})

    for record, value in [('build', 9), ('measurement', 18), ('media', 4)]:
        obj = decoded_json('records/' + record + '-terminal.json')
        assert obj['status'] == 'complete'
        assert sha_bytes(decoded['records/' + record + '-terminal.json']) == manifest['terminal_sha256'][record]
    for name in ('source-review.json', 'analysis-review.json'):
        obj = decoded_json('records/' + name)
        assert obj['status'] == 'no_findings' and not obj['findings']
    assert decoded_json('records/analysis-review.json')['analyze_py_sha256'] == sha_bytes(decoded['records/analyze.py'])
    after_tree = tree(D)
    assert after_tree == initial and (D / 'inventory.json').read_bytes() == inventory_bytes
    result = {
        'status': 'passed',
        'scope': 'Independent offline evidence byte, decoding and metadata verifier. No app/build/test or media decoding executed.',
        'script_sha256': sha(__file__),
        'destination': str(D),
        'inventory_sha256_before': sha_bytes(inventory_bytes),
        'inventory_sha256_after': sha(D / 'inventory.json'),
        'primary_unchanged': True,
        'all_files_including_inventory': len(initial),
        'inventoried_files_excluding_inventory': len(declared),
        'inventoried_bytes_excluding_inventory': inv['total_bytes'],
        'total_bytes_including_inventory': sum(v['bytes'] for v in initial.values()),
        'approved_payloads': len(payloads),
        'decoded_payload_source_bytes': sum(v['bytes'] for v in payloads),
        'gzip_payloads': sum(v.get('encoding') == 'gzip' for v in payloads),
        'fixture_archives': len(fixtures),
        'fixture_authored_members': members_count,
        'fixture_authored_bytes': fixture_bytes,
        'runtime_sidecars_checked_and_omitted_from_fixture_archives': SIDECARS,
        'media_files': len(media_payloads),
        'exports': movies,
        'source_snapshot_changes_vs_matched_profile': 2,
        'source_snapshot_overlay_vs_qualified': {'existing': 13, 'added': 1},
        'pinned_inputs': 6,
        'approval_files': {name: sha(D / name) for name in envelope},
        'all_raw_payload_sources_match': True,
        'no_new_supplement_or_main_writes': True,
    }
    with RECEIPT.open('x') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
