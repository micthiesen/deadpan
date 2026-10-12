import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

root = Path('/Users/michael/Code/deadpan')
out = Path(__file__).parent
identity = json.loads((out/'identity.json').read_text())

def verify_source():
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest() == digest, name

verify_source()
env = os.environ.copy()
env.update(DEADPAN_FFMPEG_PREFIX=identity['ffmpeg_prefix'],
           DEADPAN_PREVIEW_EXPORT_KEEP='1',
           DEADPAN_PREVIEW_EXPORT_RESULTS=str(out/'export.json'))
argv = ['cargo', 'test', '--release', '--locked', '-p', 'deadpan-cli',
        '--test', 'preview_export',
        'stored_equalizer_changes_preview_and_matches_verified_export',
        '--', '--exact', '--nocapture']
now = datetime.datetime.now(datetime.timezone.utc).isoformat()
started = time.monotonic()
with (out/'release.log').open('wb') as log:
    result = subprocess.run(argv, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(argv=argv, environment={k: env[k] for k in [
    'DEADPAN_FFMPEG_PREFIX', 'DEADPAN_PREVIEW_EXPORT_KEEP', 'DEADPAN_PREVIEW_EXPORT_RESULTS']},
    started_utc=now, seconds=time.monotonic()-started, exit_code=result.returncode)
(out/'release-command.json').write_text(json.dumps(record,indent=2)+'\n')
print((out/'release.log').read_text()[-5000:],flush=True)
if result.returncode:
    sys.exit(result.returncode)
verify_source()
report = json.loads((out/'export.json').read_text())
paths = {'target/release/deadpan-cli'}
paths.update(re.findall(r'\((target/release/[^ )]+)\)', (out/'release.log').read_text()))
pins = dict(binaries={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sorted(paths)},
            source_identity_sha256=hashlib.sha256((out/'identity.json').read_bytes()).hexdigest(),
            fixture_sha256=hashlib.sha256((root/'native/deadpan-source/tests/fixtures/cfr-bframes.mp4').read_bytes()).hexdigest(),
            movie_sha256=hashlib.sha256(Path(report['movie']).read_bytes()).hexdigest())
(out/'release-identity.json').write_text(json.dumps(pins,indent=2)+'\n')
