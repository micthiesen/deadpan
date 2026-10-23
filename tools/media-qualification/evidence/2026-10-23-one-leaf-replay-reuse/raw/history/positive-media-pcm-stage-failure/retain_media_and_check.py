from pathlib import Path,PurePosixPath
import hashlib,json,subprocess,tarfile,time,gzip,io
s=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23')
e=s/'retained-media'
dest=s/'owner-portable-check';dest.mkdir(exist_ok=False)
manifest=json.loads((s/'release-media-binaries.json').read_text());cli=manifest['helpers']['deadpan-cli'];binary=Path(cli['path'])
def sha(data):return hashlib.sha256(data).hexdigest()
assert sha(binary.read_bytes())==cli['sha256']
def write(path,data):path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
rows=[]
for phase,label in [('debug-media','neutral'),('release-media','neutral'),('release-media','aligned')]:
 row=json.loads((s/f'{phase}-{label}-fixture.json').read_text())
 package=Path(row['package']);movie=Path(row['movie']);root=package.parent
 target=e/'media'/phase/label;target.mkdir(parents=True,exist_ok=True)
 write(target/'summary.json',json.dumps(row,indent=2).encode()+b'\n')
 # Preserve every external recipe, control, trace and canonical numerical document.
 for path in root.rglob('*'):
  if not path.is_file()or any(p.endswith('.deadpan')for p in path.relative_to(root).parts):continue
  if path.suffix not in ['.json','.txt']:continue
  data=path.read_bytes();relative=Path('fixture')/path.relative_to(root)
  if len(data)>100000:write(target/(str(relative)+'.gz'),gzip.compress(data,mtime=0))
  else:write(target/relative,data)
 write(target/movie.name,movie.read_bytes())
 expected={}
 tarbytes=io.BytesIO()
 with tarfile.open(fileobj=tarbytes,mode='w')as archive:
  for path in sorted(package.rglob('*')):
   assert not path.is_symlink(),path
   if not path.is_file():continue
   name=str(Path(package.name)/path.relative_to(package));data=path.read_bytes()
   expected[name]={'sha256':sha(data),'size':len(data)}
   member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o600;archive.addfile(member,io.BytesIO(data))
 archive_data=gzip.compress(tarbytes.getvalue(),mtime=0);archive_path=target/'portable-package.tar.gz'
 write(archive_path,archive_data)
 metadata={'archive_path':str(archive_path.relative_to(e)),'archive_sha256':sha(archive_data),'regular_files':[{'path':n,**v}for n,v in expected.items()]}
 write(target/'portable-package-checksums.json',json.dumps(metadata,indent=2).encode()+b'\n')
 if phase!='release-media':continue
 outroot=dest/label;outroot.mkdir()
 with tarfile.open(fileobj=io.BytesIO(archive_data),mode='r:gz')as archive:
  members=archive.getmembers();assert {m.name for m in members}==set(expected)
  for member in members:
   name=PurePosixPath(member.name);assert member.isfile()and not name.is_absolute()and '..'not in name.parts
   data=archive.extractfile(member).read()
   assert sha(data)==expected[member.name]['sha256']and len(data)==expected[member.name]['size']
   write(outroot.joinpath(*name.parts),data)
 def snapshot():return {str(p.relative_to(outroot)):{'sha256':sha(p.read_bytes()),'size':p.stat().st_size}for p in outroot.rglob('*')if p.is_file()}
 assert snapshot()==expected
 owned=outroot/package.name;commands=[]
 for name,args in [('validate',['project','validate',str(owned)]),('doctor',['doctor','--project',str(owned)])]:
  start=time.monotonic()
  with(dest/f'{label}-{name}.stdout.json').open('w')as out,(dest/f'{label}-{name}.stderr.txt').open('w')as err:
   result=subprocess.run([str(binary),*args],stdout=out,stderr=err)
  assert result.returncode==0,(label,name,result.returncode)
  assert sha(binary.read_bytes())==cli['sha256']and snapshot()==expected
  commands.append({'command':[str(binary),*args],'exit':result.returncode,'seconds':time.monotonic()-start,'package_paths_and_bytes_unchanged':True})
 rows.append({'label':label,'package':str(owned),'regular_files':len(expected),'archive_sha256':metadata['archive_sha256'],'commands':commands})
receipt={'executed_cli':cli,'safe_relative_regular_files_only':True,'cases':rows}
write(dest/'receipt.json',json.dumps(receipt,indent=2).encode()+b'\n')
for path in dest.iterdir():
 if path.is_file():write(e/'owner-portable-check'/path.name,path.read_bytes())
# The positive parity case has two separately verified outputs, not a single-movie schema.
parity=json.loads((s/'release-media-one-leaf-fixture.json').read_text())
assert parity['schema']=='deadpan-one-leaf-media-parity-1'
assert parity['capability_present'] and parity['callback_counts']==[1,1]
assert parity['pictures']==45 and parity['samples']==72072 and parity['channels']==2
assert all(parity['canonical_comparison'][key] is True for key in [
 'document_bytes_equal','commit_undo_redo_bytes_equal','register_banks_equal',
 'authored_limited_and_limiter_gain_bits_equal','partition_and_cold_reads_equal',
 'source_pcm_nonzero','pause_interior_silent'])
assert parity['history']['undo_redo_reopen_checked'] and parity['history']['full_replay_checked']
assert parity['history']['receipt_shortcut_used'] is False
assert not parity['independent_DSP_claim'] and not parity['listening_claim'] and not parity['compressed_byte_identity_claim']
root=Path(parity['root']);assert root==Path(parity['evidence_dir']) and root.is_dir()
target=e/'media/release-media/one-leaf';target.mkdir(parents=True,exist_ok=False)
write(target/'summary.json',json.dumps(parity,indent=2).encode()+b'\n')
for path in root.rglob('*'):
 if not path.is_file() or any(part.endswith('.deadpan') for part in path.relative_to(root).parts):continue
 if path.suffix not in ['.json','.txt']:continue
 data=path.read_bytes();relative=Path('fixture')/path.relative_to(root)
 write(target/(str(relative)+'.gz') if len(data)>100000 else target/relative,
       gzip.compress(data,mtime=0) if len(data)>100000 else data)
assert [leg['leg'] for leg in parity['exports']]==['cached','uncached']
for leg in parity['exports']:
 report=leg['verification']
 assert report['summary']['pictures_checked']==45 and report['summary']['pictures_failed']==0 and report['summary']['nonzero_offsets']==0
 assert len(report['audio'])==1
 audio=report['audio'][0]
 assert audio['project_samples']==[0,72072] and audio['flags']==[] and audio['uncovered_samples']==0
 assert audio['blocks']['low_snr_blocks']==0 and audio['offset_status']=='verified_zero' and audio['measured_offset_samples']==0
 movie=Path(leg['movie']);assert movie.is_file()
 write(target/leg['leg']/movie.name,movie.read_bytes())
 package=Path(leg['package']);assert package.is_dir() and package.suffix=='.deadpan'
 expected={};archive_bytes=io.BytesIO()
 with tarfile.open(fileobj=archive_bytes,mode='w') as archive:
  for path in sorted(package.rglob('*')):
   assert not path.is_symlink(),path
   if not path.is_file():continue
   name=str(Path(package.name)/path.relative_to(package));data=path.read_bytes()
   expected[name]={'sha256':sha(data),'size':len(data)}
   member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o600;archive.addfile(member,io.BytesIO(data))
 archive_data=gzip.compress(archive_bytes.getvalue(),mtime=0)
 write(target/leg['leg']/'portable-package.tar.gz',archive_data)
 write(target/leg['leg']/'portable-package-checksums.json',json.dumps({
  'archive_sha256':sha(archive_data),'regular_files':[{'path':name,**v} for name,v in expected.items()]
 },indent=2).encode()+b'\n')
print(json.dumps({'one_leaf_exports_retained':2,'pictures_each':45,'samples_each':72072,'canonical_history_parity':True}))
subprocess.run(['python',str(s/'guard.py'),'release-media'],check=True)
print(json.dumps(receipt,indent=2))
