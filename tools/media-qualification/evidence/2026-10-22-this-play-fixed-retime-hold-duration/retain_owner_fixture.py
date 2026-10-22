from pathlib import Path
import gzip,hashlib,io,json,tarfile
s=Path('/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22')
r=next(r for r in json.loads((s/'encoded-identities.json').read_text())['outputs'] if Path(r['package']).name=='this-play-fixed-unit-grow.deadpan')
package=Path(r['package']); buffer=io.BytesIO(); files=[]
with tarfile.open(fileobj=buffer,mode='w') as archive:
 for row in r['package_files']:
  p=Path(row['path']); relative=p.relative_to(package)
  assert p.is_file() and not p.is_symlink()
  data=p.read_bytes(); assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
  name=str(Path(package.name)/relative)
  info=tarfile.TarInfo(name); info.size=len(data); info.mode=p.stat().st_mode&0o777; info.mtime=0
  archive.addfile(info,io.BytesIO(data)); files.append(dict(path=name,bytes=len(data),sha256=row['sha256']))
out=s/'this-play-fixed-unit-grow.deadpan.tar.gz'; out.write_bytes(gzip.compress(buffer.getvalue(),mtime=0))
destination=s/'owner-fixture-check'; destination.mkdir()
with tarfile.open(fileobj=io.BytesIO(gzip.decompress(out.read_bytes())),mode='r:') as archive:
 for member in archive.getmembers():
  assert member.isfile() and not Path(member.name).is_absolute() and '..' not in Path(member.name).parts
 archive.extractall(destination,filter='data')
for row in files:
 p=destination/row['path']; assert len(p.read_bytes())==row['bytes'] and hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256']
(s/'owner-fixture-identity.json').write_text(json.dumps({'archive':out.name,'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'bytes':out.stat().st_size,'files':files,'extracted':str(destination/package.name)},indent=2)+'\n')
print(len(files),'exact package files retained and independently extracted/reh ashed;',out.stat().st_size,'archive bytes')
