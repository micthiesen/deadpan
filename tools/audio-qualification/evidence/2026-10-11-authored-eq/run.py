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
records = []
for name, argv in [
    ('fmt', ['cargo', 'fmt', '--all', '--', '--check']),
    ('tests', ['cargo', 'test', '--locked', '-p', 'deadpan-core', '-p', 'deadpan-plan', '-p', 'deadpan-audio']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('whitespace', ['git', 'diff', '--check']),
]:
    print('Running ' + name, flush=True)
    start = time.monotonic()
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (out/(name+'.log')).open('wb') as log:
        result = subprocess.run(argv, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    records.append(dict(name=name, argv=argv, started_utc=now, seconds=time.monotonic()-start, exit_code=result.returncode))
    (out/'commands.json').write_text(json.dumps(records,indent=2)+'\n')
    log = (out/(name+'.log')).read_text()
    if name == 'tests':
        print('Passed tests: '+str(sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log)))),flush=True)
    print(name+': '+str(result.returncode),flush=True)
    if result.returncode:
        print(log[-12000:],flush=True)
        sys.exit(result.returncode)
paths = subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()
paths += subprocess.check_output(['git','ls-files','--others','--exclude-standard','crates'],cwd=root,text=True).splitlines()
paths = [p for p in paths if p.startswith('crates/')]
paths += ['Cargo.lock','rust-toolchain.toml','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']
identity = dict(
    base_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
    rustc=subprocess.check_output(['rustc','-Vv'],cwd=root,text=True),
    macos=subprocess.check_output(['sw_vers'],text=True),
    hardware=subprocess.check_output(['sysctl','-n','hw.model','hw.memsize','hw.physicalcpu','machdep.cpu.brand_string'],text=True),
    ffmpeg_prefix=env['DEADPAN_FFMPEG_PREFIX'],
    sources={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sorted(set(paths))},
    test_binaries={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sorted(set(re.findall(r'\((target/debug/[^ )]+)\)', (out/'tests.log').read_text())))},
)
(out/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
print('All final-source checks passed',flush=True)
