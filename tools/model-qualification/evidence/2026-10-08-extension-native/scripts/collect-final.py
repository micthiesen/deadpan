"""Retain complete production qualification receipts without generated media."""
from pathlib import Path
import gzip
import hashlib
import json

ROOT = Path(__file__).resolve().parent
DEST = Path('/Users/michael/Code/deadpan/tools/model-qualification/evidence/2026-10-08-extension-native')


def load(path):
    return json.loads(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def main():
    cases = load(ROOT / 'cases.json')
    results = load(ROOT / 'results.json')
    oracle = load(ROOT / 'oracle-results.json')
    accepted = load(ROOT / 'accepted-verification/summary.json')
    commands = load(ROOT / 'runtime-fix-results.json')
    assert len(cases) == len(results) == len(oracle) == 8
    assert [r['case'] for r in results] == [r['case'] for r in cases]
    assert all(r['ready'] for r in results)
    assert accepted['passed'] and accepted['qualified_count'] == 8
    assert accepted['cli_sha256'] == accepted['cli_sha256_after']
    assert commands[-1]['name'] == 'acceptance-export'
    assert all(r['exit_code'] == 0 for r in commands)
    summary = load(ROOT / 'measurement-summary.json')
    assert summary['complete'] and summary['ready_count'] == 8

    inventory = load(DEST / 'inventory.json')
    paths = []

    def retain(source, relative, compressed=False):
        assert source.is_file() and source.stat().st_size <= 8 * 1024 * 1024, source
        raw = source.read_bytes()
        target = DEST / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as output:
            output.write(gzip.compress(raw, mtime=0) if compressed else raw)
        inventory['copied'].append({
            'retained': str(relative), 'source': str(source.resolve()),
            'source_bytes': len(raw), 'source_sha256': sha(raw),
            'encoding': 'gzip' if compressed else 'verbatim',
        })
        paths.append(str(relative))

    for name in ['results.json', 'oracle-results.json', 'measurement-summary.json',
                 'runtime-fix-results.json']:
        retain(ROOT / name, Path('production') / name)
    for name in ['real-matrix.log', 'pixel-oracle.log', 'acceptance-export.log', 'runtime-fix.log']:
        retain(ROOT / name, Path('logs') / (name + '.gz'), True)
    retain(ROOT / 'accepted-verification/summary.json', Path('acceptance/summary.json.gz'), True)
    for case in cases:
        name = case['case']
        source = ROOT / name
        for filename in ['before.json', 'after-ready.json', 'generated.json',
                         'generated.execution.json', 'generated.stderr',
                         'generated.system.json', 'oracle.json']:
            retain(source / filename, Path('production') / name / (filename + '.gz'), True)
        generated = load(source / 'generated.json')
        provenance = generated['ready']['provenance']
        assert provenance['content']['algorithm'] == 'blake3'
        object_path = source / 'project.deadpan/Media/Generated' / ('blake3-' + provenance['content']['digest'])
        assert object_path.stat().st_size == provenance['byte_length']
        retain(object_path, Path('production') / name / 'qualified-envelope.json.gz', True)
        accepted_case = ROOT / 'accepted-verification' / name
        for path in sorted(accepted_case.iterdir()):
            if path.is_file() and path.suffix in {'.stdout', '.stderr', '.json', '.sb'}:
                retain(path, Path('acceptance') / name / (path.name + '.gz'), True)
        receipt = load(accepted_case / 'summary.json')['receipt']
        retain(Path(receipt['report']), Path('acceptance') / name / 'published-export-report.json.gz', True)

    retain(Path(__file__), Path('scripts/collect-final.py'))
    inventory['scope'] = 'Completed automated, replay, bundle, eight-case production matrix, independent pixel oracle, and acceptance/offline-export evidence.'
    save(DEST / 'inventory.json', inventory)
    # README is reconciled separately before the final manifest is rebuilt.
    print(json.dumps({'retained_files': len(paths), 'files': paths}, indent=2))


if __name__ == '__main__':
    main()
