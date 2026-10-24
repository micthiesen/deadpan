"""Bounded evidence copier. Root supplies a reviewed explicit manifest and GO record."""
from pathlib import Path, PurePosixPath
import ctypes
import errno
import gzip
import hashlib
import io
import json
import os
import shutil
import stat
import tarfile
import tempfile

S = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-insert-split-proof-reuse'
MANIFEST = S / 'approved-package-manifest.json'  # Root creates after all producers terminate.
GO = S / 'PACKAGING-GO.json'                     # Must bind all digests below.
REVIEW = S / 'packager-final-review.json'         # Produced after this file is frozen.
SCRIPT = Path(__file__).absolute()
ENVELOPE = {'approval/approved-package-manifest.json': MANIFEST,
            'approval/packager-final-review.json': REVIEW,
            'approval/PACKAGING-GO.json': GO,
            'approval/package_v2.py': SCRIPT}
SIDECARS = {'project.sqlite-shm', 'project.sqlite-wal'}
MAX_FIXTURE_BYTES = 200_000_000


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    return sha_bytes(Path(path).read_bytes())


def read(path):
    return json.loads(Path(path).read_text())


def write_json(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=2, sort_keys=True) + '\n')


def safe_dest(root, name):
    rel = PurePosixPath(name)
    assert not rel.is_absolute() and rel.parts and '..' not in rel.parts and '\\' not in name
    path = root.joinpath(*rel.parts)
    assert path.resolve().is_relative_to(root.resolve())
    return path


def require_hash(path, digest, size=None):
    path = Path(path)
    assert path.is_file() and not path.is_symlink(), path
    data = path.read_bytes()
    assert sha_bytes(data) == digest, path
    if size is not None:
        assert len(data) == size, path
    return data


def copy_payload(stage, item):
    source_path = Path(item['source'])
    assert not source_path.is_symlink(), source_path
    src = source_path.resolve(strict=True)
    data = require_hash(src, item['sha256'], item['bytes'])
    rel = item['destination']
    out = safe_dest(stage, rel)
    out.parent.mkdir(parents=True, exist_ok=True)
    mode = item.get('encoding', 'identity')
    if mode == 'identity':
        out.write_bytes(data)
    elif mode == 'gzip':
        encoded = gzip.compress(data, compresslevel=9, mtime=0)
        assert gzip.decompress(encoded) == data
        out.write_bytes(encoded)
    else:
        raise AssertionError(('unknown payload encoding', mode, rel))
    return {'path': rel, 'source_sha256': item['sha256'], 'source_bytes': len(data),
            'stored_sha256': sha(out), 'stored_bytes': out.stat().st_size, 'encoding': mode}


def fixture_root(descriptor):
    original = Path(descriptor['root'])
    assert not original.is_symlink(), ('fixture root is a symlink', original)
    root = original.resolve(strict=True)
    assert root.is_dir()
    return root


def snapshot(root):
    root = Path(root).resolve(strict=True)
    result = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise AssertionError(('symlink in authored fixture', path))
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise AssertionError(('nonregular authored fixture entry', path, mode))
        rel = path.relative_to(root).as_posix()
        result[rel] = {'bytes': path.stat().st_size, 'sha256': sha(path)}
    return result


def pinned_map(item, field, approved_files):
    ref = item[field]
    src = Path(ref['source'])
    assert not src.is_symlink(), src
    digest, size = ref['sha256'], ref['bytes']
    matches = [row for row in approved_files if row['source'] == str(src)
               and row['sha256'] == digest and row['bytes'] == size
               and row['destination'] == ref['destination']]
    assert len(matches) == 1, (item['name'], field, ref)
    require_hash(src, digest, size)
    return read(src)


def make_fixture_archive(stage, item, approved_files):
    """Verify the authored DB before/after maps, then retain safe regular files only."""
    root = fixture_root(item)
    before = pinned_map(item, 'before_map', approved_files)
    after = pinned_map(item, 'after_map', approved_files)
    before = before.get('files', before)
    after = after.get('files', after)
    allowed = item.get('allowed_sidecars', {})
    assert set(allowed) <= SIDECARS
    actual = snapshot(root)
    for sidecar, expected in allowed.items():
        row = actual.get(sidecar)
        assert row is not None and row['bytes'] == expected.get('bytes', 0), (sidecar, row)
        assert sidecar == 'project.sqlite-wal' and row['bytes'] == 0 or sidecar == 'project.sqlite-shm'
    before_authored = {k: v for k, v in before.items() if k not in allowed}
    after_authored = {k: v for k, v in after.items() if k not in allowed}
    actual_authored = {k: v for k, v in actual.items() if k not in allowed}
    assert before_authored == after_authored == actual_authored, item['name']
    assert set(actual) - set(before) <= set(allowed), item['name']
    archive_rel = item['archive_destination']
    archive_path = safe_dest(stage, archive_rel)
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    raw = archive_path.with_suffix('.tar')
    total = 0
    with tarfile.open(raw, 'w') as tar:
        for rel, metadata in actual_authored.items():
            source = root.joinpath(*PurePosixPath(rel).parts)
            data = require_hash(source, metadata['sha256'], metadata['bytes'])
            total += len(data)
            assert total <= MAX_FIXTURE_BYTES, (item['name'], total)
            info = tarfile.TarInfo(f"{item['name']}/{rel}")
            info.size = len(data)
            info.mode = 0o600
            tar.addfile(info, io.BytesIO(data))
    raw_bytes = raw.read_bytes()
    encoded = gzip.compress(raw_bytes, compresslevel=9, mtime=0)
    assert gzip.decompress(encoded) == raw_bytes
    raw.unlink()
    archive_path.write_bytes(encoded)
    # Re-open and validate every member; forbid traversal, links, and nonregular entries.
    seen = {}
    with tarfile.open(archive_path, 'r:gz') as tar:
        for member in tar.getmembers():
            name = PurePosixPath(member.name)
            assert member.isfile() and not name.is_absolute() and '..' not in name.parts
            fileobj = tar.extractfile(member)
            assert fileobj is not None
            data = fileobj.read()
            source_rel = name.relative_to(item['name']).as_posix()
            assert source_rel in actual_authored
            expected = actual_authored[source_rel]
            assert len(data) == expected['bytes'] and sha_bytes(data) == expected['sha256']
            seen[source_rel] = {'bytes': len(data), 'sha256': sha_bytes(data)}
    assert seen == actual_authored
    return {'name': item['name'], 'archive': archive_rel, 'archive_sha256': sha(archive_path),
            'authored_files': len(seen), 'authored_bytes': total,
            'excluded_root_sidecars': {key: actual[key] for key in allowed}}


def verify_fixture_snapshot(item):
    root = fixture_root(item)
    before = pinned_map(item, 'before_map', item['_approved_files'])
    after = pinned_map(item, 'after_map', item['_approved_files'])
    before = before.get('files', before)
    after = after.get('files', after)
    allowed = item.get('allowed_sidecars', {})
    actual = snapshot(root)
    for sidecar, expected in allowed.items():
        row = actual.get(sidecar)
        assert row is not None and row['bytes'] == expected.get('bytes', 0)
        assert sidecar == 'project.sqlite-wal' and row['bytes'] == 0 or sidecar == 'project.sqlite-shm'
    before_authored = {k: v for k, v in before.items() if k not in allowed}
    after_authored = {k: v for k, v in after.items() if k not in allowed}
    actual_authored = {k: v for k, v in actual.items() if k not in allowed}
    assert before_authored == after_authored == actual_authored, item['name']
    assert set(actual) - set(before) <= set(allowed), item['name']


def validate_go(manifest, review, envelope_bytes):
    go = json.loads(envelope_bytes['approval/PACKAGING-GO.json'])
    assert go['status'] == 'approved_for_packaging'
    required = {
        'package_script_sha256': sha(SCRIPT),
        'approved_manifest_sha256': sha(MANIFEST),
        'packager_review_sha256': sha_bytes(envelope_bytes['approval/packager-final-review.json']),
        'source_map_sha256': manifest['source_map_sha256'],
        'binary_manifest_sha256': manifest['binary_manifest_sha256'],
        'build_terminal_sha256': manifest['build_terminal_sha256'],
        'measurement_terminal_sha256': manifest['measurement_terminal_sha256'],
        'media_terminal_sha256': manifest['media_completion']['terminal_sha256'],
    }
    assert all(go.get(key) == value for key, value in required.items()), required
    assert review['status'] == 'no_findings'
    assert review['script_sha256'] == sha_bytes(envelope_bytes['approval/package_v2.py'])
    assert review['manifest_sha256'] == sha_bytes(envelope_bytes['approval/approved-package-manifest.json'])


def assert_approved_file(manifest, path, digest):
    source = str(Path(path))
    matches = [row for row in manifest['files']
               if row['source'] == source and row['sha256'] == digest]
    assert len(matches) == 1, (source, digest)


def verify_manifest_inputs(manifest):
    assert manifest['schema'] == 'deadpan-insert-split-proof-package-manifest-1'
    assert manifest['source_map_sha256'] == sha(S / 'frozen-sources.json')
    assert manifest['build_terminal_sha256'] == sha(S / 'build-terminal.json')
    assert manifest['measurement_terminal_sha256'] == sha(S / 'measurement-terminal.json')
    assert read(S / 'build-terminal.json')['status'] == 'complete'
    assert read(S / 'measurement-terminal.json')['status'] == 'complete'
    media = manifest['media_completion']
    completed = require_hash(media['terminal_path'], media['terminal_sha256'])
    terminal = json.loads(completed)
    assert terminal['status'] == 'complete'
    assert terminal['archive_sha256'] == media['archive_sha256']
    archive = require_hash(media['archive_path'], media['archive_sha256'])
    assert len(archive) > 0
    completed_receipt = require_hash(media['receipt_path'], media['receipt_sha256'])
    assert completed_receipt
    failed = media['historical_failure']
    failure_bytes = require_hash(failed['terminal_path'], failed['terminal_sha256'])
    failure = json.loads(failure_bytes)
    assert failure['status'] == 'failed' and failure['stage'] == 'list'
    assert failure['archive_sha256'] == media['archive_sha256']
    failed_receipt = require_hash(failed['receipt_path'], failed['receipt_sha256'])
    assert failed_receipt
    assert_approved_file(manifest, media['terminal_path'], media['terminal_sha256'])
    assert_approved_file(manifest, media['receipt_path'], media['receipt_sha256'])
    assert_approved_file(manifest, failed['terminal_path'], failed['terminal_sha256'])
    assert_approved_file(manifest, failed['receipt_path'], failed['receipt_sha256'])
    assert len(read(S / 'trials.json')) == 10
    assert len(read(S / 'measurement-receipt.json')) == 20
    assert manifest['source_map_sha256'] == sha(S / 'frozen-sources.json')
    assert manifest['binary_manifest_sha256'] == sha(S / 'binaries.json')
    for item in manifest['files']:
        require_hash(item['source'], item['sha256'], item['bytes'])
    for item in manifest['fixtures']:
        item['_approved_files'] = manifest['files']
        pinned_map(item, 'before_map', manifest['files'])
        pinned_map(item, 'after_map', manifest['files'])


def main():
    assert not DEST.exists(), ('refuse existing evidence destination', DEST)
    # Capture the exact approved envelope bytes first; later checks require these same bytes.
    for path in ENVELOPE.values():
        assert path.is_file() and not path.is_symlink(), ('approval envelope must be a regular file', path)
    envelope_bytes = {rel: path.read_bytes() for rel, path in ENVELOPE.items()}
    envelope_hashes = {rel: sha_bytes(data) for rel, data in envelope_bytes.items()}
    manifest = json.loads(envelope_bytes['approval/approved-package-manifest.json'])
    review = json.loads(envelope_bytes['approval/packager-final-review.json'])
    validate_go(manifest, review, envelope_bytes)
    verify_manifest_inputs(manifest)
    DEST.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.insert-split-proof-stage-', dir=DEST.parent))
    try:
        # Preserve the exact approval envelope outside the self-hashed manifest.
        for rel, data in envelope_bytes.items():
            out = safe_dest(stage, rel)
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(data)
        approved_destinations = {row['destination'] for row in manifest['files']}
        approved_destinations |= {row['archive_destination'] for row in manifest['fixtures']}
        reserved = set(ENVELOPE) | {'inventory.json'}
        assert len(approved_destinations) == len(manifest['files']) + len(manifest['fixtures']), 'duplicate payload destinations'
        for candidate in approved_destinations:
            for fixed in reserved:
                assert candidate != fixed and not candidate.startswith(fixed + '/') and not fixed.startswith(candidate + '/'), (candidate, fixed)
        assert len({row['name'] for row in manifest['fixtures']}) == len(manifest['fixtures']), 'duplicate fixture names'
        files = [copy_payload(stage, row) for row in manifest['files']]
        fixtures = [make_fixture_archive(stage, row, manifest['files']) for row in manifest['fixtures']]
        inventory = []
        for path in sorted(stage.rglob('*')):
            if path.is_symlink():
                raise AssertionError(('unexpected output symlink', path))
            if path.is_file():
                inventory.append({'path': path.relative_to(stage).as_posix(),
                                  'bytes': path.stat().st_size, 'sha256': sha(path)})
        summary = {'schema': 'deadpan-evidence-inventory-1', 'files': inventory,
                   'total_files': len(inventory), 'total_bytes': sum(row['bytes'] for row in inventory),
                   'source_payloads': files, 'fixtures': fixtures,
                   'source_map_sha256': manifest['source_map_sha256'],
                   'binary_manifest_sha256': manifest['binary_manifest_sha256'],
                   'build_terminal_sha256': manifest['build_terminal_sha256'],
                   'measurement_terminal_sha256': manifest['measurement_terminal_sha256'],
                   'media_terminal_sha256': manifest['media_completion']['terminal_sha256']}
        write_json(stage / 'inventory.json', summary)
        # Verify copied bytes, compressed payloads, fixture archives, and all approved inputs again
        # immediately before publication. Refuse any changed approved path or extra payload.
        verify_manifest_inputs(manifest)
        assert {rel: sha_bytes(ENVELOPE[rel].read_bytes()) for rel in ENVELOPE} == envelope_hashes
        for item in manifest['fixtures']:
            item['_approved_files'] = manifest['files']
            verify_fixture_snapshot(item)
        for item, saved in zip(manifest['files'], files, strict=True):
            path = safe_dest(stage, item['destination'])
            stored = path.read_bytes()
            decoded = gzip.decompress(stored) if item.get('encoding', 'identity') == 'gzip' else stored
            assert len(decoded) == item['bytes'] and sha_bytes(decoded) == item['sha256']
            assert sha(path) == saved['stored_sha256']
        expected_paths = {row['path'] for row in inventory} | {'inventory.json'}
        assert {p.relative_to(stage).as_posix() for p in stage.rglob('*') if p.is_file()} == expected_paths
        for row in inventory:
            path = stage.joinpath(*PurePosixPath(row['path']).parts)
            assert path.stat().st_size == row['bytes'] and sha(path) == row['sha256']
            if path.suffix == '.gz':
                gzip.decompress(path.read_bytes())
        assert not DEST.exists(), ('refuse destination created during package', DEST)
        # macOS renameatx_np(RENAME_EXCL) publishes a directory atomically without replacement.
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
    inventory_path = DEST / 'inventory.json'
    print(json.dumps({'destination': str(DEST), 'files': len(inventory) + 1,
                      'payload_bytes': summary['total_bytes'],
                      'inventory_sha256': sha(inventory_path),
                      'fixture_archives': len(fixtures)}))


if __name__ == '__main__':
    main()
