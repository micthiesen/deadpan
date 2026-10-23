from pathlib import Path
import subprocess,time,json,hashlib
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22');p=s/'next-profile-plan'
names=['capture-profile','early-fallback','binding-defaults-v2']
# Compilation completion receipts are required before any timing.
deadline=time.monotonic()+900
while True:
 done=[(p/'builds'/(n+'-build.json')).is_file()for n in names+['binding-defaults']]
 if all(done):break
 if time.monotonic()>deadline:raise RuntimeError('build receipt wait expired; inspect live build owners, do not assume terminal')
 time.sleep(5)
control=json.loads((s/'binaries.json').read_text())['candidate']
rows=[]
for name in names:
 d=json.loads((p/'builds'/(name+'-build.json')).read_text())
 assert d['exit']==0 and d.get('binary'),d
 binaries={'baseline':{'source_path':control['source_path'],'executed_path':control['executed_path'],'sha256':control['sha256']},'candidate':{'source_path':str(Path(d['target'])/'release/examples/perf'),'executed_path':d['binary']['path'],'sha256':d['binary']['sha256']}}
 (p/(name+'-binaries.json')).write_text(json.dumps(binaries,indent=2)+'\n')
 (p/(name+'-source.json')).write_text(json.dumps({'shipping_source_commit':'6916ee0d578a11c37e6e66a470eb430b02773764','experiment':d,'control_is_current_qualified_perf':True,'not_shipping_or_target_pass':True},indent=2)+'\n')
 start=time.monotonic()
 with(p/(name+'-measurement.stdout')).open('w')as out,(p/(name+'-measurement.stderr')).open('w')as err:
  r=subprocess.run(['python',str(p/(name+'-measure.py'))],stdout=out,stderr=err)
 rows.append({'experiment':name,'exit':r.returncode,'seconds':time.monotonic()-start})
 (p/'measurement-matrix.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
