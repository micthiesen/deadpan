from pathlib import Path, PurePosixPath
import gzip, hashlib, json, tarfile

S = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23')
ROOT = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-insert-split-proof-reuse')
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def digest(data):
    return hashlib.sha256(data).hexdigest()
producer = json.loads((S / 'packaging.stdout').read_text())
assert sha(ROOT / 'inventory.json') == producer['inventory_sha256']
inventory = json.loads((ROOT / 'inventory.json').read_text())
rows = inventory['files']
by_path = {r['path']: r for r in rows}
assert len(by_path) == len(rows) == inventory['total_files'] == 238
actual = {str(p.relative_to(ROOT)) for p in ROOT.rglob('*') if p.is_file()}
assert actual == set(by_path) | {'inventory.json'} and len(actual) == producer['files'] == 239
assert not any(p.is_symlink() for p in ROOT.rglob('*'))
for rel, row in by_path.items():
    name = PurePosixPath(rel)
    assert not name.is_absolute() and '..' not in name.parts
    assert (ROOT / rel).stat().st_size == row['bytes'] and sha(ROOT / rel) == row['sha256']
assert sum(r['bytes'] for r in rows) == inventory['total_bytes'] == producer['payload_bytes'] == 4863532
manifest = json.loads((ROOT / 'approval/approved-package-manifest.json').read_text())
assert sha(ROOT / 'approval/approved-package-manifest.json') == '8b200d1ad0045ad2fe96e5ba66f5961a960f2000afc8b718214c5e19cc29705b'
expected = {r['destination']: r for r in manifest['files']}
payloads = {r['path']: r for r in inventory['source_payloads']}
assert set(payloads) == set(expected) and len(expected) == 224
compressed_regular = 0
for rel, row in payloads.items():
    source = expected[rel]
    data = (ROOT / rel).read_bytes()
    assert sha(ROOT / rel) == row['stored_sha256'] and len(data) == row['stored_bytes']
    assert row['encoding'] == source['encoding']
    if row['encoding'] == 'gzip':
        data = gzip.decompress(data)
        compressed_regular += 1
    else:
        assert row['encoding'] == 'identity'
    assert len(data) == row['source_bytes'] == source['bytes']
    assert digest(data) == row['source_sha256'] == source['sha256'] == sha(Path(source['source']))
fixtures = {r['name']: r for r in inventory['fixtures']}
assert len(fixtures) == len(manifest['fixtures']) == 10
members = 0
for item in manifest['fixtures']:
    def read_packaged(ref):
        data = (ROOT / ref['destination']).read_bytes()
        if expected[ref['destination']]['encoding'] == 'gzip':
            data = gzip.decompress(data)
        assert len(data) == ref['bytes'] and digest(data) == ref['sha256']
        return json.loads(data)
    before, after = read_packaged(item['before_map']), read_packaged(item['after_map'])
    before, after = before.get('files', before), after.get('files', after)
    allowed = item.get('allowed_sidecars', {})
    assert set(allowed) <= {'project.sqlite-shm', 'project.sqlite-wal'}
    root = Path(item['root'])
    live = {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
            for p in root.rglob('*') if p.is_file()}
    assert live.get('project.sqlite-wal', {}).get('bytes', 0) == 0
    authored = {k: v for k, v in live.items() if k not in allowed}
    assert authored == {k: v for k, v in before.items() if k not in allowed}
    assert authored == {k: v for k, v in after.items() if k not in allowed}
    path = ROOT / item['archive_destination']
    assert sha(path) == fixtures[item['name']]['archive_sha256']
    with tarfile.open(path, 'r:gz') as tar:
        seen = {}
        for member in tar.getmembers():
            name = PurePosixPath(member.name)
            assert member.isfile() and not name.is_absolute() and '..' not in name.parts
            assert name.parts[0] == item['name']
            rel = str(PurePosixPath(*name.parts[1:]))
            assert rel in authored and rel not in seen
            data = tar.extractfile(member).read()
            assert len(data) == member.size
            seen[rel] = {'bytes': len(data), 'sha256': digest(data)}
            members += 1
        assert seen == authored
assert members == 30
gz_paths = list(ROOT.rglob('*.gz'))
assert len(gz_paths) == compressed_regular + 10
for path in gz_paths:
    gzip.decompress(path.read_bytes())
movies = []
media = json.loads((S / 'media-resume-terminal.json').read_text())
for row in media['evidence']['exports']:
    rel = Path(row['movie']).resolve().relative_to(Path(media['evidence']['root']).resolve())
    path = ROOT / 'media/evidence' / rel
    assert sha(path) == row['movie_sha256'] and path.stat().st_size > 0
    movies.append({'leg': row['leg'], 'path': str(path.relative_to(ROOT)), 'sha256': sha(path)})
assert len(movies) == 2
assert not any(p.is_file() and p.name.endswith('.rs') for p in ROOT.rglob('*'))
assert not any(p.name.endswith('.tar.zst') for p in ROOT.rglob('*'))
result = {'inventory_sha256': sha(ROOT / 'inventory.json'), 'payload_files': len(rows),
          'payload_bytes': inventory['total_bytes'], 'all_files': len(actual),
          'approved_payloads_compared_with_original_bytes': len(payloads),
          'gzip_streams_decoded': len(gz_paths), 'compressed_regular_payloads': compressed_regular,
          'fixture_archives': 10, 'archive_regular_members': members,
          'all_hashes_and_sizes_reread': True, 'all_fixture_maps_match_live_and_packaged_before_after': True,
          'every_wal_empty': True, 'actual_verified_export_files': movies}
with (S / 'root-package-verification.json').open('x') as out:
    out.write(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
