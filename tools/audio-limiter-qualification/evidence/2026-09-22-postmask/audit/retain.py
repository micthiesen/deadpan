"""Retain exact experimental evidence without regenerating any producer PCM."""
import ast
import hashlib
import json
from pathlib import Path
import shutil

REPO = Path('/Users/michael/Code/deadpan')
PRODUCER = Path('/tmp/deadpan-postmask-limiter-20260921')
AUDIT = Path('/tmp/deadpan-postmask-audit-20260922')
DESTINATION = REPO / 'tools/audio-limiter-qualification/evidence/2026-09-22-postmask'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    DESTINATION.mkdir(exist_ok=False)
    retained = {}

    def copy(source, relative):
        target = DESTINATION / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        before = sha(source)
        shutil.copyfile(source, target)
        assert sha(source) == before == sha(target)
        retained[str(relative)] = {'source': str(source), 'sha256': before}

    for name in ['prototype.py', 'shortened_edge.py', 'pre-run-manifest.json',
                 'shortened-edge-pre-run-manifest.json', 'results.json',
                 'shortened-edge-results.json', 'README.md']:
        copy(PRODUCER / name, Path('producer') / name)
    for path in sorted((PRODUCER / 'source').iterdir()):
        if path.is_file():
            copy(path, Path('producer/source') / path.name)
    cases = []
    for name in ['results.json', 'shortened-edge-results.json']:
        cases.extend(json.loads((PRODUCER / name).read_text())['results'])
    for case in cases:
        name = case['case']
        for folder, suffix, key in [('outputs', '.wav', 'wav_sha256'), ('meters', '.json', 'meter_sha256')]:
            path = PRODUCER / folder / (name + suffix)
            assert sha(path) == case['artifacts'][key]
            copy(path, Path('producer') / folder / path.name)
        raw = PRODUCER / 'outputs' / (name + '.f32le')
        assert sha(raw) == case['artifacts']['f32le_sha256']
        assert raw.read_bytes() == (PRODUCER / 'outputs' / (name + '.wav')).read_bytes()[44:]
        assert sha(AUDIT / 'inputs' / (name + '.wav')) == case['artifacts']['wav_sha256']
    for name in ['audit.py', 'finite_sinc.py', 'environment.json', 'results.json',
                 'high-precision.json', 'summary.json', 'retain.py']:
        copy(AUDIT / name, Path('audit') / name)
    manifest = json.loads((PRODUCER / 'pre-run-manifest.json').read_text())
    checks = {}
    for name, path in manifest['paths'].items():
        checks[name] = sha(Path(path)) == manifest['hashes'][name]
    short = json.loads((PRODUCER / 'shortened-edge-pre-run-manifest.json').read_text())
    checks['shortened_edge.py'] = sha(PRODUCER / 'shortened_edge.py') == short['hashes']['shortened_edge.py']
    assert all(checks.values())
    (DESTINATION / 'retention.json').write_text(json.dumps({
        'schema_version': 1, 'byte_identical_copies': retained,
        'producer_pre_run_hashes_rechecked': checks,
        'raw_pcm_excluded_as_identical_to_retained_wav_payload': len(cases),
        'audit_input_copies_excluded_as_identical_to_producer_wavs': len(cases),
        'binary_not_archived': 'measure_pcm; pre-run and retention hashes bind observed executable, not a reproducible build',
    }, indent=2) + '\n')
    for path in DESTINATION.rglob('*.json'):
        json.loads(path.read_text())
    for path in DESTINATION.rglob('*.py'):
        ast.parse(path.read_text(), filename=str(path))
    paths = sorted(p for p in DESTINATION.rglob('*') if p.is_file())
    (DESTINATION / 'SHA256SUMS').write_text(''.join(f'{sha(p)}  {p.relative_to(DESTINATION)}\n' for p in paths))
    print(json.dumps({'files': len(paths) + 1, 'bytes': sum(p.stat().st_size for p in paths),
                      'manifest_sha256': sha(DESTINATION / 'SHA256SUMS'), 'pre_run_checks': checks}))


if __name__ == '__main__':
    main()
