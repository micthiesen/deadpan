"""Prepare a proposed explicit payload list. This does not copy or publish evidence."""
from pathlib import Path
import hashlib
import json
import stat

S = Path('/tmp/deadpan-single-leaf-net-production-2026-10-24')
BASE_SOURCE = Path('/tmp/deadpan-insert-split-proof-production-2026-10-23/source')
REPO = Path('/Users/michael/Code/deadpan')
OUT = S / 'approved-package-manifest.json'
ALLOWED_SUFFIXES = {'.json', '.py', '.md', '.patch', '.stdout', '.stderr'}
EXCLUDE_V = {'media-one-leaf.tar.zst'}
GENERATED = {OUT, S / 'packaging-draft' / 'approved-package-manifest.json'}
MAX_PAYLOAD_FILES = 1000


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def item(source, destination, encoding='identity'):
    p = Path(source)
    assert not p.is_symlink() and p.is_file()
    raw = p.read_bytes()
    return {'source': str(p), 'destination': destination, 'bytes': len(raw),
            'sha256': hashlib.sha256(raw).hexdigest(), 'encoding': encoding}


def add_payload(rows, source, destination, compress=True):
    p = Path(source)
    encoding = 'gzip' if compress and p.stat().st_size > 100_000 else 'identity'
    rows.append(item(p, destination, encoding))


def direct_files(root):
    return sorted(p for p in root.iterdir()
                  if p.is_file() and not p.is_symlink() and p.suffix in ALLOWED_SUFFIXES)


def ref_for(source, destination, rows):
    src = str(Path(source))
    matches = [r for r in rows if r['source'] == src and r['destination'] == destination]
    assert len(matches) == 1, (src, destination)
    row = matches[0]
    return {'source': src, 'destination': destination,
            'bytes': row['bytes'], 'sha256': row['sha256']}


def main():
    assert not OUT.exists(), ('refuse existing proposed manifest', OUT)
    rows = []
    # Retain all direct, bounded evidence-side metadata and text from this exact scratch root.
    for root, prefix, excluded in [(S, 'production', EXCLUDE_V)]:
        assert root.is_dir()
        for path in direct_files(root):
            if path.name in excluded:
                continue
            add_payload(rows, path, f'{prefix}/{path.name}')
    # Retain the report and the exact manifest-preparation script as reproducibility records.
    add_payload(rows, S/'packaging-draft'/'report.md', 'report.md', compress=False)
    add_payload(rows, Path(__file__), 'provenance/prepare_manifest.py', compress=False)
    add_payload(rows, S/'packaging-draft'/'package.py', 'provenance/package.py', compress=False)
    observer = json.loads((S/'observer-provenance.json').read_text())
    for field, dest in [('actual_source', 'provenance/memory-observer/actual.rs.txt'),
                        ('reviewed_source', 'provenance/memory-observer/reviewed.rs.txt')]:
        source = Path(observer[field])
        assert sha(source) == observer[field + '_sha256']
        add_payload(rows, source, dest, compress=True)
    native_source = REPO/'native/deadpan-process/src/lib.rs'
    assert sha(native_source) == observer['actual_native_process_source_sha256']
    add_payload(rows, native_source, 'provenance/memory-observer/native-process.rs.txt', compress=True)
    # Preserve the bounded raw JSON/HTML outputs emitted into named report directories.
    report_dirs = sorted(p for p in S.iterdir() if p.is_dir() and p.name.endswith('-report'))
    for report_dir in report_dirs:
        for path in sorted(report_dir.iterdir()):
            if path.is_file() and path.suffix in {'.json', '.html', '.stdout', '.stderr'}:
                add_payload(rows, path, f'reports/{report_dir.name}/{path.name}')

    candidate = json.loads((S/'frozen-sources.json').read_text())
    base = json.loads((S/'qualified-base-sources.json').read_text())
    live = {str(p.relative_to(S/'source')): sha(p) for p in (S/'source').rglob('*.rs')}
    assert live == candidate
    base_live = {str(p.relative_to(BASE_SOURCE)): sha(p) for p in BASE_SOURCE.rglob('*.rs')}
    assert base_live == base, 'immutable qualified Main source snapshot differs from retained map'
    changes = sorted(p for p in set(candidate)|set(base) if candidate.get(p)!=base.get(p))
    source_changes = []
    for rel in changes:
        final_src = S/'source'/rel
        final_dest = 'source/final/'+rel+'.txt'
        add_payload(rows, final_src, final_dest, compress=True)
        before_dest = None
        if rel in base:
            before_src = BASE_SOURCE/rel
            before_dest = 'source/qualified-main/'+rel+'.txt'
            add_payload(rows, before_src, before_dest, compress=True)
        source_changes.append({'path':rel,'before_sha256':base.get(rel),
                               'before_destination':before_dest,
                               'after_sha256':candidate[rel],
                               'after_destination':final_dest})
    initial_snapshots = []

    # Preserve the six exact build inputs as bytes, in addition to their hash map.
    inputs = json.loads((S/'baseline-inputs.json').read_text())
    assert len(inputs) == 6
    for rel, expected in sorted(inputs.items()):
        source = S/'source'/rel
        assert sha(source) == expected
        add_payload(rows, source, 'inputs/'+rel, compress=True)

    # Each timing/memory fixture has explicit before/after authored maps. Sidecars are recorded separately.
    fixtures = []
    roots = sorted(S.glob('*.deadpan'))
    assert len(roots) == 16, len(roots)
    for root in roots:
        name = root.name[:-len('.deadpan')]
        before = S/(name+'-input-before.json')
        after = S/(name+'-input-after.json')
        assert before.is_file() and after.is_file(), name
        before_dest = 'production/'+before.name
        after_dest = 'production/'+after.name
        b = json.loads(before.read_text())
        a = json.loads(after.read_text())
        allowed = {}
        for sidecar in ('project.sqlite-shm','project.sqlite-wal'):
            if sidecar in a:
                allowed[sidecar] = {'bytes':a[sidecar]['bytes']}
        fixtures.append({'name':name,'root':str(root),
                         'before_map':ref_for(before,before_dest,rows),
                         'after_map':ref_for(after,after_dest,rows),
                         'allowed_sidecars':allowed,
                         'archive_destination':'fixtures/'+name+'.tar.gz'})

    # Retain every source file in the actual 35-file media evidence inventory as a separately
    # hash-pinned payload, including both emitted movies and package/PCM/history evidence.
    media_inv_path = S/'media-evidence-inventory.json'
    media_inv = json.loads(media_inv_path.read_text())
    media_root = Path(media_inv['root'])
    media_files = []
    for rel, meta in sorted(media_inv['files'].items()):
        source = media_root/rel
        assert not source.is_symlink() and source.is_file()
        assert source.stat().st_size == meta['bytes'] and sha(source) == meta['sha256']
        dest='media-evidence/'+rel
        add_payload(rows,source,dest,compress=False)
        media_files.append({'relative_path':rel,'source':str(source),'destination':dest,
                            'bytes':meta['bytes'],'sha256':meta['sha256']})
    assert len(media_files) == 35, len(media_files)

    destinations = [r['destination'] for r in rows]
    assert len(destinations) == len(set(destinations))
    assert len(rows) <= MAX_PAYLOAD_FILES
    for row in rows:
        p=Path(row['source'])
        assert not p.is_symlink() and stat.S_ISREG(p.lstat().st_mode)
        assert sha(p)==row['sha256'] and p.stat().st_size==row['bytes']

    manifest = {
        'schema':'deadpan-single-leaf-net-evidence-1',
        'status':'proposed_for_root_review',
        'candidate_source_map_sha256':sha(S/'frozen-sources.json'),
        'qualified_base_source_map_sha256':sha(S/'qualified-base-sources.json'),
        'input_map_sha256':sha(S/'baseline-inputs.json'),
        'source_review_sha256':sha(S/'source-review.json'),
        'analysis_review_sha256':sha(S/'analysis-review.json'),
        'binary_manifest_sha256':sha(S/'binaries.json'),
        'observer_provenance_sha256':sha(S/'observer-provenance.json'),
        'observer_sha256':observer['observer_sha256'],
        'candidate_app_sha256':json.loads((S/'build-terminal.json').read_text())['app_sha256'],
        'control_app_sha256':json.loads((S/'baseline.json').read_text())['control']['sha256'],
        'matched_profile_app_sha256':json.loads((S/'baseline.json').read_text())['matched_profile']['sha256'],
        'terminal_sha256':{
            'build':sha(S/'build-terminal.json'),
            'measurement':sha(S/'measurement-terminal.json'),
            'media':sha(S/'media-terminal.json'),
            'memory':sha(S/'memory-terminal.json')},
        'source_changes':source_changes,
        'initial_attempt_source_snapshots':initial_snapshots,
        'payloads':rows,
        'fixtures':fixtures,
        'media_files':media_files,
    }
    OUT.write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'manifest':str(OUT),'payloads':len(rows),
                      'fixture_archives':len(fixtures),'media_files':len(media_files),
                      'candidate_source_changes':len(source_changes),
                      'initial_attempt_source_snapshots':len(initial_snapshots),
                      'manifest_sha256':sha(OUT)},sort_keys=True))


if __name__=='__main__':
    main()
