"""Run exact Cargo-recorded worker and example artifacts with an outer deadline."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parent
name, build_name = sys.argv[1:]
records=[]
for line in (root/(build_name+'.log')).read_text().splitlines():
    try:
        records.append(json.loads(line))
    except json.JSONDecodeError:
        pass
artifacts={}
for target, kind in [('deadpan-cli','bin'),('qualify_project_picture','example')]:
    matches=[row for row in records if row.get('reason')=='compiler-artifact'
             and row.get('target',{}).get('name')==target
             and row.get('target',{}).get('kind')==[kind] and row.get('executable')]
    assert len(matches)==1, (target,len(matches))
    row=matches[0]
    path=Path(row['executable']).resolve()
    artifacts[target]={'cargo_artifact':row,'path':str(path),
                       'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
binary=Path(artifacts['qualify_project_picture']['path'])
env=os.environ | {'PATH':str(binary.parent)+os.pathsep+os.environ['PATH']}
assert Path(shutil.which(binary.name,path=env['PATH'])).resolve()==binary
command=[binary.name,str(root/(name+'-report.json')),str(root/(name+'-frames')),
         '/tmp/deadpan-generated-pictures-smyys035/fixture/accepted.deadpan',
         artifacts['deadpan-cli']['path']]
receipt={'artifacts':artifacts,'command':command,'cwd':str(Path.cwd()),'timeout_seconds':360}
path=root/(name+'-artifact.json')
with path.open('x') as f:
    json.dump(receipt,f,indent=2)
    f.write('\n')
try:
    result=subprocess.run(command,env=env,timeout=360)
    receipt['exit_code']=result.returncode
except subprocess.TimeoutExpired:
    receipt['timed_out']=True
    receipt['exit_code']=124
for record in artifacts.values():
    record['sha256_after']=hashlib.sha256(Path(record['path']).read_bytes()).hexdigest()
    record['unchanged']=record['sha256_after']==record['sha256']
path.write_text(json.dumps(receipt,indent=2)+'\n')
assert all(record['unchanged'] for record in artifacts.values())
sys.exit(receipt['exit_code'])
