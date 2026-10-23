from pathlib import Path
import subprocess,os,time,json,hashlib,sys,shutil
s=Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
phase,location,kind=sys.argv[1:]
b=s/'baseline' if location=='baseline' else Path('/Users/michael/Code/deadpan')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def sources():return {p:sha(b/p)for p in subprocess.check_output(['rg','--files','-g','*.rs'],cwd=b,text=True).splitlines()}
frozen=sources();(s/(phase+'-source-identities.json')).write_text(json.dumps(frozen,indent=2)+'\n')
inputs={p:sha(b/p)for p in ['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md']}
(s/(phase+'-inputs.json')).write_text(json.dumps(inputs,indent=2)+'\n')
base='ae5eedd065b851242917b38a428ee40a3588978d'
baseline_identities=json.loads((s/'qualified-control-source-identities.json').read_text())
baseline_inputs=json.loads((s/'qualified-control-inputs.json').read_text())
expected_new='crates/deadpan-core/tests/semantic_refresh.rs'
expected_changed={'crates/deadpan-core/src/semantic/planner/selection.rs',expected_new}
assert len(baseline_identities)==2050 and len(frozen)==2051
assert set(frozen)==set(baseline_identities)|{expected_new}
assert {name for name in frozen if frozen[name]!=baseline_identities.get(name)}==expected_changed
assert inputs==baseline_inputs
for name,digest in frozen.items():
 if baseline_identities.get(name)!=digest:
  target=s/(phase+'-overlays')/(name+'.txt');target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes((b/name).read_bytes())
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',DEADPAN_SCOPE_PROBE_EVIDENCE=str(s/(phase+'-evidence')),DEADPAN_REQUIRE_PHYSICAL_MAC='1',DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
def guard():
 assert sources()==frozen and all(sha(b/p)==v for p,v in inputs.items()),phase
def run(label,args):
 start=time.monotonic()
 with(s/f'{phase}-{label}.log').open('w')as log:r=subprocess.run(args,cwd=b,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'command':args,'exit':r.returncode,'seconds':time.monotonic()-start}
 (s/f'{phase}-{label}-receipt.json').write_text(json.dumps(row,indent=2)+'\n')
 guard();print(label,json.dumps(row),flush=True)
 return r.returncode
if kind=='strict':
 code=run('check',['cargo','xtask','check','--since',base,'--no-test'])
 if code:raise SystemExit(code)
 code=run('format',['cargo','fmt','--all','--','--check'])
 raise SystemExit(code)
targets={'ui':['-p','deadpan-app','--features','ui-harness'],'pcm':['-p','deadpan-audio','--test','audio_definition'],'core':['-p','deadpan-core','--test','compact_audio_bindings'],'store':['-p','deadpan-store','--test','source_registration'],'allcore':['-p','deadpan-core'],'broad':[arg for p in ['deadpan-plan','deadpan-audio','deadpan-analysis','deadpan-store','deadpan-app','deadpan-cli']for arg in ['-p',p]]+['--features','deadpan-cli/synthetic-worker']}[kind]
expression='all()' if kind in ['allcore','ui'] else ('package(deadpan-plan) | package(deadpan-audio) | package(deadpan-analysis) | package(deadpan-store) | package(deadpan-app) | test(macros) | test(semantic) | test(mark) | test(project_commands) | test(selection) | test(nodes_cli) | test(ranges_cli) | test(transcript_search) | test(provider_clock_coverage) | binary(transcript_search) | binary(project_commands) | test(speech::) | test(sound) | test(group)' if kind=='broad' else 'all()')
archive=s/(phase+'.tar.zst');extract=s/(phase+'-extracted');extract.mkdir(exist_ok=True)
args=['cargo','nextest','archive','--locked',*targets]
if location=='baseline':args+=['--target-dir',str(s/'baseline-target')]
code=run('build',args+['--archive-file',str(archive),'--zstd-level=-1'])
if code:raise SystemExit(code)
reuse=['--archive-file',str(archive),'--extract-to',str(extract),'--extract-overwrite','--workspace-remap',str(b)]
with(s/(phase+'-list.json')).open('w')as out,(s/(phase+'-list.log')).open('w')as err:
 subprocess.run(['cargo','nextest','list',*reuse,'-E',expression,'--message-format','json'],cwd=b,env=env,stdout=out,stderr=err,check=True)
identities={}
for key,row in json.loads((s/(phase+'-list.json')).read_text())['rust-suites'].items():
 tests=[n for n,c in row.get('testcases',{}).items()if c.get('filter-match',{}).get('status')=='matches']
 if tests:identities[key]={'path':row['binary-path'],'sha256':sha(Path(row['binary-path'])),'tests':tests}
(s/(phase+'-binaries.json')).write_text(json.dumps(identities,indent=2)+'\n')
helpers={}
if kind in ['broad','ui']:
 for name in ['deadpan-app','deadpan-cli','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
  source=b/'target/debug'/name
  matches=list(extract.rglob(name))
  if not matches:
   dest=extract/'target/debug'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest);matches=[dest]
  assert len(matches)==1 and sha(matches[0])==sha(source),(name,matches)
  helpers[name]={'source':str(source),'path':str(matches[0]),'sha256':sha(source)}
 (s/(phase+'-helpers.json')).write_text(json.dumps(helpers,indent=2)+chr(10))
code=run('run',['cargo','nextest','run',*reuse,'-E',expression,'--test-threads','2' if kind=='pcm' else '8','--no-fail-fast'])
assert all(sha(Path(r['path']))==r['sha256']for r in identities.values())
assert all(sha(Path(r['path']))==r['sha256'] and sha(Path(r['source']))==r['sha256']for r in helpers.values())
guard()
result={'source_base':base,'source_location':location,'exit':code,'tests':sum(len(r['tests'])for r in identities.values()),'binaries':len(identities),'sources_inputs_executables_unchanged':True}
(s/(phase+'-result.json')).write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
raise SystemExit(code)
