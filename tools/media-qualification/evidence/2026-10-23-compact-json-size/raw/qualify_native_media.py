from pathlib import Path
import os,subprocess,time,json,hashlib,shutil,sys
s=Path('/tmp/deadpan-compact-json-size-qualification-2026-10-23');phase=sys.argv[1];mode=sys.argv[2]
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_REQUIRE_PHYSICAL_MAC='1',DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def guard():subprocess.run(['python',str(s/'guard.py'),phase],check=True)
def run(name,args,extra=None):
 start=time.monotonic()
 with(s/(phase+'-'+name+'.log')).open('w')as log:r=subprocess.run(args,env=dict(env,**(extra or {})),stdout=log,stderr=subprocess.STDOUT)
 row={'command':args,'exit':r.returncode,'seconds':time.monotonic()-start}
 (s/(phase+'-'+name+'-receipt.json')).write_text(json.dumps(row,indent=2)+'\n');print(name,json.dumps(row),flush=True);guard()
 if r.returncode:raise SystemExit(r.returncode)
subprocess.run(['python',str(s/'freeze.py'),phase],check=True)
if mode=='native':
 run('build',['cargo','build','--locked','-p','deadpan-app','--release','--features','ui-harness','--bin','deadpan-app'])
 bins={}
 for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
  source=Path('target/release')/name;dest=s/(phase+'-bin')/name;dest.parent.mkdir(exist_ok=True);shutil.copy2(source,dest)
  bins[name]={'source':str(source),'executed':str(dest),'sha256':sha(dest),'freshly_built':name=='deadpan-app'}
 (s/(phase+'-binaries.json')).write_text(json.dumps(bins,indent=2)+'\n');guard()
 kestrel=Path('/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift')
 (s/(phase+'-kestrel.json')).write_text(json.dumps({'path':str(kestrel),'sha256':sha(kestrel)},indent=2)+'\n')
 lines=subprocess.check_output(['ps','-axo','pid,ppid,etime,pcpu,comm'],text=True).splitlines()
 conflicts=[l for l in lines if any(n in l.split()[-1].lower() for n in ['cargo','rustc','deadpan'])]
 (s/(phase+'-pre-performance-processes.json')).write_text(json.dumps(conflicts,indent=2)+'\n')
 assert not conflicts,conflicts
 (s/(phase+'-power.txt')).write_text(subprocess.check_output(['pmset','-g','batt'],text=True))
 run('performance',[bins['deadpan-app']['executed'],'--ui-check','--scenario','sound-group-endpoints','--mode','performance','--output',str(s/(phase+'-performance')),'--kestrel-source',str(kestrel)])
 for r in bins.values():assert sha(Path(r['executed']))==r['sha256']
else:
 release=mode=='release-media'
 archive=s/(phase+'.tar.zst');target=s/(phase+'-extracted');target.mkdir(exist_ok=True)
 args=['cargo','nextest','archive','--locked','-p','deadpan-cli','--test','preview_export','--features','synthetic-worker','--archive-file',str(archive),'--zstd-level=-1']
 if release:args.insert(3,'--release')
 run('build',args)
 reuse=['--archive-file',str(archive),'--extract-to',str(target),'--extract-overwrite','--workspace-remap',str(Path.cwd())]
 with(s/(phase+'-list.json')).open('w')as out,(s/(phase+'-list.log')).open('w')as err:
  subprocess.run(['cargo','nextest','list',*reuse,'--message-format','json'],env=env,stdout=out,stderr=err,check=True)
 binaries={row['binary-path']:sha(Path(row['binary-path'])) for row in json.loads((s/(phase+'-list.json')).read_text())['rust-suites'].values()}
 helpers={};profile='release'if release else'debug'
 for name in ['deadpan-cli','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
  source=Path('target')/profile/name;matches=list(target.rglob(name))
  if not matches:
   dest=target/'target'/profile/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest);matches=[dest]
  assert len(matches)==1 and sha(matches[0])==sha(source),(name,matches)
  helpers[name]={'source':str(source),'path':str(matches[0]),'sha256':sha(matches[0])}
 (s/(phase+'-binaries.json')).write_text(json.dumps({'tests':binaries,'helpers':helpers},indent=2)+'\n');guard()
 trials=[('neutral','test(neutral_unpaired_sound_group_ungroup_preserves_complete_pcm_and_verified_export)')]
 if release:trials.append(('aligned','test(aligned_sound_scope_group_split_move_copy_preserves_canonical_pcm_and_verified_export)'))
 for label,expression in trials:
  needle=expression.removeprefix('test(').removesuffix(')')
  listed=json.loads((s/(phase+'-list.json')).read_text())['rust-suites']
  assert sum(needle in name for suite in listed.values()for name in suite['testcases'])==1,expression
  run(label,['cargo','nextest','run',*reuse,'-E',expression,'--run-ignored','all','--test-threads','1','--no-fail-fast'],{'DEADPAN_PREVIEW_EXPORT_KEEP':'1','DEADPAN_PREVIEW_EXPORT_RESULTS':str(s/(phase+'-'+label+'-fixture.json'))})
  for path,digest in binaries.items():assert sha(Path(path))==digest
  for row in helpers.values():assert sha(Path(row['path']))==row['sha256'] and sha(Path(row['source']))==row['sha256']
