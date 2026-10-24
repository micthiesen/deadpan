from pathlib import Path
import hashlib,json,os,subprocess,sys,time,datetime
root=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-sequence-follow-creep-2026-10-24')
label=sys.argv[1]
argv=sys.argv[2:]
owned=['crates/deadpan-plan/tests/picture_plan.rs','crates/deadpan-plan/tests/picture_plan/sequence_follow_creep.rs']
basis=list(root.glob('crates/deadpan-plan/src/**/*.rs'))+list(root.glob('crates/deadpan-core/src/**/*.rs'))
def pin(paths):
    return {str(p):{'bytes':(root/p).stat().st_size,'sha256':hashlib.sha256((root/p).read_bytes()).hexdigest()} for p in paths}
receipt={'argv':argv,'cwd':str(root),'env':{k:os.environ.get(k) for k in ['CARGO_TARGET_DIR','DEADPAN_FFMPEG_PREFIX']},'source_before':pin(owned),'basis_before':pin([str(p.relative_to(root)) for p in basis]),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
for filename in [f'{label}.stdout',f'{label}.stderr',f'{label}.json']:
    assert not (scratch/filename).exists(),filename
started=time.monotonic()
with (scratch/f'{label}.stdout').open('w') as out,(scratch/f'{label}.stderr').open('w') as err:
    result=subprocess.run(argv,cwd=root,env=os.environ,stdout=out,stderr=err)
receipt.update(exit_code=result.returncode,duration_seconds=time.monotonic()-started,finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=pin(owned),basis_after=pin([str(p.relative_to(root)) for p in basis]))
receipt['owned_sources_unchanged']=receipt['source_before']==receipt['source_after']
receipt['production_basis_unchanged']=receipt['basis_before']==receipt['basis_after']
receipt['scope']='focused two Plan tests only; complete cross-lane source freeze and broad checks belong to Main'
(scratch/f'{label}.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['argv','exit_code','duration_seconds','owned_sources_unchanged','production_basis_unchanged']},indent=2))
print((scratch/f'{label}.stdout').read_text())
print((scratch/f'{label}.stderr').read_text())
sys.exit(result.returncode)
