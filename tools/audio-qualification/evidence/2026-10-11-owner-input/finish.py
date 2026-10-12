import hashlib
import json
import subprocess
from pathlib import Path
import datetime
import time

root = Path('/Users/michael/Code/deadpan')
out = Path(__file__).parent
records = json.loads((out/'commands.json').read_text())
for name, argv in [
    ('final-fmt', ['cargo', 'fmt', '--all']),
    ('final-owner-tests', ['cargo', 'test', '--locked', '-p', 'deadpan-plan', '--lib', 'audio_owners::tests::input']),
    ('final-fmt-check', ['cargo', 'fmt', '--all', '--', '--check']),
    ('final-whitespace', ['git', 'diff', '--check']),
]:
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    print(name, flush=True)
    with (out/(name+'.log')).open('wb') as log:
        result = subprocess.run(argv, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    records.append(dict(name=name, argv=argv, started_utc=started, seconds=time.monotonic()-clock, exit_code=result.returncode))
    (out/'commands.json').write_text(json.dumps(records, indent=2)+'\n')
    if result.returncode:
        print((out/(name+'.log')).read_text())
        raise SystemExit(result.returncode)
identity = json.loads((out/'identity.json').read_text())
identity['sources'] = {p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in identity['sources']}
identity['validation_note'] = 'Full suites passed before adding the PointCeil regression and documentation/private-field refinements; the final six owner-input plan tests pass after those changes. Workspace Clippy included the added regression. Final formatting changes no behavior.'
(out/'identity.json').write_text(json.dumps(identity, indent=2)+'\n')
print('Final owner tests, formatting and whitespace pass', flush=True)
