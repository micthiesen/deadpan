from pathlib import Path
import subprocess,os,json,time,hashlib,statistics
s=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
h=Path('/tmp/deadpan-large-pause-harness-2026-10-23')
(s/'post-corrected-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'waiting_for':'corrected UI/lint/release four successful steps; prior portable receipt verified'},indent=2)+'\n')
while True:
 try:r=json.loads((s/'corrected-receipt.json').read_text())
 except(FileNotFoundError,json.JSONDecodeError):r=[]
 if any(x['exit']!=0 for x in r):raise SystemExit('qualification failed; no post steps run')
 if len(r)==4:break
 time.sleep(2)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(p):return {str(f.relative_to(p)):{'bytes':f.stat().st_size,'sha256':sha(f)}for f in sorted(p.rglob('*'))if f.is_file()}
commands=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:r=subprocess.run(cmd,stdout=out,stderr=err)
 commands.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'post-corrected-receipt.json').write_text(json.dumps(commands,indent=2)+'\n')
 print(json.dumps(commands[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
# The initial --lib filter selected zero cases; require the actual binary case.
needle='project::service::macros::tests::transactional_runtime_preparation_rejects_bad_capture_before_saving_the_bank'
for phase in ['candidate-broad','candidate-ui','candidate-ui-corrected']:
 listed=json.loads((s/(phase+'-list.json')).read_text())['rust-suites']
 matches=[case for suite in listed.values()for name,case in suite['testcases'].items()if name==needle]
 assert len(matches)==1 and matches[0]['filter-match']['status']=='matches',phase
 log=(s/(phase+'-run.log')).read_text()
 assert any('PASS'in line and needle in line for line in log.splitlines()),phase
portable=json.loads((s/'owner-portable-check/receipt.json').read_text());assert len(portable['cases'])==2 and all(c['exit']==0 and c['package_paths_and_bytes_unchanged']for case in portable['cases']for c in case['commands'])
bins=json.loads((s/'release-native-corrected-binaries.json').read_text())
candidate=bins['deadpan-app']
control=json.loads((h/'release-final-binaries.json').read_text())['deadpan-app']
def guard():
 subprocess.run(['python',str(s/'guard.py'),'release-native-corrected'],stdout=subprocess.DEVNULL,check=True)
 for b in list(bins.values())+[control]:assert sha(Path(b['executed']))==b['sha256']
kestrel='/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'
native=[]
for name in ['macros','named-registers','dot-repeat','range-set-selectors','transcript-search-selectors']:
 guard()
 output=s/('corrected-native-'+name)
 run('corrected-native-'+name,[candidate['executed'],'--ui-check','--mode','performance','--scenario',name,'--output',str(output),'--kestrel-source',kestrel])
 report=json.loads((output/'report.json').read_text());assert not report['failed']
 native.append({'scenario':name,'checks':sum(len(c['checks'])for c in report['scenarios']),'findings':sum(len(c['findings'])for c in report['scenarios']),'failed':False})
 guard()
(s/'extra-native-corrected-summary.json').write_text(json.dumps(native,indent=2)+'\n')
perf=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
assert sha(perf)=='fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
trials=[]
for pair in range(1,6):
 for kind in (['control','candidate']if pair%2 else['candidate','control']):
  guard();name=f'corrected-pair-{pair}-{kind}';fixture=s/(name+'.deadpan')
  run(name+'-make',[str(perf),'make-large',str(fixture),'--beats','10000'])
  before=snapshot(fixture);(s/(name+'-input-before.json')).write_text(json.dumps(before,indent=2)+'\n')
  waits=[];start=time.monotonic()
  while os.getloadavg()[0]>3 and time.monotonic()-start<240:
   waits.append({'seconds':time.monotonic()-start,'load':os.getloadavg()});time.sleep(5)
  load=os.getloadavg()
  (s/(name+'-pre-run.json')).write_text(json.dumps({'load':load,'waits':waits},indent=2)+'\n')
  assert load[0]<=3,load
  b=candidate if kind=='candidate'else control;output=s/(name+'-report')
  run(name,[b['executed'],'--ui-check','--mode','performance','--scenario','large-first-pause','--project',str(fixture),'--output',str(output)])
  after=snapshot(fixture);(s/(name+'-input-after.json')).write_text(json.dumps(after,indent=2)+'\n')
  excluded={'project.sqlite-shm','project.sqlite-wal'}
  assert {k:v for k,v in before.items()if k not in excluded}=={k:v for k,v in after.items()if k not in excluded}
  assert after.get('project.sqlite-wal',{}).get('bytes',0)==0
  report=json.loads((output/'report.json').read_text());assert not report['failed']
  assert report['metadata']['fixture_sha256']==before['project.sqlite']['sha256']==after['project.sqlite']['sha256']
  case=next(c for c in report['scenarios']if c['name']=='large-first-pause');assert len(case['checks'])==7 and all(c['passed']for c in case['checks'])
  metrics={t['name']:t['summary']['completed']['p50_ms']for t in case['timings']if t['name'].startswith('large_first_pause')}
  row={'pair':pair,'kind':kind,'metrics':metrics,'load':load,'authored_input_unchanged':True,'wal_empty':True,'source_binary_guards':True,'binary_sha256':b['sha256'],'checks':7}
  trials.append(row);(s/'pause-trials-corrected.json').write_text(json.dumps(trials,indent=2)+'\n');guard();print(json.dumps(row),flush=True)
summary={'n_per_kind':5,'diagnostic_only':True,'no_percentile_or_target_pass':True,'kinds':{}}
for kind in ['control','candidate']:
 rr=[r for r in trials if r['kind']==kind]
 summary['kinds'][kind]={m:{'min':min(v),'median':statistics.median(v),'max':max(v)}for m in rr[0]['metrics']for v in [[r['metrics'][m]for r in rr]]}
(s/'pause-summary-corrected.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
