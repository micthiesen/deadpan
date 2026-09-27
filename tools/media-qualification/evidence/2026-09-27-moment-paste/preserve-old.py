import hashlib,json,os,shutil,subprocess
from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-moment-paste-20260926')
old=scratch/'old-bin'
old.mkdir(exist_ok=False)
binary=old/'deadpan-core26-cli'
shutil.copy2(repo/'target/debug/deadpan-cli',binary)
env=dict(os.environ,PATH=str(old)+os.pathsep+os.environ['PATH'])
doctor=json.loads(subprocess.check_output(['deadpan-core26-cli','doctor'],env=env,cwd=scratch))
assert (doctor['document_schema'],doctor['database_schema'])==(26,32)
(scratch/'old-binary.json').write_text(json.dumps(dict(binary=str(binary),
    sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),doctor=doctor,
    source_manifest=str(scratch/'baseline.json'),
    source_matches_prior_verified_gate=True),indent=2)+'\n')
print(json.dumps(dict(binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),document_schema=26,database_schema=32)))
