from pathlib import Path
import os, json, hashlib, subprocess, time, shutil, re, statistics

s=Path('/tmp/deadpan-first-pause-profile-2026-10-22')
repo=Path('/Users/michael/Code/deadpan')
source=s/'source';profile=s/'instrumented'
overlay=Path('/tmp/deadpan-neutral-ungroup-2026-10-22/next-performance-map/overlay.py')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows=[]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def snapshot(root):
 names=subprocess.check_output(['rg','--files','-g','*.rs'],cwd=root,text=True).splitlines()
 return {n:sha(root/n)for n in names}
def run(name,cmd,cwd,extra=None):
 started=time.monotonic()
 with(s/(name+'.stdout')).open('w')as out,(s/(name+'.stderr')).open('w')as err:
  result=subprocess.run(cmd,cwd=cwd,env=dict(env,**(extra or {})),stdout=out,stderr=err)
 row={'name':name,'command':cmd,'cwd':str(cwd),'exit':result.returncode,'seconds':time.monotonic()-started}
 rows.append(row);(s/'commands.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(row),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
 return row

source.mkdir(exist_ok=False)
run('extract',['tar','-xf',str(s/'source.tar'),'-C',str(source)],s)
frozen=snapshot(repo);archived=snapshot(source)
assert frozen==archived,'current Rust sources differ from immutable archive'
inputs=['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml']
assert all(sha(repo/n)==sha(source/n)for n in inputs)
(s/'source-identities.json').write_text(json.dumps(archived,indent=2)+'\n')
(s/'inputs.json').write_text(json.dumps({n:sha(source/n)for n in inputs},indent=2)+'\n')
shutil.copy2(overlay,s/'overlay.py')
run('overlay',['python',str(s/'overlay.py'),'--source',str(source),'--dest',str(profile)],s)
instrumented=snapshot(profile)
changed=[n for n in archived if archived[n]!=instrumented[n]]
assert set(changed)=={'crates/deadpan-core/src/insert_time.rs','crates/deadpan-core/src/command.rs','crates/deadpan-store/src/lib.rs','crates/deadpan-cli/examples/perf/edit.rs'}
for name in changed:
 p=s/'overlays'/(name+'.txt');p.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(profile/name,p)
(s/'instrumented-identities.json').write_text(json.dumps(instrumented,indent=2)+'\n')
(s/'source.json').write_text(json.dumps({'commit':'a18ff65443aa8fd1aa5ab63fa33678c9afd88220','archive_sha256':sha(s/'source.tar'),'timer_overlay_sha256':sha(s/'overlay.py'),'modified_sources':changed},indent=2)+'\n')
run('baseline-build',['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'],repo)
run('instrumented-build',['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'],profile,{'CARGO_TARGET_DIR':str(s/'instrumented-target')})
assert snapshot(repo)==frozen and snapshot(source)==archived and snapshot(profile)==instrumented
binary_paths={'baseline':repo/'target/release/examples/perf','instrumented':s/'instrumented-target/release/examples/perf'}
binaries={}
for name,path in binary_paths.items():
 dest=s/(name+'-perf');shutil.copy2(path,dest);binaries[name]={'source_path':str(path),'executed_path':str(dest),'sha256':sha(dest)}
(s/'binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
(s/'power.txt').write_text(subprocess.check_output(['pmset','-g','batt'],text=True))
processes=subprocess.check_output(['ps','-axo','pid,ppid,etime,pcpu,comm'],text=True)
(s/'pre-performance-processes.txt').write_text(processes)
assert not any(any(n in l.split()[-1].lower()for n in ['cargo','rustc','deadpan'])for l in processes.splitlines()[1:]if l.split()),'concurrent build or Deadpan workload'
trials=[]
for i in range(5):
 for mode in ['baseline','instrumented']:
  name=f'{mode}-{i+1}';load=os.getloadavg()
  package=s/(name+'.deadpan');binary=Path(binaries[mode]['executed_path'])
  assert sha(binary)==binaries[mode]['sha256']
  run(name+'-make',[str(binary),'make-large',str(package),'--beats','10000'],s)
  run(name+'-edit',[str(binary),'edit',str(package),'--cycles','1','--kinds','pause'],s)
  d=json.loads((s/(name+'-edit.stdout')).read_text())
  assert d['initial']['nodes']==10001 and d['insert_pause']['committed']==1 and d['insert_pause']['refused']==0
  assert d['insert_pause']['total_ms']['n']==1
  assert sha(binary)==binaries[mode]['sha256']
  trace=(s/(name+'-edit.stderr')).read_text()
  measured=[]
  if mode=='instrumented':
   assert trace.count('DP24 measure_begin')==trace.count('DP24 measure_end')==1
   segment=trace.split('DP24 measure_begin',1)[1].split('DP24 measure_end',1)[0]
   measured=[l for l in segment.splitlines()if l.startswith('DP24 ')]
   assert any('core_reducer_total_ms='in l for l in measured),measured
  trial={'mode':mode,'trial':i+1,'load_at_start':load,'load_above_three':load[0]>3,'total_ms':d['insert_pause']['total_ms']['max'],'commit_ms':d['insert_pause']['store_commit_ms']['max'],'prepare_ms':d['insert_pause']['command_preparation_ms']['max'],'refresh_ms':d['insert_pause']['workspace_refresh_ms']['max'],'initial':d['initial'],'final':d['final'],'phase_lines':measured}
  trials.append(trial);(s/'trials.json').write_text(json.dumps(trials,indent=2)+'\n');print(json.dumps(trial),flush=True)
assert snapshot(repo)==frozen and snapshot(source)==archived and snapshot(profile)==instrumented
summary={mode:{'n':5,'min_ms':min(t['total_ms']for t in trials if t['mode']==mode),'median_ms':statistics.median(t['total_ms']for t in trials if t['mode']==mode),'max_ms':max(t['total_ms']for t in trials if t['mode']==mode)}for mode in binaries}
(s/'summary.json').write_text(json.dumps({'source_and_binary_guards_pass':True,'fresh_packages_only':True,'no_optimization_claim':True,'load_flags':[t['mode']+'-'+str(t['trial'])for t in trials if t['load_above_three']],'summary':summary},indent=2)+'\n')
print(json.dumps(summary),flush=True)
