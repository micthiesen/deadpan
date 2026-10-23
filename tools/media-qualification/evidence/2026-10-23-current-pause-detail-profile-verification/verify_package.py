from pathlib import Path, PurePosixPath
import gzip
import hashlib
import json
import tarfile

S = Path('/tmp/deadpan-current-pause-detail-profile-2026-10-23')
ROOT = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-current-pause-detail-profile')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


producer = json.loads((S / 'packaging.stdout').read_text())
assert sha(ROOT / 'inventory.json') == producer['inventory_sha256']
inventory = json.loads((ROOT / 'inventory.json').read_text())
rows = inventory['files']
by_path = {row['path']: row for row in rows}
assert len(by_path) == len(rows) == producer['payload_files'] == 195
actual = {str(p.relative_to(ROOT)) for p in ROOT.rglob('*') if p.is_file()}
assert actual == set(by_path) | {'inventory.json', 'PACKAGE-SUMMARY.md'}
for rel, row in by_path.items():
    name = PurePosixPath(rel)
    assert not name.is_absolute() and '..' not in name.parts
    assert not (ROOT / rel).is_symlink()
    assert (ROOT / rel).stat().st_size == row['bytes'] and sha(ROOT / rel) == row['sha256'], rel
assert sum(row['bytes'] for row in rows) == inventory['total_bytes'] == producer['payload_bytes'] == 4749341
compression_rows = json.loads((ROOT / 'compression-verification.json').read_text())['verified_gzip_payloads']
compression_map = {row['stored']: row for row in compression_rows}
assert len(compression_map) == len(compression_rows)
compressed = archives = members = 0
decoded_regular = set()
excluded = {'project.sqlite-shm', 'project.sqlite-wal'}
for path in sorted(ROOT.rglob('*.gz')):
    data = gzip.decompress(path.read_bytes())
    compressed += 1
    if not path.name.endswith('.tar.gz'):
        rel = str(path.relative_to(ROOT))
        meta = compression_map[rel]
        assert len(data) == meta['decoded_bytes']
        assert hashlib.sha256(data).hexdigest() == meta['decoded_sha256'] == sha(Path(meta['source']))
        assert sha(path) == meta['stored_sha256']
        decoded_regular.add(rel)
        continue
    archives += 1
    stem = path.name.removesuffix('.tar.gz')
    after = json.loads((S / (stem + '-input-after.json')).read_text())
    before = json.loads((S / (stem + '-input-before.json')).read_text())
    assert after.get('project.sqlite-wal', {}).get('bytes', 0) == 0
    expected = {k: v for k, v in after.items() if k not in excluded}
    assert expected == {k: v for k, v in before.items() if k not in excluded}
    with tarfile.open(path, 'r:gz') as archive:
        seen = {}
        for member in archive.getmembers():
            name = PurePosixPath(member.name)
            assert member.isfile() and not name.is_absolute() and '..' not in name.parts
            rel = str(name)
            assert rel in expected and rel not in seen
            stream = archive.extractfile(member)
            data = stream.read()
            assert len(data) == member.size
            seen[rel] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
            members += 1
        assert seen == expected, path
assert archives == 9 and decoded_regular == set(compression_map)
assert not any(p.name.endswith('.rs') or p.name.endswith('.deadpan') for p in ROOT.rglob('*'))
assert not any('raw-private' in p or 'host-observations-private' in p for p in actual)
pins = json.loads((S / 'driver-pins.json').read_text())
assert sha(ROOT / 'run/host-observations.json') == pins['host-observations.json']
host = json.loads((ROOT / 'run/host-observations.json').read_text())
for name in ['hardware', 'os', 'machine', 'power', 'thermal']:
    assert 'stderr' not in host[name]
assert 'Serial Number:' not in host['hardware']['stdout'] and 'Hardware UUID:' not in host['hardware']['stdout']
result = {'inventory_sha256': sha(ROOT / 'inventory.json'), 'payload_files': len(rows),
          'payload_bytes': sum(row['bytes'] for row in rows), 'all_files': len(actual),
          'gzip_streams_decoded': compressed, 'decoded_payloads_equal_live_source_hashes': len(decoded_regular),
          'safe_fixture_archives': archives, 'archive_regular_members': members,
          'all_payload_sizes_and_hashes_reread': True,
          'all_tar_members_match_authored_after_maps': True,
          'excluded_root_sidecars': sorted(excluded), 'each_wal_verified_empty': True,
          'public_host_matches_pin_and_omits_private_fields': True}
(S / 'root-package-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
