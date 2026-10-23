from pathlib import Path
import os,subprocess,json,time,hashlib,shutil
s=Path('/tmp/deadpan-large-pause-harness-2026-10-23')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(root):return {str(p.relative_to(root)):{'bytes':p.stat().st_size,'sha256':sha(p)}for p in sorted(root.rglob('*'))if p.is_file()}
(s/'final-driver-owner.json').write_text(json.dumps({'pid':os.getpid(),'started_unix':time.time(),'script':str(Path(__file__)),'stdout_stderr_redirected':True},indent=2)+'\n')
rows=[]
def run(name,cmd):
 start=time.monotonic()
 with(s/(name+'.stdout')).open('w')as stdout,(s/(name+'.stderr')).open('w')as stderr:r=subprocess.run(cmd,env=env,stdout=stdout,stderr=stderr)
 rows.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':time.monotonic()-start})
 (s/'final-qualification-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
run('final-all-ui',['python',str(s/'qualify_all_ui.py'),'candidate-all-ui-final','candidate','ui'])
run('final-strict',['python',str(s/'ui_lint_final.py')])
run('final-freeze',['python',str(s/'freeze.py'),'release-final'])
run('final-build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app'])
run('final-build-guard',['python',str(s/'guard.py'),'release-final'])
bins={}
for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
 source=Path('target/release')/name;dest=s/'release-final-bin'/name;dest.parent.mkdir(exist_ok=True)
 shutil.copy2(source,dest);bins[name]={'source':str(source),'executed':str(dest),'sha256':sha(dest),'freshly_built':name=='deadpan-app'}
(s/'release-final-binaries.json').write_text(json.dumps(bins,indent=2)+'\n')
perf=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
assert sha(perf)=='fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
fixture=s/'final-diagnostic-input.deadpan'
run('final-make',[str(perf),'make-large',str(fixture),'--beats','10000'])
before=snapshot(fixture);(s/'final-input-before.json').write_text(json.dumps(before,indent=2)+'\n')
waits=[];start=time.monotonic()
while os.getloadavg()[0]>3 and time.monotonic()-start<180:
 waits.append({'seconds':time.monotonic()-start,'load':os.getloadavg()});time.sleep(5)
load=os.getloadavg()
(s/'final-pre-run.json').write_text(json.dumps({'load_at_start':load,'load_above_three':load[0]>3,'waits':waits},indent=2)+'\n')
assert load[0]<=3,load
run('final-pilot',[bins['deadpan-app']['executed'],'--ui-check','--mode','performance','--scenario','large-first-pause','--project',str(fixture),'--output',str(s/'final-diagnostic-report')])
after=snapshot(fixture);(s/'final-input-after.json').write_text(json.dumps(after,indent=2)+'\n')
coordination={'project.sqlite-shm','project.sqlite-wal'}
assert {n:r for n,r in before.items()if n not in coordination}=={n:r for n,r in after.items()if n not in coordination}
assert after.get('project.sqlite-wal',{}).get('bytes',0)==0
assert all(sha(Path(row['executed']))==row['sha256']for row in bins.values())
report=json.loads((s/'final-diagnostic-report/report.json').read_text());assert report['failed']is False
assert report['metadata']['fixture_sha256']==before['project.sqlite']['sha256']==after['project.sqlite']['sha256']
case=next(r for r in report['scenarios']if r['name']=='large-first-pause')
summary={'n':1,'diagnostic_only':True,'no_budget_or_percentile_pass':True,'checks':len(case['checks']),'failed':case['failed'],'metrics':{r['name']:r['summary']for r in case['timings']if r['name'].startswith('large_first_pause')},'load_at_start':load,'all_authored_input_paths_and_bytes_unchanged':True,'empty_wal':True,'sqlite_coordination_sidecars_observed':{n:r for n,r in after.items()if n in coordination},'binary':bins['deadpan-app']}
(s/'final-diagnostic-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
run('final-source-guard',['python',str(s/'guard.py'),'release-final'])
print(json.dumps(summary),flush=True)
