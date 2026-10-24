from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,sys,time
ROOT=Path('/Users/michael/Code/deadpan')
S=Path('/tmp/deadpan-retime-follow-creep-2026-10-24')
label=sys.argv[1]
argv=sys.argv[2:]
owned=['crates/deadpan-plan/tests/picture_plan.rs','crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs']
def basis_paths():
    return sorted(str(p.relative_to(ROOT)) for crate in ['deadpan-core','deadpan-plan'] for p in (ROOT/'crates'/crate/'src').rglob('*.rs'))
def pins(paths):
    return {p:{'bytes':(ROOT/p).stat().st_size,'sha256':hashlib.sha256((ROOT/p).read_bytes()).hexdigest()} for p in paths}
for suffix in ['.stdout','.stderr','.json']:
    assert not (S/(label+suffix)).exists()
manifest=json.loads((S/'source-files.json').read_text())
assert len(manifest)==17 and all((ROOT/name).is_file() for name in manifest)
scope=sorted(set(owned+basis_paths()+manifest))
record={'argv':argv,'cwd':str(ROOT),'environment':{k:os.environ[k] for k in ['CARGO_TARGET_DIR','DEADPAN_FFMPEG_PREFIX'] if k in os.environ},'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'source_before':pins(owned),'core_plan_basis_before':pins(basis_paths()),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope_limit':'Own2testpaths plus all Core/Plan src .rs basis and actual global17 presence/hash snapshots for this focused run. Root owns final17freeze/broadApp/native qualification.'}
record['global17_before']=pins(manifest)
snapshot=S/(label+'-source')
snapshot.mkdir()
for name in scope:
    dest=snapshot/(name+'.txt')
    dest.parent.mkdir(parents=True,exist_ok=True)
    dest.write_bytes((ROOT/name).read_bytes())
assert pins(scope)=={**record['source_before'],**record['core_plan_basis_before'],**record['global17_before']}
started=time.monotonic()
with (S/(label+'.stdout')).open('x') as stdout,(S/(label+'.stderr')).open('x') as stderr:
    result=subprocess.run(argv,cwd=ROOT,env=os.environ,stdout=stdout,stderr=stderr)
record.update(exit_code=result.returncode,seconds=time.monotonic()-started,finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=pins(owned),core_plan_basis_after=pins(basis_paths()))
record['global17_after']=pins(manifest)
record['global17_unchanged']=record['global17_before']==record['global17_after']
record['owned_sources_unchanged']=record['source_before']==record['source_after']
record['core_plan_basis_unchanged']=record['core_plan_basis_before']==record['core_plan_basis_after']
record['streams']={str(S/(label+ext)):{'bytes':(S/(label+ext)).stat().st_size,'sha256':hashlib.sha256((S/(label+ext)).read_bytes()).hexdigest()} for ext in ['.stdout','.stderr']}
stderr=(S/(label+'.stderr')).read_text()
match=re.search(r'Running tests/picture_plan.rs \(([^)]+)\)',stderr)
if match:
    p=Path(match.group(1))
    record['executed_binary']={'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
(S/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({k:record[k] for k in ['argv','exit_code','seconds','owned_sources_unchanged','core_plan_basis_unchanged','global17_unchanged']},indent=2))
print((S/(label+'.stdout')).read_text())
print(stderr)
sys.exit(result.returncode)
