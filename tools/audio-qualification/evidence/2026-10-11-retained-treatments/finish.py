import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

root=Path('/Users/michael/Code/deadpan')
out=Path(__file__).parent
env=os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX']='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
records=json.loads((out/'commands.json').read_text())
for name,argv in [
    ('final-fmt',['cargo','fmt','--all']),
    ('final-owner-tests',['cargo','test','--locked','-p','deadpan-audio','--lib','owner_input']),
    ('final-fmt-check',['cargo','fmt','--all','--','--check']),
    ('final-whitespace',['git','diff','--check']),
]:
    now=datetime.datetime.now(datetime.timezone.utc).isoformat()
    start=time.monotonic()
    print(name,flush=True)
    with (out/(name+'.log')).open('wb') as log:
        result=subprocess.run(argv,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    records.append(dict(name=name,argv=argv,started_utc=now,seconds=time.monotonic()-start,exit_code=result.returncode))
    (out/'commands.json').write_text(json.dumps(records,indent=2)+'\n')
    if result.returncode:
        print((out/(name+'.log')).read_text(),flush=True)
        raise SystemExit(result.returncode)
identity=json.loads((out/'identity.json').read_text())
identity['sources']={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in identity['sources']}
identity['validation_note']='The complete plan/audio suites passed. The final seven owner-input audio tests also cover a subsequently added direct retained silent-Hold assertion; workspace Clippy included that assertion. Final formatting changes no behavior.'
(out/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
print('Final retained input checks passed',flush=True)
