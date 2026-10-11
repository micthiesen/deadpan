from pathlib import Path
import hashlib, json, subprocess, shutil, re
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
def sha(data):return hashlib.sha256(data).hexdigest()
def command(args):return subprocess.check_output(args,cwd=repo,text=True).strip()
bin_dir=root/'bin-verified';bin_dir.mkdir()
executables={}
for name in ['deadpan-app','deadpan-cli','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
 path=bin_dir/name;shutil.copy2(repo/'target/release'/name,path)
 executables[name]=sha(path.read_bytes())
source=[
 'crates/deadpan-app/src/preview/harness/telemetry.rs',
 'crates/deadpan-app/src/presentation.rs',
 'crates/deadpan-app/src/preview/harness.rs',
 'crates/deadpan-app/src/preview/harness/generated_playback.rs',
 'crates/deadpan-app/src/ui_harness.rs',
 'crates/deadpan-cli/tests/generated_joins.rs',
 'crates/deadpan-app/src/worker.rs',
 'crates/deadpan-app/src/worker/playback_cache.rs',
 'crates/deadpan-app/src/worker/generated_ahead.rs',
]
linkage=command(['otool','-L',str(bin_dir/'deadpan-app')])
identity=dict(binary_dir=str(bin_dir),base=command(['git','rev-parse','HEAD']),binary_sha256=executables,
 source_sha256={name:sha((repo/name).read_bytes()) for name in source},
 rust=command(['rustc','--version']),os=command(['sw_vers']),
 hardware=command(['sysctl','hw.model','hw.memsize','hw.ncpu','machdep.cpu.brand_string']),linkage=linkage,
 libraries_sha256={name:sha(Path(name).read_bytes()) for name in re.findall(r'^\s+(/[^\s]+\.dylib)',linkage,re.M) if Path(name).is_file()})
(root/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
for name in ['4k-fallback','1080p-fallback']:
 fixture=root/(name+'.deadpan')
 pins={str(path.relative_to(fixture)):dict(bytes=path.stat().st_size,sha256=sha(path.read_bytes())) for path in sorted(fixture.rglob('*')) if path.is_file() and path.name not in {'.writer.lock','.host.json'} and not path.name.endswith(('-shm','-wal'))}
 (root/(name+'-pins.json')).write_text(json.dumps(pins,indent=2)+'\n')
print(json.dumps(identity['binary_sha256']))
