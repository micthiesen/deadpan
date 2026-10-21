from pathlib import Path
import hashlib
import json
import shutil
import sys

scratch = Path('/tmp/deadpan-scoped-audio-setters-2026-10-21')
sources = json.loads((scratch / 'source-identities.json').read_text())
for source, expected in sources.items():
    actual = hashlib.sha256(Path(source).read_bytes()).hexdigest()
    assert actual == expected, (source, expected, actual)
if sys.argv[1:] == ['snapshot']:
    directory = scratch / 'bin'
    directory.mkdir()
    binaries = {}
    for name in ['deadpan-app', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
        source = Path('target/release') / name
        destination = directory / name
        shutil.copy2(source, destination)
        expected = hashlib.sha256(source.read_bytes()).hexdigest()
        assert hashlib.sha256(destination.read_bytes()).hexdigest() == expected
        binaries[name] = {'source': str(source), 'executed': str(destination), 'sha256': expected}
    (scratch / 'binaries.json').write_text(json.dumps(binaries, indent=2) + '\n')
print('Source identities verified' + ('; release binaries copied and hashed' if sys.argv[1:] == ['snapshot'] else ''))
