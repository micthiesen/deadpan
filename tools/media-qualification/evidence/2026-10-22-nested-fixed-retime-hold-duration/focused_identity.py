from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-nested-fixed-retime-duration-2026-10-22')
for name,digest in json.loads((s/'focused-runtime-source-identities.json').read_text()).items():
 assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
listing=json.loads((s/'focused-list.json').read_text())
rows=[]
for name,suite in listing['rust-suites'].items():
 tests=[n for n,t in suite.get('testcases',{}).items() if t['filter-match']['status']=='matches']
 if not tests: continue
 p=Path(suite['binary-path']); t=s/'focused-bin'/p.name
 t.parent.mkdir(exist_ok=True); shutil.copy2(p,t)
 rows.append({'suite':name,'executed':str(p),'retained':str(t),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size,'tests':tests})
(s/'focused-binaries.json').write_text(json.dumps(rows,indent=2)+'\n')
print(sum(len(r['tests']) for r in rows),'tests in',len(rows),'hashed binaries; source identities unchanged')
