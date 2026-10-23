from pathlib import Path
import os,json,subprocess,time,hashlib,statistics
s=Path('/tmp/deadpan-reused-leaf-result-2026-10-23')
q=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
(s/'measure-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'started_unix':time.time()},indent=2)+'\n')
source=s/'source'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
manifest=json.loads((s/'instrumented-source-identities.json').read_text())
(s/'build-source-identities.json').write_text(json.dumps(manifest,indent=2)+'\n')
bins=json.loads((s/'binaries.json').read_text())
control=json.loads((q/'release-native-corrected-binaries.json').read_text())['deadpan-app']
prototype=bins['deadpan-app']
perf=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
assert sha(perf)=='fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
def guard():
 assert manifest=={str(p.relative_to(source)):sha(p)for p in source.rglob('*.rs')}
 for b in list(bins.values())+[control]:assert sha(Path(b['executed']))==b['sha256']
def files(p):return {str(f.relative_to(p)):{'sha256':sha(f),'bytes':f.stat().st_size}for f in sorted(p.rglob('*'))if f.is_file()}
commands=[];rows=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:r=subprocess.run(cmd,cwd=source,stdout=out,stderr=err)
 commands.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
 if r.returncode:raise SystemExit(r.returncode)
for pair in range(1,6):
 for kind in (['control','prototype']if pair%2 else['prototype','control']):
  guard();name=f'{pair}-{kind}';fixture=s/(name+'.deadpan')
  run(name+'-make',[str(perf),'make-large',str(fixture),'--beats','10000'])
  before=files(fixture);(s/(name+'-input-before.json')).write_text(json.dumps(before,indent=2)+'\n')
  waits=[];start=time.monotonic()
  while os.getloadavg()[0]>3 and time.monotonic()-start<240:
   waits.append({'seconds':time.monotonic()-start,'load':os.getloadavg()});time.sleep(5)
  load=os.getloadavg();(s/(name+'-pre-run.json')).write_text(json.dumps({'load':load,'waits':waits},indent=2)+'\n')
  assert load[0]<=3,load
  b=control if kind=='control'else prototype;out=s/(name+'-report')
  run(name,[b['executed'],'--ui-check','--mode','performance','--scenario','large-first-pause','--project',str(fixture),'--output',str(out)])
  after=files(fixture);(s/(name+'-input-after.json')).write_text(json.dumps(after,indent=2)+'\n')
  exclude={'project.sqlite-shm','project.sqlite-wal'}
  assert {k:v for k,v in before.items()if k not in exclude}=={k:v for k,v in after.items()if k not in exclude}
  assert after.get('project.sqlite-wal',{}).get('bytes',0)==0
  report=json.loads((out/'report.json').read_text());assert not report['failed']
  assert report['metadata']['fixture_sha256']==before['project.sqlite']['sha256']==after['project.sqlite']['sha256']
  case=next(c for c in report['scenarios']if c['name']=='large-first-pause')
  assert len(case['checks'])==7 and all(c['passed']for c in case['checks'])
  metrics={m['name']:m['summary']['completed']['p50_ms']for m in case['timings']if m['name'].startswith('large_first_pause')}
  row={'pair':pair,'kind':kind,'metrics':metrics,'load':load,'checks':7,'source_and_binary_guards':True,'authored_inputs_unchanged':True,'empty_wal':True,'binary_sha256':b['sha256']}
  guard();rows.append(row);(s/'trials.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
summary={'n_per_kind':5,'prototype_only':True,'no_percentile_or_target_pass':True,'kinds':{}}
for kind in ['control','prototype']:
 rr=[r for r in rows if r['kind']==kind]
 summary['kinds'][kind]={m:{'min':min(v),'median':statistics.median(v),'max':max(v)}for m in rr[0]['metrics']for v in [[r['metrics'][m]for r in rr]]}
(s/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
