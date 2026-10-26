#!/usr/bin/env python3
"""Exercise fixture authoring only using the already-qualified old bundle."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid

ROOT=Path(__file__).resolve().parent
OUT=ROOT/'schema-validation-v2'
OUT.mkdir(exist_ok=False)
CLI=Path('/tmp/deadpan-native-trust-2026-10-25/bundle-local-second/Deadpan.app/Contents/MacOS/deadpan-cli')
ENV={'HOME':str(ROOT/'home'),'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','TMPDIR':'/tmp'}
PROJECT=OUT/'Fixture.deadpan'
records=[]

def run(name,args):
    command=[str(CLI)]+[str(x) for x in args]
    started=time.monotonic()
    result=subprocess.run(command,env=ENV,capture_output=True,timeout=120)
    (OUT/(name+'.stdout')).write_bytes(result.stdout)
    (OUT/(name+'.stderr')).write_bytes(result.stderr)
    records.append({'name':name,'command':command,'exit_code':result.returncode,
                    'seconds':time.monotonic()-started})
    (OUT/'results.json').write_text(json.dumps({'cli':str(CLI),
        'cli_sha256':hashlib.sha256(CLI.read_bytes()).hexdigest(),'environment':ENV,
        'records':records,'inference_or_ai_commands_run':False},indent=2)+'\n')
    assert result.returncode==0,(name,result.returncode,result.stderr)
    return result

run('create-original',['project','create-original',PROJECT,ROOT/'cfr-bframes.mp4'])
document=json.loads(run('original',['project','dump',PROJECT,'--json']).stdout)
assert 'document' not in document and 'nodes' in document
revision=str(uuid.uuid4())
request={'protocol':1,'project_id':document['project_id'],
    'expected_revision':document['revision_id'],'new_revision':revision,
    'command':{'command':'insert_time','at':60,
        'hold':{'duration':30,'video':{'type':'background'},'audio':{'type':'silence'}},
        'id':'ai-trust-hold','identities':{'nodes':['ai-trust-split-'+str(i) for i in range(16)]},
        'timing':{'allocation':revision,'ordinal':0}}}
path=OUT/'insert-hold.json'
path.write_text(json.dumps(request,indent=2)+'\n')
run('insert-hold-preview',['command',PROJECT,'--json',path,'--dry-run'])
assert json.loads(run('after-preview',['project','dump',PROJECT,'--json']).stdout)==document
run('insert-hold',['command',PROJECT,'--json',path])
fallback=json.loads(run('fallback',['project','dump',PROJECT,'--json']).stdout)
assert fallback['nodes']['ai-trust-hold']['kind']['recipe']==request['command']['hold']
assert fallback['revision_id']==revision
run('validate',['project','validate',PROJECT,'--quick'])
print(json.dumps({'status':'passed','output':str(OUT),
    'rate':document['presentation_basis']['frame_rate'],
    'hold':fallback['nodes']['ai-trust-hold'],'revision':revision},indent=2))
