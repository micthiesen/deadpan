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
env = os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX'] = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
commands = [
    ('fmt', ['cargo', 'fmt', '--all', '--', '--check']),
    ('tests', ['cargo', 'test', '--locked', '-p', 'deadpan-plan', '-p', 'deadpan-audio']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('whitespace', ['git', 'diff', '--check']),
]
records = []
for name, argv in commands:
    print('Running ' + name, flush=True)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    with (out / (name + '.log')).open('wb') as log:
        result = subprocess.run(argv, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = dict(name=name, argv=argv, started_utc=started, seconds=time.monotonic()-clock, exit_code=result.returncode)
    records.append(record)
    (out / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
    text = (out / (name + '.log')).read_text()
    if name == 'tests':
        totals = [int(x) for x in re.findall(r'test result: ok\. (\d+) passed;', text)]
        print('Passed tests: ' + str(sum(totals)), flush=True)
    print(name + ': ' + str(result.returncode), flush=True)
    if result.returncode:
        print(text[-12000:], flush=True)
        sys.exit(result.returncode)
paths = subprocess.check_output(['git', 'diff', '--name-only'], cwd=root, text=True).splitlines()
paths += subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=root, text=True).splitlines()
paths = sorted(set(p for p in paths if p.startswith('crates/')))
paths += ['Cargo.lock', 'rust-toolchain.toml', 'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']
identity = dict(
    base_revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    rustc=subprocess.check_output(['rustc', '-Vv'], cwd=root, text=True),
    macos=subprocess.check_output(['sw_vers'], text=True),
    ffmpeg_prefix=env['DEADPAN_FFMPEG_PREFIX'],
    sources={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in paths},
)
(out/'identity.json').write_text(json.dumps(identity, indent=2)+'\n')
print('All checks passed', flush=True)
