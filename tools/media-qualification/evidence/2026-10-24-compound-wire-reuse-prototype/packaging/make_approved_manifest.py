"""Build the explicit, hash-pinned payload inventory for Root review only."""
from pathlib import Path
import hashlib
import json

S = Path('/tmp/deadpan-compound-wire-reuse-prototype-2026-10-24')
D = S / 'packaging-draft'
OUT = S / 'approved-package-manifest.json'
BASE = Path('/tmp/deadpan-single-leaf-net-prototype-v2-2026-10-24/source')
QUALIFIED = Path('/tmp/deadpan-single-leaf-net-production-2026-10-24/source')
HELPERS = Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json')
EMPTY_SHA = hashlib.sha256(b'').hexdigest()
SHM_SHA = 'fd4c9fda9cd3f9ae7c962b0ddf37232294d55580e1aa165aa06129b8549389eb'
SIDECARS = {
    'project.sqlite-shm': {'bytes': 32768, 'sha256': SHM_SHA},
    'project.sqlite-wal': {'bytes': 0, 'sha256': EMPTY_SHA},
}
TEXT_SUFFIXES = {'.json', '.py', '.md', '.patch', '.stdout', '.stderr', '.txt'}
EXCLUDED_TOP = {'media-one-leaf.tar.zst', 'approved-package-manifest.json',
                'packager-final-review.json', 'PACKAGING-GO.json'}
PAYLOADS = []
_DESTINATIONS = set()


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def add(path, destination, category, *, encoding=None):
    source = Path(path)
    assert source.is_file() and not source.is_symlink(), source
    assert destination not in _DESTINATIONS, destination
    _DESTINATIONS.add(destination)
    size = source.stat().st_size
    mode = encoding or ('gzip' if size > 100_000 and category != 'media-evidence' else 'identity')
    dest = destination
    if mode == 'gzip':
        assert category != 'source-text', (source, 'Rust source snapshots remain plain .rs.txt')
        if not dest.endswith('.gz'):
            dest += '.gz'
    if category == 'source-text':
        assert source.suffix == '.rs' and dest.endswith('.rs.txt'), (source, dest)
    PAYLOADS.append({'source': str(source), 'destination': dest, 'category': category,
                     'bytes': size, 'sha256': sha(source), 'encoding': mode})
    return dest


def add_record_file(path):
    p = Path(path)
    return add(p, f'records/{p.name}', 'record')


def main():
    # Every root-level record, source driver, raw command stream, and review is explicit
    # in the output manifest. No recursively harvested source, binary or target tree.
    for path in sorted(S.iterdir()):
        if not path.is_file() or path.name in EXCLUDED_TOP:
            continue
        if path.suffix.lower() in TEXT_SUFFIXES:
            add_record_file(path)

    # Preserve the three known package/preparation snapshots, including exact recovery
    # of the original draft from its reversible sidecar edit and recorded hash.
    for filename, destination in [
        ('package-initial-draft-reconstructed.py.txt', 'history/package-initial-draft.py.txt'),
        ('package-after-sidecar-fix.py.txt', 'history/package-after-sidecar-fix.py.txt'),
        ('static-packaging-plan-initial.md.txt', 'history/static-packaging-plan-initial.md.txt'),
        ('sidecar-correction-history.json', 'history/sidecar-correction-history.json'),
        ('approved-package-manifest-before-input-fix.json.txt', 'history/approved-package-manifest-before-input-fix.json.txt'),
        ('input-payload-correction-history.json', 'history/input-payload-correction-history.json'),
        ('approved-package-manifest-before-plan-pin.json.txt', 'history/approved-package-manifest-before-plan-pin.json.txt'),
        ('package-before-plan-pin.py.txt', 'history/package-before-plan-pin.py.txt'),
        ('plan-pin-correction-history.json', 'history/plan-pin-correction-history.json'),
    ]:
        add(D / filename, destination, 'packaging-history')
    # Retain this generator so the explicit inventory can be regenerated and audited.
    generator = D / 'make_approved_manifest.py'
    add(generator, 'packaging/make_approved_manifest.py', 'packaging-tool')

    candidate = json.loads((S / 'frozen-sources.json').read_text())
    profile = json.loads((S / 'baseline-sources.json').read_text())
    qualified = json.loads((S / 'qualified-base-sources.json').read_text())
    assert len(candidate) == len(profile) == 2054 and len(qualified) == 2053

    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert len(inputs) == 6
    for name, expected in inputs.items():
        source = S / 'source' / name
        assert sha(source) == expected and sha(BASE / name) == expected, name
        add(source, f'inputs/{name}', 'pinned-input', encoding='identity')

    source_after = {}
    for path in sorted(set(candidate) | set(profile) | set(qualified)):
        if candidate.get(path) != qualified.get(path):
            src = S / 'source' / path
            dest = add(src, f'source/candidate/{path}.txt', 'source-text', encoding='identity')
            source_after[path] = dest
    qualified_overlay = []
    for path in sorted(set(candidate) | set(qualified)):
        if candidate.get(path) == qualified.get(path):
            continue
        before_dest = None
        if path in qualified:
            before_dest = add(QUALIFIED / path, f'source/qualified-before/{path}.txt',
                              'source-text', encoding='identity')
        qualified_overlay.append({'path': path,
                                  'before_sha256': qualified.get(path),
                                  'after_sha256': candidate.get(path),
                                  'before_destination': before_dest,
                                  'after_destination': source_after[path]})

    source_changes = []
    for path in sorted(set(candidate) | set(profile)):
        if candidate.get(path) == profile.get(path):
            continue
        before_dest = add(BASE / path, f'source/profile-before/{path}.txt',
                          'source-text', encoding='identity')
        source_changes.append({'path': path,
                               'before_sha256': profile.get(path),
                               'after_sha256': candidate.get(path),
                               'before_destination': before_dest,
                               'after_destination': source_after[path]})

    # Preserve each report and HTML rendering separately from its command streams.
    for rotation in range(1, 7):
        for kind in ('control', 'baseline', 'candidate'):
            report = S / f'rotation-{rotation}-{kind}-report'
            assert report.is_dir()
            for path in sorted(report.iterdir()):
                assert path.is_file() and path.suffix in {'.json', '.html'}
                add(path, f'native-reports/{rotation}/{kind}/{path.name}', 'native-report')

    # Retain all 18 authored fixture states by exact path, maps and safe archive plan.
    fixtures = []
    for rotation in range(1, 7):
        for kind in ('control', 'baseline', 'candidate'):
            name = f'rotation-{rotation}-{kind}'
            root = S / f'{name}.deadpan'
            before_name = f'{name}-input-before.json'
            after_name = f'{name}-input-after.json'
            before_path = S / before_name
            after_path = S / after_name
            before_dest = f'records/{before_name}'
            after_dest = f'records/{after_name}'
            assert before_dest in _DESTINATIONS and after_dest in _DESTINATIONS
            before_map = json.loads(before_path.read_text())
            after_map = json.loads(after_path.read_text())
            assert set(before_map).isdisjoint(SIDECARS)
            assert {k: after_map[k] for k in SIDECARS} == SIDECARS
            fixtures.append({'name': name, 'root': str(root),
                             'before_map': {'source': str(before_path), 'destination': before_dest,
                                            'bytes': before_path.stat().st_size, 'sha256': sha(before_path)},
                             'after_map': {'source': str(after_path), 'destination': after_dest,
                                           'bytes': after_path.stat().st_size, 'sha256': sha(after_path)},
                             'archive_destination': f'fixtures/{name}.tar.gz',
                             'allowed_sidecars': SIDECARS})

    # Include the complete independently inventoried media evidence tree, preserving
    # all exact file paths (including packages, canonical data, and both movie exports).
    media_inventory_path = S / 'media-evidence-inventory.json'
    media_inventory = json.loads(media_inventory_path.read_text())
    media_root = Path(media_inventory['root'])
    assert media_root.is_dir()
    assert len(media_inventory['files']) == 35
    for relative, meta in sorted(media_inventory['files'].items()):
        source = media_root / relative
        assert source.is_file() and not source.is_symlink(), source
        assert source.stat().st_size == meta['bytes'] and sha(source) == meta['sha256'], relative
        add(source, f'media/{relative}', 'media-evidence', encoding='identity')

    binaries = json.loads((S / 'binaries.json').read_text())
    baseline = json.loads((S / 'baseline.json').read_text())
    terminal = json.loads((S / 'media-terminal.json').read_text())
    helper_manifest = json.loads(HELPERS.read_text())
    manifest = {
        'schema': 'deadpan-compound-wire-reuse-package-1',
        'candidate_source_map_sha256': sha(S / 'frozen-sources.json'),
        'matched_profile_source_map_sha256': sha(S / 'baseline-sources.json'),
        'qualified_main_source_map_sha256': sha(S / 'qualified-base-sources.json'),
        'input_map_sha256': sha(S / 'baseline-inputs.json'),
        'source_review_sha256': sha(S / 'source-review.json'),
        'binary_manifest_sha256': sha(S / 'binaries.json'),
        'candidate_app_sha256': binaries['deadpan-app']['sha256'],
        'control_app_sha256': baseline['control']['sha256'],
        'matched_profile_app_sha256': baseline['matched_profile']['sha256'],
        'helper_manifest_sha256': sha(HELPERS),
        'helper_sha256': {name: row['sha256'] for name, row in helper_manifest.items()
                          if name in {'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track'}},
        'analysis_sha256': sha(S / 'analysis.json'),
        'analysis_review_sha256': sha(S / 'analysis-review.json'),
        'static_packaging_plan_sha256': sha(D / 'static-packaging-plan.md'),
        'terminal_sha256': {name: sha(S / f'{name}-terminal.json')
                            for name in ('build', 'measurement', 'media')},
        'source_changes': source_changes,
        'qualified_overlay': qualified_overlay,
        'payloads': PAYLOADS,
        'fixtures': fixtures,
        'media_inventory_sha256': sha(media_inventory_path),
        'media_exports': [{'leg': row['leg'], 'sha256': row['movie_sha256']}
                          for row in terminal['evidence']['exports']],
        'generation_notes': {
            'native_fixture_sidecars': SIDECARS,
            'native_sidecars_omitted_from_archives': ['project.sqlite-shm', 'project.sqlite-wal'],
            'source_snapshot_differences_vs_qualified_main': len(qualified_overlay),
            'source_snapshot_differences_vs_matched_profile': len(source_changes),
            'media_tree_files': len(media_inventory['files']),
            'memory_lane': False,
        },
    }
    assert len(source_changes) == 2
    assert len(qualified_overlay) == 14
    assert len(fixtures) == 18
    assert len([p for p in PAYLOADS if p['category'] == 'media-evidence']) == 35
    OUT.write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    print(json.dumps({'manifest': str(OUT), 'payloads': len(PAYLOADS),
                      'fixtures': len(fixtures), 'qualified_overlay_paths': len(qualified_overlay),
                      'matched_profile_changes': len(source_changes), 'media_files': len(media_inventory['files']),
                      'manifest_sha256': sha(OUT), 'manifest_bytes': OUT.stat().st_size}, sort_keys=True))


if __name__ == '__main__':
    main()
