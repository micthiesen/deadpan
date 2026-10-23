from pathlib import Path,PurePosixPath
import hashlib,json,subprocess,tarfile,time,gzip,io
s=Path('/tmp/deadpan-compact-json-size-qualification-2026-10-23')
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
subprocess.run(['python',str(s/'guard.py'),'release-media'],check=True)
print(json.dumps(receipt,indent=2))
