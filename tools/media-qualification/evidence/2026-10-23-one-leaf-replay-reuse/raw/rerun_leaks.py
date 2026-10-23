from pathlib import Path
import os,subprocess,time,json,re,hashlib
q=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
repo=Path('/Users/michael/Code/deadpan')
def read(p): return json.loads(p.read_text())
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
steps=read(q/'qualification-receipt.json');post=read(q/'post-receipt.json')
assert len(steps)==8 and all(r['exit']==0 for r in steps)
assert len(post)==26 and all(r['exit']==0 for r in post)
assert read(q/'pause-summary.json')['n_per_kind']==5
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',DEADPAN_REQUIRE_PHYSICAL_MAC='1',DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
cases=[]
for phase in ['candidate-all-core','candidate-broad','candidate-ui']:
 for line in (q/f'{phase}-run.log').read_text().splitlines():
  if not re.search(r'\bLEAK\b',line):continue
  match=re.match(r'^\s*LEAK\s+\[[^\]]+\]\s+\(\d+/\d+\)\s+(\S+)\s+(\S+)\s*$',line)
  assert match is not None,(phase,line)
  cases.append((phase,*match.groups()))
assert len(cases)==len(set(cases))
(q/'isolated-leak-owner.json').write_text(json.dumps({'pid':os.getpid(),'script':str(Path(__file__)),'cases':cases},indent=2)+'\n')
rows=[]
for index,(phase,binary,test) in enumerate(cases,1):
 subprocess.run(['python',str(q/'guard.py'),'release-native'],cwd=repo,check=True)
 suites=read(q/f'{phase}-list.json')['rust-suites']
 suite=suites[binary]
 assert suite['binary-id']==binary and test in suite['testcases']
 manifest=read(q/f'{phase}-binaries.json')[binary]
 assert sha(Path(manifest['path']))==manifest['sha256']
 expression=f"package(={suite['package-name']}) & binary(={suite['binary-name']}) & test(={test})"
 reuse=['--archive-file',str(q/f'{phase}.tar.zst'),'--extract-to',str(q/f'{phase}-extracted'),'--extract-overwrite','--workspace-remap',str(repo)]
 with(q/f'isolated-leak-{index}-list.json').open('w') as out,(q/f'isolated-leak-{index}-list.log').open('w') as err:
  subprocess.run(['cargo','nextest','list',*reuse,'-E',expression,'--message-format','json'],cwd=repo,env=env,stdout=out,stderr=err,check=True)
 selected=[(key,name) for key,r in read(q/f'isolated-leak-{index}-list.json')['rust-suites'].items() for name,c in r['testcases'].items() if c['filter-match']['status']=='matches']
 assert selected==[(binary,test)],selected
 command=['cargo','nextest','run',*reuse,'-E',expression,'--test-threads','1','--no-fail-fast']
 log_path=q/f'isolated-leak-{index}.log';start=time.monotonic()
 with log_path.open('w') as out:result=subprocess.run(command,cwd=repo,env=env,stdout=out,stderr=subprocess.STDOUT)
 text=log_path.read_text()
 leaked=bool(re.search(r'\bLEAK\b',text))
 passed_cases=[m.groups() for line in text.splitlines() if(m:=re.match(r'^\s*PASS\s+\[[^\]]+\]\s+\(\d+/\d+\)\s+(\S+)\s+(\S+)\s*$',line))]
 passed=result.returncode==0 and not leaked and passed_cases==[(binary,test)] and re.search(r'1 tests? run: 1 passed, \d+ skipped',text) is not None
 subprocess.run(['python',str(q/'guard.py'),'release-native'],cwd=repo,check=True)
 assert sha(Path(manifest['path']))==manifest['sha256']
 row={'phase':phase,'binary':binary,'test':test,'binary_id':suite['binary-id'],'binary_sha256':manifest['sha256'],'run_log':log_path.name,'log_sha256':sha(log_path),'command':command,'exit':result.returncode,'seconds':time.monotonic()-start,'passed':passed,'leaky':leaked,'source_binary_guards':True}
 rows.append(row);(q/'isolated-leak-reruns.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(json.dumps(row),flush=True)
 assert passed,row
print(json.dumps({'done':True,'isolated_cases':len(rows),'all_original_leak_cases_covered':True}),flush=True)
