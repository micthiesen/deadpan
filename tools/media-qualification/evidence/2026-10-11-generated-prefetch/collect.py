from pathlib import Path
import gzip, hashlib, json, platform, re, subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path(__file__).parent
root = repo / 'tools/media-qualification/evidence/2026-10-11-generated-prefetch'
root.mkdir(exist_ok=True)
def sha(data): return hashlib.sha256(data).hexdigest()
def run(args): return subprocess.check_output(args, cwd=repo, text=True).strip()
source = [
 'crates/deadpan-app/src/worker.rs',
 'crates/deadpan-app/src/worker/playback_cache.rs',
 'crates/deadpan-app/src/worker/generated_ahead.rs',
 'crates/deadpan-app/src/worker/generated_playback_tests.rs',
 'crates/deadpan-app/src/worker/lookahead_tests.rs',
 'crates/deadpan-media/src/lookahead.rs',
]
log = (scratch / 'generated-playback.log').read_text()
match = re.search(r'Running unittests src/main.rs \((target/release/deps/[^)]+)\)', log)
assert match, 'test binary path'
binary = scratch / 'release-tests'
assert sha(binary.read_bytes()) == sha((repo/match.group(1)).read_bytes())
identity = dict(base=run(['git','rev-parse','HEAD']),
    source_sha256={name: sha((repo/name).read_bytes()) for name in source},
    binary=str(binary), binary_sha256=sha(binary.read_bytes()),
    rust=run(['rustc','--version']), os=run(['sw_vers']),
    hardware=run(['sysctl','hw.model','hw.memsize','hw.ncpu','machdep.cpu.brand_string']),
    linkage=run(['otool','-L',str(binary)]))
identity['tools_sha256'] = {str(p): sha(p.read_bytes()) for p in [repo/'target/release/deadpan-cli', repo/'target/release/deadpan-media-worker', repo/'target/release/deadpan-track', Path('/opt/homebrew/bin/ffmpeg')]}
identity['fixture_source_sha256'] = sha((repo/'native/deadpan-source/tests/fixtures/cfr-bframes.mp4').read_bytes())
identity['libraries_sha256'] = {p: sha(Path(p).read_bytes()) for p in re.findall(r'^\s+(/[^\s]+\.dylib)', identity['linkage'], re.M) if Path(p).is_file()}
(scratch/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
fixture = Path('/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan')
(scratch/'fixture-pins.json').write_text(json.dumps({str(p.relative_to(fixture)):dict(bytes=p.stat().st_size,sha256=sha(p.read_bytes())) for p in sorted(fixture.rglob('*')) if p.is_file()},indent=2)+'\n')
(scratch/'source.patch').write_bytes(subprocess.check_output(['git','diff','--',*source],cwd=repo))
manifest=[]
for path in sorted(scratch.iterdir()):
    if not path.is_file() or path.suffix not in {'.json','.log','.py','.patch'}: continue
    data=path.read_bytes()
    name=path.name
    compressed=path.suffix in {'.log','.patch'}
    output=gzip.compress(data,mtime=0) if compressed else data
    name += '.gz' if compressed else ''
    (root/name).write_bytes(output)
    manifest.append(dict(path=name,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
# New files do not occur in git diff until staged, so retain their exact source.
for name in source:
    data=(repo/name).read_bytes()
    dest='source/'+name+'.gz'
    (root/dest).parent.mkdir(parents=True,exist_ok=True)
    output=gzip.compress(data,mtime=0)
    (root/dest).write_bytes(output)
    manifest.append(dict(path=dest,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest),binary_sha256=identity['binary_sha256'])))
