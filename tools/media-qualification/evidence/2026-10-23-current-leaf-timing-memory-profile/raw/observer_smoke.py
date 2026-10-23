from pathlib import Path
import subprocess,time,json,hashlib
q=Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23');d=q/'observer-smoke';d.mkdir()
source=q/'memory_observer.rs';original=source.read_text();manifest=json.loads((q/'observer-binary.json').read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(source)==manifest['source_sha256'] and sha(Path(manifest['path']))==manifest['sha256']
rlib=Path(manifest['adapter_rlib']);assert sha(rlib)==manifest['adapter_rlib_sha256']
helper=d/'helper.rs.txt';helper.write_text('fn main() { let ms = std::env::args().nth(1).unwrap().parse::<u64>().unwrap(); std::thread::sleep(std::time::Duration::from_millis(ms)); }\\n')
rows=[]
def command(name,args):
 start=time.monotonic()
 with(d/(name+'.stdout')).open('w')as out,(d/(name+'.stderr')).open('w')as err:r=subprocess.run(args,cwd=q/'source',stdout=out,stderr=err)
 row={'name':name,'command':args,'exit':r.returncode,'seconds':time.monotonic()-start};rows.append(row)
 (d/'commands.json').write_text(json.dumps(rows,indent=2)+'\\n');return r.returncode
assert command('helper-build',['rustc','--edition','2024','--crate-name','observer_smoke_helper',str(helper),'-o',str(d/'helper')])==0
variants={'timeout':('const MAX_RUNTIME: Duration = Duration::from_secs(180);','const MAX_RUNTIME: Duration = Duration::from_millis(25);'),
 'sample-refusal':('let sample = deadpan_native_process::owned_child_memory(&child);','let sample = Err::<Option<deadpan_native_process::WorkerMemory>, _>(io::Error::other("injected owned-child sample refusal"));')}
variant_rows={}
for name,(before,after)in variants.items():
 assert original.count(before)==1
 text=original.replace(before,after);p=d/(name+'.rs.txt');p.write_text(text);exe=d/(name+'-observer')
 args=['rustc','--edition','2024','--crate-name','observer_smoke_'+name.replace('-','_'),'-C','opt-level=2','-C','lto=thin','-C','embed-bitcode=yes','-L','dependency='+str(rlib.parent),'--extern','deadpan_native_process='+str(rlib),str(p),'-o',str(exe)]
 assert command(name+'-build',args)==0
 variant_rows[name]={'source':str(p),'sha256':sha(p),'only_exact_replacement':{'before':before,'after':after},'binary':str(exe),'binary_sha256':sha(exe)}
cases=[
 ('natural',Path(manifest['path']),[str(d/'helper'),'80'],'exited',0,True),
 ('setup-failure',Path(manifest['path']),[str(d/'helper'),'80'],'setup_failed',1,False),
 ('spawn-failure',Path(manifest['path']),[str(d/'missing-executable')],'spawn_failed',1,False),
 ('timeout',Path(variant_rows['timeout']['binary']),[str(d/'helper'),'1000'],'runtime_bound',1,False),
 ('sample-refusal',Path(variant_rows['sample-refusal']['binary']),[str(d/'helper'),'1000'],'sample_error',1,False)]
results=[]
for name,exe,app,reason,exit_code,valid in cases:
 case=d/name;case.mkdir();receipt=case/'receipt.json';samples=case/'samples.json'
 if name=='setup-failure':samples=case/'absent-parent'/'samples.json'
 args=[str(exe),'--receipt',str(receipt),'--samples',str(samples),'--stdout',str(case/'app.stdout'),'--stderr',str(case/'app.stderr'),'--',*app]
 assert command(name,args)==exit_code
 r=json.loads(receipt.read_text());assert r['terminal']is True and r['reason']==reason and r['valid_memory_trial']is valid
 if reason in {'exited','runtime_bound','sample_error'}:
  assert r['group_teardown_confirmed'] and r['leader_exit_confirmed'] and r['reaped'] and r['reap_attempted'] and not r['cleanup_unconfirmed']
 if valid:assert r['sample_count']>0 and r['app_exit_code']==0 and r['errors']==[]
 if reason=='sample_error':assert r['sample_count']==0 and r['observed_max_phys_footprint_bytes']is None and any('injected owned-child sample refusal'in e for e in r['errors'])
 results.append({'case':name,'receipt':str(receipt),'expected_reason':reason,'expected_exit':exit_code,'expected_valid':valid,'passed':True})
assert sha(source)==manifest['source_sha256'] and sha(rlib)==manifest['adapter_rlib_sha256'] and sha(Path(manifest['path']))==manifest['sha256']
(d/'summary.json').write_text(json.dumps({'source_unchanged':True,'observer_binary_unchanged':True,'adapter_unchanged':True,'runtime_fault_variants_are_separate_binaries':variant_rows,'cases':results,'cases_passed':len(results),'limitations':['Fault builds change only the one retained expression/constant; they are not the memory-measurement executable.','Group-failure/leader-fallback and reap-error paths have source review only.','Sample refusal branch injected before adapter call; this does not claim a real libproc permission failure.']},indent=2)+'\\n')
print(json.dumps({'passed':len(results),'summary':str(d/'summary.json')}))
