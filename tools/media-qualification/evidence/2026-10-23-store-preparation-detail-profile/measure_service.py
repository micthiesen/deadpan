from pathlib import Path
import os,subprocess,time,json,hashlib,statistics
s=Path('/tmp/deadpan-store-preparation-detail-profile-2026-10-23')
h=Path('/tmp/deadpan-compact-json-size-qualification-2026-10-23')
(s/'measurement-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__))},indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(root):return {str(p.relative_to(root)):{'bytes':p.stat().st_size,'sha256':sha(p)}for p in sorted(root.rglob('*'))if p.is_file()}
source=s/'source'
identities=json.loads((s/'instrumented-source-identities.json').read_text())
inputs=json.loads((s/'input-identities.json').read_text())
def guard():
 assert identities=={str(p.relative_to(source)):sha(p)for p in source.rglob('*.rs')}
 for b in bins.values():assert sha(Path(b['executed']))==b['sha256']
 assert all(sha(source/p)==v for p,v in inputs.items())
bins=json.loads((s/'binaries.json').read_text())
rootbin=json.loads((h/'release-native-binaries.json').read_text())['deadpan-app']
assert rootbin['sha256']=='c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037'
bins['root-control']=rootbin
perf=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
assert sha(perf)=='fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
rows=[];commands=[]
def run(name,cmd,env):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:
  r=subprocess.run(cmd,cwd=source,env=env,stdout=out,stderr=err)
 commands.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'measurement-commands.json').write_text(json.dumps(commands,indent=2)+'\n')
 if r.returncode:raise RuntimeError(commands[-1])
env=dict(os.environ);env.pop('DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE',None)
for round,order in enumerate([['root-control','off','on'],['off','on','root-control'],['on','root-control','off']],1):
 for kind in order:
  guard();name=f'{round}-{kind}'
  fixture=s/(name+'.deadpan')
  run(name+'-make',[str(perf),'make-large',str(fixture),'--beats','10000'],env)
  before=snapshot(fixture);(s/(name+'-input-before.json')).write_text(json.dumps(before,indent=2)+'\n')
  waits=[];start=time.monotonic()
  while os.getloadavg()[0]>3 and time.monotonic()-start<180:
   waits.append({'seconds':time.monotonic()-start,'load':os.getloadavg()});time.sleep(5)
  load=os.getloadavg()
  (s/(name+'-pre-run.json')).write_text(json.dumps({'load':load,'waits':waits},indent=2)+'\n')
  assert load[0]<=3,load
  b=bins['root-control']if kind=='root-control'else bins['deadpan-app']
  e=dict(env)
  if kind=='on':e['DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE']='1'
  output=s/(name+'-report')
  run(name,[b['executed'],'--ui-check','--mode','performance','--scenario','large-first-pause','--project',str(fixture),'--output',str(output)],e)
  after=snapshot(fixture);(s/(name+'-input-after.json')).write_text(json.dumps(after,indent=2)+'\n')
  exclude={'project.sqlite-shm','project.sqlite-wal'}
  assert {k:v for k,v in before.items()if k not in exclude}=={k:v for k,v in after.items()if k not in exclude}
  assert after.get('project.sqlite-wal',{}).get('bytes',0)==0
  report=json.loads((output/'report.json').read_text());assert not report['failed']
  assert report['metadata']['binary_sha256']==b['sha256']
  assert report['metadata']['build_profile']=='release'
  assert report['metadata']['cargo_lock_sha256_at_run']==inputs['Cargo.lock']
  kestrel=next(x for x in report['scenarios']if x['name']=='kestrel-shortcuts')
  assert len(kestrel['checks'])==1 and all(c['passed']for c in kestrel['checks'])
  assert report['metadata']['fixture_sha256']==before['project.sqlite']['sha256']==after['project.sqlite']['sha256']
  case=next(x for x in report['scenarios']if x['name']=='large-first-pause')
  assert len(case['checks'])==7 and all(c['passed']for c in case['checks'])
  metrics={m['name']:m['summary']['completed']['p50_ms']for m in case['timings']if m['name'].startswith('large_first_pause')}
  phase=None
  if kind=='on':
   run(name+'-parse',['python',str(s/'check_profile.py'),str(s/(name+'.stderr'))],env)
   phase=json.loads((s/(name+'-parse.stdout')).read_text())
  else:assert 'DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE 'not in(s/(name+'.stderr')).read_text()
  guard()
  rows.append({'round':round,'kind':kind,'load':load,'metrics':metrics,'profile':phase,'checks':7,'kestrel_checks':1,'source_and_binary_guards':True,'input_guards':True,'authored_input_unchanged':True,'wal_empty':True,'binary_sha256':b['sha256'],'fixture_main_sha256':before['project.sqlite']['sha256']})
  (s/'service-trials.json').write_text(json.dumps(rows,indent=2)+'\n')
  print(json.dumps({k:v for k,v in rows[-1].items()if k!='profile'}),flush=True)
summary={'diagnostic_only':True,'no_percentile_or_target_pass':True,'n_per_kind':3,'kinds':{}}
for kind in ['root-control','off','on']:
 rr=[r for r in rows if r['kind']==kind]
 summary['kinds'][kind]={metric:{'min':min(v),'median':statistics.median(v),'max':max(v)}for metric in rr[0]['metrics']for v in [[r['metrics'][metric]for r in rr]]}
profiles=[r['profile']['nanoseconds']for r in rows if r['kind']=='on']
summary['phase_ms']={phase:{'min':min(v)/1e6,'median':statistics.median(v)/1e6,'max':max(v)/1e6}for phase in profiles[0]for v in [[r[phase]for r in profiles]]}
residuals=[r['profile']['residual_nanoseconds']for r in rows if r['kind']=='on']
summary['residual_ms']={phase:{'min':min(v)/1e6,'median':statistics.median(v)/1e6,'max':max(v)/1e6}for phase in residuals[0]for v in [[r[phase]for r in residuals]]}
summary['instrumentation_overhead_unqualified']=True
summary['no_production_optimization_in_this_profile']=True
guard()
(s/'service-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'done':True,'trials':len(rows),'n_per_kind':3,'diagnostic_only':True}),flush=True)
