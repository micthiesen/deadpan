from pathlib import Path
import os, json, hashlib, subprocess, time, re, statistics

s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
subprocess.run(['python',str(s/'qualify_ui_lint.py')],check=True)
out=s/'controlled';out.mkdir(exist_ok=False)
binaries=json.loads((s/'binaries.json').read_text())
rows=[];trials=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def run(name,cmd):
 started=time.monotonic()
 with(out/(name+'.stdout')).open('w')as stdout,(out/(name+'.stderr')).open('w')as stderr:
  result=subprocess.run(cmd,cwd=s,stdout=stdout,stderr=stderr)
 row={'name':name,'command':cmd,'exit':result.returncode,'seconds':time.monotonic()-started}
 rows.append(row);(out/'commands.json').write_text(json.dumps(rows,indent=2)+'\n')
 assert result.returncode==0,row
 return row
keys=['core_capture_for_insertion_ms','core_resume_owner_loop_ms','core_new_binding_validation_ms','core_leaf_insert_ms','core_reducer_total_ms','core_net_patch_and_result_validation_ms','core_apply_validated_total_ms','store_prepare_command_ms','store_write_plan_ms','store_sqlite_commit_ms','perf_refresh_ms','perf_commit_plus_refresh_ms']
for i in range(5):
 for mode in (['baseline','candidate']if i%2==0 else['candidate','baseline']):
  name=f'{mode}-{i+1}';binary=Path(binaries[mode]['executed_path']);package=out/(name+'.deadpan')
  assert sha(binary)==binaries[mode]['sha256']
  run(name+'-make',[str(binary),'make-large',str(package),'--beats','10000'])
  waits=[];started=time.monotonic()
  while os.getloadavg()[0]>3 and time.monotonic()-started<180:
   waits.append({'after_seconds':time.monotonic()-started,'load':os.getloadavg()});time.sleep(5)
  load=os.getloadavg()
  (out/(name+'-pre-edit.json')).write_text(json.dumps({'load_at_edit_start':load,'load_above_three':load[0]>3,'waits':waits},indent=2)+'\n')
  run(name+'-edit',[str(binary),'edit',str(package),'--cycles','2','--kinds','pause'])
  d=json.loads((out/(name+'-edit.stdout')).read_text())
  assert d['initial']['nodes']==10001 and d['insert_pause']['committed']==2 and d['insert_pause']['refused']==0
  assert len(d['insert_pause']['raw_total_ms'])==2
  phase=[];anomalies=[]
  if False:
   trace=(out/(name+'-edit.stderr')).read_text()
   assert trace.count('DP24 measure_begin')==trace.count('DP24 measure_end')==2
   for ordinal,part in enumerate(trace.split('DP24 measure_begin')[1:]):
    segment=part.split('DP24 measure_end',1)[0]
    values={key:[float(x)for x in re.findall(r'\b'+re.escape(key)+r'=([0-9.]+)',segment)]for key in keys}
    counts={key:len(value)for key,value in values.items()}
    valid=all(n==1 for n in counts.values())
    if not valid:anomalies.append({'edit':ordinal+1,'counts':counts})
    phase.append({'edit':ordinal+1,'unique_expected_phases':valid,'values':values,'shifted_suffix_owners':[int(x)for x in re.findall(r'\bowners=([0-9]+)',segment)],'lines':[l for l in segment.splitlines()if l.startswith('DP24 ')]})
  trial={'mode':mode,'trial':i+1,'load_at_edit_start':load,'load_above_three':load[0]>3,'first_total_ms':d['insert_pause']['raw_total_ms'][0],'second_total_ms':d['insert_pause']['raw_total_ms'][1],'initial':d['initial'],'final':d['final'],'phase':phase,'path_anomalies':anomalies}
  trials.append(trial);(out/'trials.json').write_text(json.dumps(trials,indent=2)+'\n')
  assert sha(binary)==binaries[mode]['sha256']
  print(json.dumps(trial),flush=True)
result={}
for mode in binaries:
 values=[t for t in trials if t['mode']==mode]
 result[mode]={label:{'n':len(values),'min_ms':min(t[key]for t in values),'median_ms':statistics.median(t[key]for t in values),'max_ms':max(t[key]for t in values)}for label,key in [('first','first_total_ms'),('second','second_total_ms')]}
summary={'fresh_packages':True,'phase_multiplicity_check':'not_applicable_uninstrumented_binaries','source':json.loads((s/'source.json').read_text()),'binaries':binaries,'load_flags':[t['mode']+'-'+str(t['trial'])for t in trials if t['load_above_three']],'path_anomalies':[{'mode':t['mode'],'trial':t['trial'],'anomalies':t['path_anomalies']}for t in trials if t['path_anomalies']],'summary':result,'no_p95_or_target_pass_claim':True}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary),flush=True)
