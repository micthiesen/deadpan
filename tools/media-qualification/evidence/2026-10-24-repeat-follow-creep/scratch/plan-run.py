from pathlib import Path
import hashlib,json,os,subprocess,sys,time,datetime,re,shutil
root=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-repeat-follow-creep-2026-10-24')
label=sys.argv[1]
argv=sys.argv[2:]
owned=['crates/deadpan-plan/tests/picture_plan.rs','crates/deadpan-plan/tests/picture_plan/repeat_follow_creep.rs']
basis=list(root.glob('crates/deadpan-plan/src/**/*.rs'))+list(root.glob('crates/deadpan-core/src/**/*.rs'))
manifest=json.loads((scratch/'source-files.json').read_text())
def pin(paths):
    return {str(p):{'bytes':(root/p).stat().st_size,'sha256':hashlib.sha256((root/p).read_bytes()).hexdigest()} for p in paths}
receipt={'argv':argv,'cwd':str(root),'env':{k:os.environ.get(k) for k in ['CARGO_TARGET_DIR','DEADPAN_FFMPEG_PREFIX']},'source_before':pin(owned),'basis_before':pin([str(p.relative_to(root)) for p in basis]),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt['main_manifest_before']=pin(manifest)
receipt['runner_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
for filename in [f'{label}.stdout',f'{label}.stderr',f'{label}.json']:
    assert not (scratch/filename).exists(),filename
# Exact bytes remain available even for a failed initial fixture.
for p in owned:
    shutil.copyfile(root/p,scratch/f'{label}-{Path(p).stem}.rs.txt')
started=time.monotonic()
with (scratch/f'{label}.stdout').open('w') as out,(scratch/f'{label}.stderr').open('w') as err:
    result=subprocess.run(argv,cwd=root,env=os.environ,stdout=out,stderr=err)
receipt.update(exit_code=result.returncode,duration_seconds=time.monotonic()-started,finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=pin(owned),basis_after=pin([str(p.relative_to(root)) for p in basis]))
receipt['main_manifest_after']=pin(manifest)
receipt['main_manifest_unchanged']=receipt['main_manifest_before']==receipt['main_manifest_after']
receipt['owned_sources_unchanged']=receipt['source_before']==receipt['source_after']
receipt['production_basis_unchanged']=receipt['basis_before']==receipt['basis_after']
stderr=(scratch/f'{label}.stderr').read_text()
match=re.search(r'Running tests/picture_plan.rs \(([^)]+)\)',stderr)
if match:
    binary=Path(match.group(1))
    receipt['executed_binary']={'path':str(binary),'bytes':binary.stat().st_size,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
receipt['scope']='focused three Plan tests; full Main21manifest retained; broad qualification belongs to Main'
(scratch/f'{label}.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['argv','exit_code','duration_seconds','owned_sources_unchanged','production_basis_unchanged','main_manifest_unchanged']},indent=2))
print((scratch/f'{label}.stdout').read_text())
print(stderr)
sys.exit(result.returncode)
