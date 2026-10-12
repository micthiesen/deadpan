import gzip,hashlib,json,pathlib,re,shutil,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
out=repo/'tools/audio-qualification/evidence/2026-10-12-partial-repeat-sounds'
out.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(name,value): (out/name).write_text(json.dumps(value,indent=2)+'\n')
records=[]
for path in sorted(s.glob('*.json')):
 try: value=json.loads(path.read_text())
 except json.JSONDecodeError: continue
 if isinstance(value,dict) and 'log_sha256' in value:
  assert sha(s/(value['name']+'.log'))==value['log_sha256']
  records.append(value)
write('commands.json',records)
for path in s.glob('*.log'):
 (out/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
base=subprocess.check_output(['git','rev-parse','414b9d6b'],cwd=repo,text=True).strip()
changed=subprocess.check_output(['git','diff','--name-only',base,'--','crates'],cwd=repo,text=True).splitlines()
untracked=[p for p in subprocess.check_output(['git','ls-files','--others','--exclude-standard','--','crates'],cwd=repo,text=True).splitlines() if p.endswith('.rs')]
sources=sorted(set(changed+untracked+['Cargo.lock','rust-toolchain.toml','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']))
write('identity.json',dict(base_commit=base,sources={p:sha(repo/p) for p in sources},final_release_binaries={p:sha(repo/p) for p in ['target/release/deadpan-cli','target/release/deadpan-app']}))
patch=subprocess.check_output(['git','diff',base,'--','crates'],cwd=repo)
for path in untracked:
 result=subprocess.run(['git','diff','--no-index','--','/dev/null',path],cwd=repo,capture_output=True)
 assert result.returncode==1
 patch+=result.stdout
(out/'implementation.patch').write_bytes(patch)
counts={}
for name in ['core-plan','audio-clocks','store-clocks','release-audio']:
 data=(s/(name+'.log')).read_text()
 entries=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',data)]
 counts[name]=dict(passed=sum(v[0] for v in entries),failed=sum(v[1] for v in entries),ignored=sum(v[2] for v in entries),result_groups=len(entries))
write('test-counts.json',counts)
for name in ['README.md','review.md','run.py','checks.py','retain.py','check-evidence.py','baseline-final.py','baseline-final-request.json','debug-export-result.json','release-export-result.json','environment.json','nested-eq-reproduction.patch']:
 shutil.copyfile(s/name,out/name)
(out/'baseline-final-result.json.gz').write_bytes(gzip.compress((s/'baseline-final-result.json').read_bytes(),mtime=0))
report=json.loads((s/'release-export-result.json').read_text())
shutil.make_archive(str(out/'project'),'zip',root_dir=pathlib.Path(report['package']).parent)
shutil.copyfile(report['movie'],out/'partial-repeat-sounds.mp4')
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files')
