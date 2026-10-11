from pathlib import Path
import gzip,hashlib,json,re,shutil,subprocess
scratch=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-loops'
root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
sources=['crates/deadpan-app/src/'+name for name in ['worker.rs','worker/provider_ahead.rs','worker/playback_cache.rs','worker/generated_playback_tests.rs','worker/lookahead_tests.rs','preview/harness.rs','preview/harness/generated_playback.rs','ui_harness.rs']]
test=next(p for p in re.findall(r'Running unittests [^\n]+ \(([^)]+)\)',(scratch/'fixture-tests.log').read_text()) if '/deadpan_app-' in p)
shutil.copy2(repo/test,scratch/'release-tests')
fixtures={'1080p':'/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan','4k':str(scratch/'4k/two-holds.deadpan')}
for name,path in fixtures.items():
    fixture=Path(path)
    (scratch/(name+'-fixture-pins.json')).write_text(json.dumps({str(p.relative_to(fixture)):dict(bytes=p.stat().st_size,sha256=sha(p.read_bytes())) for p in sorted(fixture.rglob('*')) if p.is_file()},indent=2)+'\n')
libs={}
for line in subprocess.check_output(['otool','-L',str(scratch/'bin-verified/deadpan-app')]).decode().splitlines():
    path=line.strip().split(' (')[0]
    if 'Deadpan/ffmpeg-' in path:libs[path]=sha(Path(path).read_bytes())
identity=dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo).decode().strip(),sources={name:sha((repo/name).read_bytes()) for name in sources},fixtures=fixtures,
    test_executable=str(scratch/'release-tests'),test_executable_sha256=sha((scratch/'release-tests').read_bytes()),libraries=libs,
    old_control_app='/tmp/deadpan-generated-transitions-20261011/bin/deadpan-app',old_control_app_sha256=sha(Path('/tmp/deadpan-generated-transitions-20261011/bin/deadpan-app').read_bytes()),
    os=subprocess.check_output(['sw_vers']).decode(),hardware=subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.ncpu','machdep.cpu.brand_string']).decode(),rust=subprocess.check_output(['rustc','--version']).decode().strip())
(scratch/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
manifest=[]
def add(name,data,compress=False):
    output=gzip.compress(data,mtime=0) if compress else data
    name+='.gz' if compress else ''
    target=root/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(output)
    manifest.append(dict(path=name,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
for folder,prefix in [(scratch,''),(scratch/'4k','4k/')]:
    for p in sorted(folder.iterdir()):
        if p.is_file() and p.suffix in {'.py','.json','.stdout','.stderr','.log','.txt'}:
            add(prefix+p.name,p.read_bytes(),p.suffix in {'.stdout','.stderr','.log','.txt'})
for name in ['baseline-1080p','baseline-4k','fixed-1080p','fixed-4k','verified-1080p','verified-4k','control','control-old','control-repeat']:
    for filename in ['report.json','generated-loop.json','generated-playback.json']:
        p=scratch/name/filename
        if p.exists():add(name+'/'+filename,p.read_bytes(),True)
for label in ['baseline-source','fixed-source','verified-source']:
    for p in sorted((scratch/label).rglob('*.rs')):add(label+'/'+str(p.relative_to(scratch/label)),p.read_bytes(),True)
for name in sources:add('source/'+name,(repo/name).read_bytes(),True)
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest))))
