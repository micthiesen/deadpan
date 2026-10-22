from pathlib import Path
import gzip,hashlib,json,shutil

scratch=Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
out=Path('tools/media-qualification/evidence/2026-10-21-selected-hold-duration-semantics')
out.mkdir(parents=True,exist_ok=True)

def copy(source,name=None):
    name=name or source.name
    target=out/name
    if source.suffix in ['.log','.rs','.txt'] or (source.suffix!='.png' and source.stat().st_size>100000):
        target=out/(name+'.gz')
        target.write_bytes(gzip.compress(source.read_bytes(),mtime=0))
    else:
        shutil.copy2(source,target)

# Publication receipts can contain private owner sign-in links. They never
# belong in repository qualification evidence.
for source in sorted(scratch.iterdir()):
    if source.is_file() and source.suffix in ['.log','.json','.sh','.py','.txt','.md','.rs']:
        if any(word in source.name.lower() for word in ['artifact','progress']):
            continue
        copy(source)

snapshots={}
directories=['baseline-source']+sorted(p.name for p in scratch.iterdir() if p.is_dir() and p.name.endswith('runtime-source'))+['final-source']
for directory in directories:
    rows=[]
    for source in sorted((scratch/directory).rglob('*.rs')):
        digest=hashlib.sha256(source.read_bytes()).hexdigest()
        blob='source-'+digest+'.rs'
        if not (out/(blob+'.gz')).exists():
            copy(source,blob)
        rows.append({'path':str(source.relative_to(scratch/directory)),'sha256':digest,'bytes':source.stat().st_size,'blob':blob+'.gz'})
    snapshots[directory]=rows
(out/'source-snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')

for mode in sorted(p.name for p in scratch.iterdir() if p.is_dir() and (p.name.startswith('visual') or p.name.startswith('performance'))):
    for source in sorted((scratch/mode).rglob('*.json')):
        copy(source,mode+'__'+str(source.relative_to(scratch/mode)).replace('/','__'))
for capture in json.loads((scratch/'selected-captures.json').read_text()):
    copy(scratch/capture['mode']/capture['file'],capture['mode']+'-'+capture['file'])

inventory=[]
for source in sorted(out.iterdir()):
    if source.name=='inventory.json':
        continue
    inventory.append({'file':source.name,'bytes':source.stat().st_size,'sha256':hashlib.sha256(source.read_bytes()).hexdigest()})
(out/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for row in inventory:
    source=out/row['file']
    assert source.stat().st_size==row['bytes']
    assert hashlib.sha256(source.read_bytes()).hexdigest()==row['sha256'],source
print(f'{len(inventory)} entries, {sum(row["bytes"] for row in inventory)} bytes; all rehashed')
