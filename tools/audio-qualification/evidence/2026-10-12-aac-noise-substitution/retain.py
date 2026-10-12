import gzip,hashlib,json,pathlib,re,shutil,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
out=repo/'tools/audio-qualification/evidence/2026-10-12-aac-noise-substitution'
out.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(name,value): (out/name).write_text(json.dumps(value,indent=2)+'\n')
records=[]
for path in sorted(s.glob('*.json')):
 value=json.loads(path.read_text())
 if isinstance(value,dict) and 'log_sha256' in value:
  assert sha(s/(value['name']+'.log'))==value['log_sha256']
  records.append(value)
write('commands.json',records)
for pattern in ['*.log','*.decoded']:
 for path in s.glob(pattern): (out/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
base=subprocess.check_output(['git','rev-parse','83af1774'],cwd=repo,text=True).strip()
changed=subprocess.check_output(['git','diff','--name-only',base,'--','crates','native'],cwd=repo,text=True).splitlines()
untracked=subprocess.check_output(['git','ls-files','--others','--exclude-standard','--','crates','native'],cwd=repo,text=True).splitlines()
sources=sorted(set(changed+untracked+['Cargo.lock','rust-toolchain.toml','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']))
write('identity.json',dict(base_commit=base,sources={p:sha(repo/p) for p in sources},old_binary_sha256=sha(s/'old-deadpan-cli'),final_release_binaries={p:sha(repo/p) for p in ['target/release/deadpan-cli','target/release/deadpan-app']}))
patch=subprocess.check_output(['git','diff','--binary',base,'--','crates','native'],cwd=repo)
for path in untracked:
 result=subprocess.run(['git','diff','--no-index','--binary','--','/dev/null',path],cwd=repo,capture_output=True)
 assert result.returncode==1
 patch+=result.stdout
(out/'implementation.patch').write_bytes(patch)
counts={}
for name in ['native-tests2','cli-encoded','jobs-admission','release-native','release-preview2']:
 entries=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(s/(name+'.log')).read_text())]
 counts[name]=dict(passed=sum(v[0] for v in entries),failed=sum(v[1] for v in entries),ignored=sum(v[2] for v in entries),result_groups=len(entries))
write('test-counts.json',counts)
for name in ['README.md','review.md','run.py','checks.py','finish-checks.py','retain.py','check-evidence.py','capture.py','probe.c','probe.py','qualify-final.py','reference.json','reproduction.json','codec-identity.json','environment.json','recipe-coverage.json']:
 shutil.copyfile(s/name,out/name)
for name in ['probe-results.json','final-result.json','old-verify-report.json','partial-repeat-verify-report.json','nested-eq-verify-report.json','release-preview2-result.json']:
 (out/(name+'.gz')).write_bytes(gzip.compress((s/name).read_bytes(),mtime=0))
fixture=json.loads((s/'reference.json').read_text())
shutil.make_archive(str(out/'project-current'),'zip',root_dir=pathlib.Path(fixture['package']).parent)
previous=repo/'tools/audio-qualification/evidence/2026-10-12-partial-repeat-sounds'
assert sha(pathlib.Path(fixture['movie']))==sha(previous/'partial-repeat-sounds.mp4')
shutil.copyfile(previous/'project.zip',out/'project-old.zip')
shutil.copyfile(previous/'partial-repeat-sounds.mp4',out/'old-partial-repeat.mp4')
final=json.loads((s/'final-result.json').read_text())
for name,row in final['reports'].items(): shutil.copyfile(row['movie'],out/(name+'.mp4'))
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files')
