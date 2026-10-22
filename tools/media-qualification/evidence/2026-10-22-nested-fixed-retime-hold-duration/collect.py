from pathlib import Path
import gzip,hashlib,io,json,shutil,tarfile
scratch=Path('/tmp/deadpan-nested-fixed-retime-duration-2026-10-22')
out=Path('tools/media-qualification/evidence/2026-10-22-nested-fixed-retime-hold-duration')
out.mkdir(parents=True,exist_ok=True)
def copy(source,name=None):
    name=name or source.name
    data=source.read_bytes()
    if source.name == 'verify.sh' or source.suffix in ['.log','.rs','.txt','.stdout','.stderr'] or (source.suffix!='.png' and len(data)>100000):
        (out/(name+'.gz')).write_bytes(gzip.compress(data,mtime=0))
    else: (out/name).write_bytes(data)
for source in sorted(scratch.iterdir()):
    if source.is_file() and source.suffix in ['.log','.json','.sh','.py','.txt','.md','.rs','.sha256','.patch','.stdout','.stderr']:
        if any(word in source.name.lower() for word in ['artifact','progress','next-mixed']): continue
        copy(source)
snapshots={}
directories=sorted(p.name for p in scratch.iterdir() if p.is_dir() and p.name.endswith('source'))
for directory in directories:
    rows=[]
    buffer=io.BytesIO()
    with tarfile.open(fileobj=buffer,mode='w') as archive:
        for source in sorted((scratch/directory).rglob('*.rs')):
            data=source.read_bytes(); name=str(source.relative_to(scratch/directory))
            entry=tarfile.TarInfo(name); entry.size=len(data); entry.mode=0o644; entry.mtime=0
            archive.addfile(entry,io.BytesIO(data))
            rows.append({'path':name,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)})
    archive_name=directory+'.tar.gz'
    (out/archive_name).write_bytes(gzip.compress(buffer.getvalue(),mtime=0))
    snapshots[directory]={'archive':archive_name,'files':rows}
(out/'source-snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
for mode in sorted(p.name for p in scratch.iterdir() if p.is_dir() and (p.name in ['visual','performance'] or p.name.startswith('debug-visual'))):
    if not (scratch/mode).exists(): continue
    for source in sorted((scratch/mode).rglob('*.json')):
        copy(source,mode+'__'+str(source.relative_to(scratch/mode)).replace('/','__'))
if (scratch/'selected-captures.json').exists():
    for capture in json.loads((scratch/'selected-captures.json').read_text()):
        copy(scratch/capture['mode']/capture['file'],capture['mode']+'-'+capture['file'])
for row in json.loads((scratch/'encoded-identities.json').read_text())['outputs']:
    movie=Path(row['movie'])
    assert hashlib.sha256(movie.read_bytes()).hexdigest()==row['movie_sha256']
    copy(movie,'encoded-'+movie.name)
owner=scratch/'nested-fixed-unit-grow.deadpan.tar.gz'
(out/owner.name).write_bytes(owner.read_bytes())
inventory=[]
for source in sorted(out.iterdir()):
    if source.name=='inventory.json': continue
    inventory.append({'file':source.name,'bytes':source.stat().st_size,'sha256':hashlib.sha256(source.read_bytes()).hexdigest()})
(out/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for row in inventory:
    data=(out/row['file']).read_bytes()
    assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
for snapshot in snapshots.values():
    with tarfile.open(fileobj=io.BytesIO(gzip.decompress((out/snapshot['archive']).read_bytes())),mode='r:') as archive:
        for row in snapshot['files']:
            data=archive.extractfile(row['path']).read()
            assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
print(len(inventory),'entries,',sum(row['bytes'] for row in inventory),'bytes; all inventory and archived sources rehashed')
