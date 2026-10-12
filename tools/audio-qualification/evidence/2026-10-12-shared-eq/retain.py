import gzip,hashlib,io,json,re,shutil,tarfile
from pathlib import Path
source=Path(__file__).parent
out=Path('/Users/michael/Code/deadpan/tools/audio-qualification/evidence/2026-10-12-shared-eq')
records=json.loads((source/'commands.json').read_text());assert all(r['exit_code']==0 for r in records)
report=json.loads((source/'export.json').read_text());assert report['report']['passed'] and not report['report']['failures']
out.mkdir(parents=True,exist_ok=True)
for name in ['verify.py','retain.py','check-evidence.py','review.md','commands.json','identity.json','release-identity.json','export.json']:
 shutil.copyfile(source/name,out/name)
for name in ['tests','fmt','clippy','release','whitespace']:
 (out/f'{name}.log.gz').write_bytes(gzip.compress((source/f'{name}.log').read_bytes(),mtime=0))
movie=Path(report['movie']);shutil.copyfile(movie,out/'shared-equalizer.mp4')
receipts=list(movie.parent.glob('deadpan-render-*.json'));assert len(receipts)==1
shutil.copyfile(receipts[0],out/'render-receipt.json')
package=Path(report['package']);wal=package/'project.sqlite-wal';assert not wal.exists() or wal.stat().st_size==0
stream=io.BytesIO()
with tarfile.open(fileobj=stream,mode='w') as archive:
 for path in sorted(package.rglob('*')):
  if path.is_file() and not path.name.endswith(('-shm','-wal','.lock')):
   item=archive.gettarinfo(str(path),arcname='shared-equalizer.deadpan/'+str(path.relative_to(package)))
   item.uid=item.gid=item.mtime=0;item.uname=item.gname=''
   with path.open('rb') as data:archive.addfile(item,data)
(out/'shared-equalizer.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(),mtime=0))
result={'passed_tests':970,'release_tests':1,'summary':report['report']['summary'],'audio_offsets':[w['measured_offset_samples'] for w in report['report']['audio']]}
(out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
manifest={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(result))
