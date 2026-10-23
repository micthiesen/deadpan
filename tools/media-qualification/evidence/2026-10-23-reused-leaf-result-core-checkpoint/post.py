from pathlib import Path
import subprocess,os,json,time,hashlib,statistics
s=Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
h=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
(s/'post-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'waiting_for':'qualification eight successful steps'},indent=2)+'\n')
while True:
 try:r=json.loads((s/'qualification-receipt.json').read_text())
 except(FileNotFoundError,json.JSONDecodeError):r=[]
 if any(x['exit']!=0 for x in r):raise SystemExit('qualification failed; no post steps run')
 if len(r)==8:break
 time.sleep(2)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(p):return {str(f.relative_to(p)):{'bytes':f.stat().st_size,'sha256':sha(f)}for f in sorted(p.rglob('*'))if f.is_file()}
commands=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:r=subprocess.run(cmd,stdout=out,stderr=err)
 commands.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'post-receipt.json').write_text(json.dumps(commands,indent=2)+'\n')
 print(json.dumps(commands[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
# Require the added Core differential case in the complete archived suite.
needle='interior_pause_and_audio_setter_match_legacy_leaf_patches_with_bound_audio_and_marks'
phase='candidate-all-core'
listed=json.loads((s/(phase+'-list.json')).read_text())['rust-suites']
matches=[case for suite in listed.values()for name,case in suite['testcases'].items()if name.endswith('::'+needle) or name==needle]
assert len(matches)==1 and matches[0]['filter-match']['status']=='matches'
log=(s/(phase+'-run.log')).read_text()
assert any('PASS'in line and needle in line for line in log.splitlines())
run('portable',['python',str(s/'retain_media_and_check.py')])
bins=json.loads((s/'release-native-binaries.json').read_text())
candidate=bins['deadpan-app']
control=json.loads((h/'release-native-corrected-binaries.json').read_text())['deadpan-app']
def guard():
 subprocess.run(['python',str(s/'guard.py'),'release-native'],stdout=subprocess.DEVNULL,check=True)
 for b in list(bins.values())+[control]:assert sha(Path(b['executed']))==b['sha256']
kestrel='/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'
native=[]
for name in ['macros','named-registers','dot-repeat','range-set-selectors','transcript-search-selectors']:
 guard()
 output=s/('native-'+name)
 run('native-'+name,[candidate['executed'],'--ui-check','--mode','performance','--scenario',name,'--output',str(output),'--kestrel-source',kestrel])
 report=json.loads((output/'report.json').read_text());assert not report['failed']
 native.append({'scenario':name,'checks':sum(len(c['checks'])for c in report['scenarios']),'findings':sum(len(c['findings'])for c in report['scenarios']),'failed':False})
 guard()
(s/'extra-native-summary.json').write_text(json.dumps(native,indent=2)+'\n')
perf=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
assert sha(perf)=='fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
trials=[]
for pair in range(1,6):
 for kind in (['control','candidate']if pair%2 else['candidate','control']):
  guard();name=f'pair-{pair}-{kind}';fixture=s/(name+'.deadpan')
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
  trials.append(row);(s/'pause-trials.json').write_text(json.dumps(trials,indent=2)+'\n');guard();print(json.dumps(row),flush=True)
summary={'n_per_kind':5,'diagnostic_only':True,'no_percentile_or_target_pass':True,'kinds':{}}
for kind in ['control','candidate']:
 rr=[r for r in trials if r['kind']==kind]
 summary['kinds'][kind]={m:{'min':min(v),'median':statistics.median(v),'max':max(v)}for m in rr[0]['metrics']for v in [[r['metrics'][m]for r in rr]]}
(s/'pause-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
