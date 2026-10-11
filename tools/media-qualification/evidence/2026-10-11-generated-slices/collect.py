from pathlib import Path
import gzip,hashlib,json,re,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-slices'
root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
tests={}
for label in ['native-full','worker-tests']:
 for relative in re.findall(r'Running [^\n]+ \(([^)]+)\)',(r/(label+'.log')).read_text()):
  p=repo/relative
  if p.is_file():
   dst=r/'test-binaries'/p.name;dst.parent.mkdir(exist_ok=True);shutil.copy2(p,dst)
   tests[str(dst)]=sha(dst.read_bytes())
libs={}
for line in subprocess.check_output(['otool','-L',str(r/'bin/deadpan-app')],text=True).splitlines():
 p=Path(line.strip().split(' (')[0])
 if 'Deadpan/ffmpeg-' in str(p):libs[str(p)]=sha(p.read_bytes())
prefix=Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3')
upstream={str(prefix/'ffmpeg-8.0.3/libavcodec'/name):sha((prefix/'ffmpeg-8.0.3/libavcodec'/name).read_bytes()) for name in ['ffv1.c','ffv1dec.c','ffv1_parse.c','pthread_slice.c']}
identity=dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),libraries=libs,upstream_sources=upstream,test_binaries=tests,
 fixtures={'1080p':'/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan','4k':'/tmp/deadpan-generated-loops-20261011/4k/two-holds.deadpan','long':'/tmp/deadpan-generated-playback-performance-20261011/1080p-fallback.deadpan'},
 os=subprocess.check_output(['sw_vers'],text=True),hardware=subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.ncpu','machdep.cpu.brand_string'],text=True),rust=subprocess.check_output(['rustc','--version'],text=True).strip())
(r/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
manifest=[]
def add(name,data,compress=False):
 output=gzip.compress(data,mtime=0) if compress else data
 name+='.gz' if compress else ''
 target=root/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(output)
 manifest.append(dict(path=name,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
for p in sorted(r.iterdir()):
 if p.is_file() and p.suffix in {'.py','.json','.log','.txt'}:add(p.name,p.read_bytes(),p.suffix in {'.log','.txt'})
for name in [f'play-1080p-{i}' for i in range(5)]+['play-4k','loop-1080p','loop-4k','play-long']:
 for filename in ['report.json','generated-loop.json','generated-playback.json']:
  p=r/name/filename
  if p.exists():add(name+'/'+filename,p.read_bytes(),True)
for label in ['source','compare-source']:
 for p in sorted((r/label).rglob('*')):
  if p.is_file():add(label+'/'+str(p.relative_to(r/label)),p.read_bytes(),True)
for name in ['report.json','test.log']:
 p=r/'sanitizers'/name;add('sanitizers/'+name,p.read_bytes(),name.endswith('.log'))
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest))))
