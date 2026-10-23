from pathlib import Path
import hashlib,json,gzip,tarfile,subprocess,io
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22')
dest=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-neutral-ungroup')
dest.mkdir(parents=True,exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
def put(name,data):
 target=dest/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
def copy(name,path):put(name,path.read_bytes())
base='c72abe12965b08f7705e0c9a61ab2298d5a562ae'
with tarfile.open(s/'baseline.tar')as archive:
 base_files={member.name:archive.extractfile(member).read()for member in archive.getmembers()if member.isfile()}
base_hashes={name:sha(data)for name,data in base_files.items()}
put('source-base.json',(json.dumps({'git_commit':base,'archive_sha256':sha((s/'baseline.tar').read_bytes()),'reconstruction':'Git archive of the immutable base plus each phase source-overlays; Rust paths use .rs.txt only in this evidence directory.'},indent=2)+'\n').encode())
phases=sorted(p.name.removesuffix('-source-identities.json')for p in s.glob('*-source-identities.json'))
for phase in phases:
 identities=json.loads((s/(phase+'-source-identities.json')).read_text())
 overlay=s/(phase+'-overlays')
 full=s/(phase+'-source')
 for name,wanted in identities.items():
  candidate=overlay/(name+'.txt')
  if candidate.is_file():data=candidate.read_bytes()
  elif full.is_dir()and(full/name).is_file():data=(full/name).read_bytes()
  else:
   assert base_hashes.get(name)==wanted,(phase,name,'no reconstructable exact source')
   continue
  assert sha(data)==wanted,(phase,name,'frozen source mismatch')
  if base_hashes.get(name)!=wanted:
   put(f'{phase}/source-overlays/{name}.txt',data)
 copy(f'{phase}/source-identities.json',s/(phase+'-source-identities.json'))
 for suffix in ['-inputs.json','-lock.json','-build.log','-build-receipt.json','-run.log','-run-receipt.json','-check.log','-check-receipt.json','-format.log','-format-receipt.json','-binaries.json','-helpers.json','-result.json','-list.log','-kestrel.json','-pre-performance-processes.json','-power.txt','-performance.log','-performance-receipt.json','-neutral.log','-neutral-receipt.json','-neutral-fixture.json','-aligned.log','-aligned-receipt.json','-aligned-fixture.json']:
  source=s/(phase+suffix)
  if source.is_file():copy(f'{phase}/{source.name}',source)
 numerical=s/(phase+'-neutral-numerical-summary.json')
 if numerical.is_file():copy(f'{phase}/{numerical.name}',numerical)
 for folder in [s/(phase+'-evidence'),s/(phase+'-performance')]:
  if not folder.is_dir():continue
  for path in folder.rglob('*'):
   if path.is_file():
    rel=path.relative_to(folder)
    # Full numerical arrays are retained deterministically compressed.
    data=path.read_bytes()
    name=f'{phase}/{folder.name}/{rel}'
    if path.suffix=='.json' and len(data)>100000:put(name+'.gz',gzip.compress(data,mtime=0))
    else:put(name,data)
for name in ['run_baseline_pcm.py','qualify.py','freeze.py','guard.py','run_matrix.py','finish_qualification.py','qualify_native_media.py','retain_evidence.py','retain_media_and_check.py','check_leak.py','reviews.json','broad-isolated-leak.log','broad-isolated-leak-receipt.json','matrix-receipt.json','finish-qualification-receipt.json','baseline-pcm-setup-failure.json','candidate-pcm-numerical-summary.json']:
 source=s/name
 if source.is_file():copy('procedure/'+name,source)
host={'hardware':'Mac17,7','cpu':'Apple M5 Max','memory_bytes':137438953472,'macos':'26.5.2','macos_build':'25F84','rustc':'1.97.1 (8bab26f4f 2026-07-14)','cargo':'1.97.1 (c980f4866 2026-06-30)','ffmpeg_prefix':'/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'}
put('host.json',(json.dumps(host,indent=2)+'\n').encode())
# Every input must be reconstructable from the base or retained phase inputs.
for phase in phases:
 inputs=s/(phase+'-inputs.json')
 if not inputs.is_file():inputs=s/(phase+'-lock.json')
 for name,wanted in json.loads(inputs.read_text()).items():
  if base_hashes.get(name)==wanted:continue
  source=s/(phase+'-inputs')/name
  assert source.is_file()and sha(source.read_bytes())==wanted,(phase,name,'input mismatch')
  copy(f'{phase}/inputs/{name}',source)
assert not list(dest.rglob('*.rs')),'Evidence Rust overlays must use .rs.txt'
files={str(p.relative_to(dest)):{'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())}for p in sorted(dest.rglob('*'))if p.is_file()and p!=dest/'inventory.json'}
put('inventory.json',(json.dumps({'source_base':base,'phases':phases,'files':files},indent=2)+'\n').encode())
print(json.dumps({'phases':len(phases),'payload_files':len(files),'payload_bytes':sum(r['bytes']for r in files.values()),'inventory_sha256':sha((dest/'inventory.json').read_bytes())}))
