from pathlib import Path
import gzip
import hashlib
import json
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
evidence = repo / 'tools/media-qualification/evidence/2026-10-11-youtube-network'
evidence.mkdir(exist_ok=True)
compression = []

def retain(source, relative):
    data = source.read_bytes()
    compressed = len(data) > 16000
    target = evidence / (str(relative) + ('.gz' if compressed else ''))
    target.parent.mkdir(parents=True, exist_ok=True)
    if compressed:
        compression.append(dict(path=str(target.relative_to(evidence)), original_bytes=len(data),
                                original_sha256=hashlib.sha256(data).hexdigest()))
        target.write_bytes(gzip.compress(data, mtime=0))
    else:
        target.write_bytes(data)

originals = root / 'packaged-4k/Charge.deadpan/Media/Originals'
media = {path.name: dict(bytes=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest())
         for path in originals.iterdir() if path.is_file()}
(root / 'packaged-4k/original-identity.json').write_text(json.dumps(media, indent=2) + '\n')
for source in sorted(root.iterdir()):
    if source.is_file() and source.suffix in ['.json', '.log', '.py']:
        retain(source, Path('checks' if source.suffix != '.py' else 'scripts') / source.name)
for folder in ['packaged-4k', 'connect-fixture', 'file-url-probe', 'chaos']:
    for source in sorted((root / folder).iterdir()):
        if source.is_file() and source.suffix in ['.json', '.jsonl', '.stderr', '.log', '.bin']:
            retain(source, Path(folder) / source.name)
bundle = Path('/tmp/deadpan-youtube-network-bundle-20261011')
for source in [bundle / 'Deadpan.release.json', bundle / 'Deadpan.sbom.cdx.json',
               bundle / 'Deadpan.app/Contents/Resources/build-provenance.json']:
    retain(source, Path('bundle') / source.name)
files = json.loads((root / 'reviewed-source.json').read_text())
for path, digest in files.items():
    assert hashlib.sha256((repo / path).read_bytes()).hexdigest() == digest, path
identity = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
                files=files,
                review='Sole-agent review of the complete diff; no subagent or live GUI used.',
                os=subprocess.check_output(['sw_vers'], text=True),
                cpu=subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(),
                memory_bytes=int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)),
                binaries={path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                          for path in (bundle / 'Deadpan.app/Contents/MacOS').iterdir() if path.is_file()})
(evidence / 'source-identity.json').write_text(json.dumps(identity, indent=2) + '\n')
(evidence / 'compression.json').write_text(json.dumps(compression, indent=2) + '\n')
for row in compression:
    data = gzip.decompress((evidence / row['path']).read_bytes())
    assert len(data) == row['original_bytes']
    assert hashlib.sha256(data).hexdigest() == row['original_sha256']
(evidence / 'SHA256SUMS').write_text(''.join(
    hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + str(path.relative_to(evidence)) + '\n'
    for path in sorted(evidence.rglob('*')) if path.is_file() and path.name != 'SHA256SUMS'))
for line in (evidence / 'SHA256SUMS').read_text().splitlines():
    digest, name = line.split('  ', 1)
    assert hashlib.sha256((evidence / name).read_bytes()).hexdigest() == digest
print('Collected and verified', sum(p.is_file() for p in evidence.rglob('*')), 'files')
