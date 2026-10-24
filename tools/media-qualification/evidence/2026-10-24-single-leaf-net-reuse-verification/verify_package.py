#!/usr/bin/env python3
"""Read-only independent verification of the already published production package."""
from pathlib import Path, PurePosixPath
import gzip
import hashlib
import io
import json
import os
import stat
import subprocess
import tarfile

S = Path('/tmp/deadpan-single-leaf-net-production-2026-10-24')
ROOT = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-24-single-leaf-net-reuse')
INVENTORY_SHA = 'e0ca61a60782b5c3bb5f7b5b311a2893d0032137d2a3a758fb5785335cada68b'
SCRIPT_SHA = '8fb51381ddd8a9d116f6167e48e15a28b0f6d98ee8f88f523f72b8c91ec1aa9e'
MANIFEST_SHA = '0f48857cd7ba86c6739a048877dc531d68b0613f319020984d5f447104b936fc'
MAGIC = {b'\x7fELF', b'\xcf\xfa\xed\xfe', b'\xce\xfa\xed\xfe',
         b'\xfe\xed\xfa\xcf', b'\xfe\xed\xfa\xce', b'\xca\xfe\xba\xbe'}


def need(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def json_bytes(data):
    return json.loads(data.decode('utf-8', errors='strict'))


def safe(text):
    rel = PurePosixPath(text)
    need(bool(text) and str(rel) == text and not rel.is_absolute() and
         '..' not in rel.parts and '\\' not in text, f'unsafe/noncanonical path: {text}')
    return rel


def package_path(text):
    return ROOT.joinpath(*safe(text).parts)


def decode(row):
    data = package_path(row['destination']).read_bytes()
    encoding = row.get('encoding', 'identity')
    if encoding == 'gzip':
        # Limit expanded bytes to the approved source size, then demand EOF.
        with gzip.GzipFile(fileobj=io.BytesIO(data), mode='rb') as stream:
            raw = stream.read(row['bytes'] + 1)
            need(len(raw) <= row['bytes'] and stream.read(1) == b'',
                 f'gzip exceeds approved source size: {row["destination"]}')
    else:
        need(encoding == 'identity', f'unknown encoding: {encoding}')
        raw = data
    need(len(raw) == row['bytes'] and digest(raw) == row['sha256'],
         f'decoded payload differs: {row["destination"]}')
    need(raw[:4] not in MAGIC, f'executable payload leak: {row["destination"]}')
    return raw


def main():
    output = S / 'root-package-verification.json'
    need(not os.path.lexists(output), 'refusing existing verification receipt')
    need(not ROOT.is_symlink() and ROOT.is_dir(), 'published root must be a real directory')
    need(sha(ROOT / 'inventory.json') == INVENTORY_SHA, 'inventory identity mismatch')
    inventory = json_bytes((ROOT / 'inventory.json').read_bytes())
    expected = {}
    for row in inventory['files']:
        safe(row['path'])
        need(row['path'] not in expected and row['path'] != 'inventory.json', 'duplicate/self inventory')
        expected[row['path']] = row
    actual = {}
    directories = set()
    for p in sorted(ROOT.rglob('*')):
        need(not p.is_symlink(), f'published symlink: {p}')
        mode = p.lstat().st_mode
        if stat.S_ISDIR(mode):
            directories.add(p.relative_to(ROOT).as_posix())
            continue
        need(stat.S_ISREG(mode), f'published special file: {p}')
        rel = p.relative_to(ROOT).as_posix()
        need(not rel.endswith('.rs') and 'target' not in PurePosixPath(rel).parts,
             f'source/target leakage: {rel}')
        actual[rel] = {'path': rel, 'bytes': p.stat().st_size, 'sha256': sha(p)}
    need(set(actual) == set(expected) | {'inventory.json'}, 'published file path set mismatch')
    need({k: v for k, v in actual.items() if k != 'inventory.json'} == expected,
         'stored inventory bytes/hashes mismatch')
    need(inventory['total_files'] == len(expected) == 341 and
         inventory['total_bytes'] == sum(r['bytes'] for r in expected.values()) == 12369663,
         'inventory totals mismatch')
    expected_directories = {str(parent) for path in actual
                            for parent in PurePosixPath(path).parents if str(parent) != '.'}
    need(directories == expected_directories, 'unexpected empty/extra directory')
    manifest_raw = (ROOT / 'approval/approved-package-manifest.json').read_bytes()
    need(digest(manifest_raw) == MANIFEST_SHA, 'captured approved manifest mismatch')
    manifest = json_bytes(manifest_raw)
    review_raw = (ROOT / 'approval/packager-final-review.json').read_bytes()
    review = json_bytes(review_raw)
    go_raw = (ROOT / 'approval/PACKAGING-GO.json').read_bytes()
    go = json_bytes(go_raw)
    script_raw = (ROOT / 'approval/package.py').read_bytes()
    need(digest(script_raw) == SCRIPT_SHA and review['script_sha256'] == SCRIPT_SHA and
         review['manifest_sha256'] == MANIFEST_SHA and review['status'] == 'no_findings' and
         review['findings'] == [], 'captured script/review identity mismatch')
    need(go['status'] == 'approved_for_packaging' and
         go['package_script_sha256'] == SCRIPT_SHA and go['approved_manifest_sha256'] == MANIFEST_SHA and
         go['packager_review_sha256'] == digest(review_raw), 'captured GO mismatch')
    need(sha(S / 'approved-package-manifest.json') == MANIFEST_SHA and
         sha(S / 'packaging-draft/package.py') == SCRIPT_SHA and
         sha(S / 'packager-final-review.json') == digest(review_raw) and
         sha(S / 'PACKAGING-GO.json') == digest(go_raw), 'live approval input changed')
    package = json_bytes((ROOT / 'package-manifest.json').read_bytes())
    need(package['schema'] == 'deadpan-single-leaf-net-package-1' and
         package['manifest_sha256'] == MANIFEST_SHA and
         package['candidate_app_sha256'] == manifest['candidate_app_sha256'] and
         package['source_map_sha256'] == manifest['candidate_source_map_sha256'] and
         package['terminal_sha256'] == manifest['terminal_sha256'], 'package metadata mismatch')
    payloads = manifest['payloads']
    need(len(payloads) == 320, 'approved payload count changed')
    bydest = {r['destination']: r for r in payloads}
    need(len(bydest) == len(payloads), 'duplicate payload destination')
    copied = {r['path']: r for r in package['copied_payloads']}
    need(set(copied) == set(bydest), 'copied payload metadata path set mismatch')
    gzip_files = 0
    live_payload_bytes = 0
    for row in payloads:
        rel = row['destination']
        raw = decode(row)
        stored = actual[rel]
        meta = copied[rel]
        need(meta == {'path': rel, 'source_sha256': row['sha256'], 'source_bytes': len(raw),
                      'stored_sha256': stored['sha256'], 'stored_bytes': stored['bytes'],
                      'encoding': row.get('encoding', 'identity')},
             f'copied payload metadata mismatch: {rel}')
        gzip_files += row.get('encoding') == 'gzip'
        src = Path(row['source'])
        need(not src.is_symlink() and src.is_file() and stat.S_ISREG(src.stat().st_mode) and
             src.stat().st_size == row['bytes'] and sha(src) == row['sha256'],
             f'live approved input changed: {src}')
        live_payload_bytes += row['bytes']

    def captured_json(ref):
        row = bydest[ref['destination']]
        need(all(row[k] == ref[k] for k in ('source', 'destination', 'bytes', 'sha256')),
             'fixture map reference mismatch')
        value = json_bytes(decode(row))
        return value.get('files', value)

    fixture_meta = {r['name']: r for r in package['fixtures']}
    need(len(manifest['fixtures']) == len(fixture_meta) == 16, 'fixture count mismatch')
    members = member_bytes = 0
    for fixture in manifest['fixtures']:
        name = fixture['name']
        need(str(safe(name)) == name and len(safe(name).parts) == 1, 'unsafe fixture name')
        before, after = captured_json(fixture['before_map']), captured_json(fixture['after_map'])
        allowed = fixture.get('allowed_sidecars', {})
        need(set(allowed) <= {'project.sqlite-shm', 'project.sqlite-wal'}, 'invalid sidecar exception')
        need(after.get('project.sqlite-wal', {}).get('bytes', 0) == 0 and
             allowed.get('project.sqlite-wal', {}).get('bytes', 0) == 0, 'recorded root WAL not empty')
        strip = lambda rows: {k: v for k, v in rows.items() if k not in allowed}
        authored = strip(before)
        need(authored == strip(after), 'fixture recorded authored maps differ')
        archive = package_path(fixture['archive_destination'])
        seen, total = {}, 0
        with tarfile.open(archive, mode='r:gz') as tar:
            for member in tar:
                need(member.isfile() and not member.issym() and not member.islnk(),
                     f'nonregular TAR member: {member.name}')
                rel = safe(member.name)
                need(rel.parts[0] == name and len(rel.parts) > 1, 'TAR root mismatch')
                key = PurePosixPath(*rel.parts[1:]).as_posix()
                need(key not in seen and key in authored and
                     member.size == authored[key]['bytes'], 'unexpected/duplicate/oversized TAR member')
                need(key not in allowed, 'excluded root SQLite sidecar archived')
                stream = tar.extractfile(member)
                need(stream is not None, 'missing TAR content')
                content = stream.read(member.size + 1)
                need(len(content) == member.size and digest(content) == authored[key]['sha256'],
                     'TAR member differs from captured approved map')
                seen[key] = {'bytes': len(content), 'sha256': digest(content)}
                total += len(content)
                need(total <= 200000000, 'TAR fixture exceeds copier bound')
        need(seen == authored, 'TAR path set mismatch')
        meta = fixture_meta[name]
        need(meta == {'name': name, 'archive': fixture['archive_destination'],
                      'archive_sha256': sha(archive), 'authored_files': len(seen),
                      'authored_bytes': total, 'excluded_sidecars': allowed},
             'fixture archive metadata mismatch')
        root = Path(fixture['root'])
        live = {}
        need(root.is_dir() and not root.is_symlink(), 'live fixture root changed')
        for p in root.rglob('*'):
            need(not p.is_symlink(), 'live fixture symlink')
            if p.is_dir():
                continue
            need(stat.S_ISREG(p.stat().st_mode), 'live fixture special file')
            live[p.relative_to(root).as_posix()] = {'bytes': p.stat().st_size, 'sha256': sha(p)}
        need(strip(live) == authored and set(live) - set(before) <= set(allowed),
             'live fixture authored state differs')
        for sidecar, expected_sidecar in allowed.items():
            need(live.get(sidecar, {}).get('bytes') == expected_sidecar['bytes'], 'live sidecar size changed')
        need(live.get('project.sqlite-wal', {}).get('bytes', 0) == 0, 'live root WAL not empty')
        members += len(seen)
        member_bytes += total

    def payload_for_source(source):
        matches = [r for r in payloads if r['source'] == source]
        need(len(matches) == 1, f'nonunique retained source: {source}')
        return matches[0]

    candidate_map_row = payload_for_source(str(S / 'frozen-sources.json'))
    base_map_row = payload_for_source(str(S / 'qualified-base-sources.json'))
    need(candidate_map_row['sha256'] == manifest['candidate_source_map_sha256'] and
         base_map_row['sha256'] == manifest['qualified_base_source_map_sha256'],
         'captured source-map identities mismatch')
    candidate_map = json_bytes(decode(candidate_map_row))
    base_map = json_bytes(decode(base_map_row))
    need(len(candidate_map) == len(base_map) == 2053 and set(candidate_map) == set(base_map),
         'production source-map path set mismatch')
    delta = sorted(p for p in candidate_map if candidate_map[p] != base_map[p])
    need(delta == sorted(r['path'] for r in manifest['source_changes']) and len(delta) == 3,
         'production before/after delta mismatch')
    need(manifest['initial_attempt_source_snapshots'] == [], 'unexpected production initial snapshots')
    for change in manifest['source_changes']:
        need(change['after_sha256'] == candidate_map[change['path']] ==
             bydest[change['after_destination']]['sha256'] and
             change['before_sha256'] == base_map[change['path']] ==
             bydest[change['before_destination']]['sha256'] and
             change['after_destination'].endswith('.rs.txt') and
             change['before_destination'].endswith('.rs.txt'), 'source snapshot mismatch')
    base_root = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23/source')
    for root, expected_map in ((S / 'source', candidate_map), (base_root, base_map)):
        need({str(p.relative_to(root)): sha(p) for p in root.rglob('*.rs')} == expected_map,
             'live immutable source tree differs')
    main = Path('/Users/michael/Code/deadpan')
    main_paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=main,
                                         text=True).splitlines()
    need({p: sha(main / p) for p in main_paths} == candidate_map,
         'promoted Main differs from production candidate')
    need('crates/deadpan-core/src/pause_route_profile.rs' not in candidate_map,
         'unexpected production profile source')

    terminal_names = {'build': 'build-terminal.json', 'measurement': 'measurement-terminal.json',
                      'media': 'media-terminal.json', 'memory': 'memory-terminal.json'}
    terminals = {}
    for kind, filename in terminal_names.items():
        row = payload_for_source(str(S / filename))
        need(row['sha256'] == manifest['terminal_sha256'][kind], 'terminal retained hash mismatch')
        terminals[kind] = json_bytes(decode(row))
        need(terminals[kind]['status'] == 'complete', 'terminal not complete')
    media_rows = manifest['media_files']
    need(len(media_rows) == 35, 'media payload row count mismatch')
    for row in media_rows:
        need(all(bydest[row['destination']][k] == row[k] for k in ('source', 'bytes', 'sha256')),
             'media row differs from payload')
        if row['relative_path'].endswith('project.sqlite-wal'):
            need(row['bytes'] == 0, 'media package root WAL not empty')
    mp4s = []
    for export in terminals['media']['evidence']['exports']:
        matches = [r for r in media_rows if Path(r['source']).resolve() == Path(export['movie']).resolve()]
        need(len(matches) == 1 and matches[0]['sha256'] == export['movie_sha256'],
             'actual export MP4 not exactly retained')
        raw = decode(bydest[matches[0]['destination']])
        need(digest(raw) == export['movie_sha256'], 'retained MP4 byte digest mismatch')
        mp4s.append({'leg': export['leg'], 'destination': matches[0]['destination'],
                     'sha256': digest(raw), 'bytes': len(raw)})
    need(len(mp4s) == 2, 'expected two actual media export movies')
    envelope = {'approval/approved-package-manifest.json', 'approval/packager-final-review.json',
                'approval/PACKAGING-GO.json', 'approval/package.py'}
    expected_paths = set(bydest) | {r['archive_destination'] for r in manifest['fixtures']} | envelope | {
        'package-manifest.json', 'inventory.json'}
    need(set(actual) == expected_paths, 'published path set differs from approved logical package')
    need(len(actual) == 342 and members == 48, 'final file/member count mismatch')
    # Re-read the inventory identity at the end, without altering the primary package.
    need(sha(ROOT / 'inventory.json') == INVENTORY_SHA, 'inventory changed during audit')
    result = {
        'status': 'complete', 'verification': 'independent published-package reread',
        'verifier_sha256': sha(Path(__file__)),
        'root': str(ROOT), 'inventory_sha256': INVENTORY_SHA,
        'inventory_files_excluding_itself': 341, 'all_stored_files': len(actual),
        'inventory_bytes_excluding_itself': 12369663,
        'approved_manifest_sha256': MANIFEST_SHA, 'package_script_sha256': SCRIPT_SHA,
        'packager_review_sha256': digest(review_raw), 'packaging_GO_sha256': digest(go_raw),
        'payloads_decoded_verified': len(payloads), 'gzip_payloads': gzip_files,
        'identity_payloads': len(payloads) - gzip_files, 'raw_payload_bytes': live_payload_bytes,
        'fixtures_verified': 16, 'safe_regular_TAR_members': members,
        'fixture_authored_bytes': member_bytes, 'media_file_rows_verified': 35,
        'actual_export_MP4s': mp4s, 'live_approved_payload_hashes_verified': len(payloads),
        'live_fixture_maps_and_root_WAL_zero_verified': 16,
        'terminal_hashes_verified': manifest['terminal_sha256'],
        'production_source_deltas_verified': 3, 'initial_source_snapshots': 0,
        'candidate_Rust_paths': 2053, 'immutable_base_Rust_paths': 2053,
        'Main_rg_paths_equal_promoted_candidate': True,
        'candidate_source_map_sha256': manifest['candidate_source_map_sha256'],
        'qualified_base_source_map_sha256': manifest['qualified_base_source_map_sha256'],
        'no_bare_rs_target_or_executable_payload_leak': True,
        'primary_package_unchanged_by_verifier': True,
        'notes': [
            'The immutable package.py bytes occur as both provenance payload and approval envelope input. Both match the same pinned bytes; this duplication is intentional and is not an output hash cycle.',
            'Root SQLite sidecars are excluded from fixture TAR members exactly as approved. Captured and live root WAL sizes are zero; the 35 explicit media payload rows separately retain their exact sidecar bytes.',
            'TAR streams were read without extracting members. Gzip payloads were decoded under their approved source byte limit.',
            'Verification establishes retained paths/bytes/hash agreement, not new correctness, latency, memory, production qualification or crash-durability results.'
        ]
    }
    with output.open('x') as stream:
        stream.write(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'status': 'complete', 'receipt': str(output), 'receipt_sha256': sha(output),
                      'inventory_files': 341, 'all_files': 342, 'payloads': 320,
                      'gzip_payloads': gzip_files, 'fixtures': 16, 'TAR_members': members,
                      'media_files': 35, 'actual_MP4s': 2}), flush=True)


if __name__ == '__main__':
    main()
