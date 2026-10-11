from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

scratch = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
destination = repo / 'tools/media-qualification/evidence/2026-10-11-proxy-order'
destination.mkdir(exist_ok=True)
compressed = []

def retain(relative, target=None):
    source = scratch / relative
    output = destination / (target or relative)
    output.parent.mkdir(parents=True, exist_ok=True)
    raw = source.read_bytes()
    if output.suffix == '.gz':
        output.write_bytes(gzip.compress(raw, mtime=0))
        compressed.append({'path': str(output.relative_to(destination)),
                           'original_bytes': len(raw),
                           'original_sha256': hashlib.sha256(raw).hexdigest()})
    else:
        output.write_bytes(raw)

for name in ['environment.json', 'original-decode.json', 'checks.json',
             'followup-checks.json', 'make-retry-fixture.json',
             'make-retry-fixture.log']:
    retain(name)
for name in ['probe.py', 'stress.py', 'actual.py', 'capture.py', 'drain.c',
             'main.c', 'make_retry_fixture.py', 'check.py', 'followup.py', 'collect.py']:
    retain(name, 'scripts/' + name)
for name in ['format.log', 'clippy.log', 'proxy-tests.log', 'warning-followup.log',
             'retry-stability.log', 'fmt-check.log']:
    retain(name, 'checks/' + name)
for variant in ['', 'concurrent-320x180', 'actual-cfr', 'capture-first-order']:
    source = Path(variant)
    target = Path(variant or 'sequential')
    for name in ['identity.json', 'compile.log']:
        retain(str(source / name), str(target / name))
    for name in ['results.json', 'instrumented.c']:
        retain(str(source / name), str(target / (name + '.gz')))
for trial in ['concurrent-320x180/trial-148', 'actual-cfr/trial-323',
              'capture-first-order/trial-041']:
    for source in sorted((scratch / trial).iterdir()):
        if source.is_file():
            retain(str(source.relative_to(scratch)))
retain('trial-000/trace.jsonl', 'sequential/sample-success-trace.jsonl.gz')
paths = ['native/deadpan-media-worker/src/converter.c',
         'native/deadpan-media-worker/tests/proxy_real_media.rs',
         'native/deadpan-media-worker/tests/fixtures/proxy/cfr-retry-success.mp4',
         'native/deadpan-media-worker/tests/fixtures/proxy/cfr-retry-success.json',
         'native/deadpan-source/tests/fixtures/cfr-bframes.mp4']
identity = {'base': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
            'note': 'Uncommitted test and fixture change on this base; no production source changed.',
            'files': {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in paths}}
(destination / 'source-identity.json').write_text(json.dumps(identity, indent=2) + '\n')
(destination / 'compressed-originals.json').write_text(json.dumps(compressed, indent=2) + '\n')
print(f'Retained {sum(p.is_file() for p in destination.rglob("*"))} files')
