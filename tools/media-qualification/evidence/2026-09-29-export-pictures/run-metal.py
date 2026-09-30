"""Run the Cargo-recorded example under an external deadline."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parent
name = sys.argv[1]
build_name = sys.argv[2]
rows=[]
for line in (root/(build_name+'.log')).read_text().splitlines():
    try:
        row=json.loads(line)
    except json.JSONDecodeError:
        continue
    if (row.get('reason')=='compiler-artifact'
        and row.get('target',{}).get('name')=='qualify_project_picture'
        and row.get('target',{}).get('kind')==['example']
        and row.get('executable')):
        rows.append(row)
assert len(rows)==1, len(rows)
artifact=rows[0]
binary=Path(artifact['executable']).resolve()
env=os.environ | {'PATH':str(binary.parent)+os.pathsep+os.environ['PATH']}
assert Path(shutil.which(binary.name,path=env['PATH'])).resolve()==binary
before=hashlib.sha256(binary.read_bytes()).hexdigest()
command=[binary.name,str(root/(name+'-report.json')),str(root/(name+'-frames')),
         '/tmp/deadpan-generated-pictures-smyys035/fixture/accepted.deadpan']
receipt={'cargo_artifact':artifact,'sha256':before,'command':command,'cwd':str(Path.cwd()),
         'timeout_seconds':240,'resolved_executable':str(binary)}
path=root/(name+'-artifact.json')
with path.open('x') as f:
    json.dump(receipt,f,indent=2)
    f.write('\n')
try:
    result=subprocess.run(command,env=env,timeout=240)
    receipt['exit_code']=result.returncode
except subprocess.TimeoutExpired:
    receipt['timed_out']=True
    receipt['exit_code']=124
receipt['sha256_after']=hashlib.sha256(binary.read_bytes()).hexdigest()
receipt['binary_unchanged']=receipt['sha256_after']==before
path.write_text(json.dumps(receipt,indent=2)+'\n')
assert receipt['binary_unchanged']
sys.exit(receipt['exit_code'])
