from pathlib import Path
import gzip,hashlib,json,re,shutil,subprocess
r=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-terminal-picture';root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
tests={}
for label in ['transport-tests-final','presentation-tests-final']:
 for relative in re.findall(r'Running [^\n]+ \(([^)]+)\)',(r/(label+'.log')).read_text()):
  p=repo/relative
  if p.is_file():
   dst=r/'test-binaries'/p.name;dst.parent.mkdir(exist_ok=True);shutil.copy2(p,dst)
   tests[str(dst)]=sha(dst.read_bytes())
libs={}
for line in subprocess.check_output(['otool','-L',str(r/'bin/deadpan-app')],text=True).splitlines():
 p=Path(line.strip().split(' (')[0])
 if 'Deadpan/ffmpeg-' in str(p):libs[str(p)]=sha(p.read_bytes())
identity=dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),libraries=libs,test_binaries=tests,
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
for folder in sorted(r.iterdir()):
 if folder.is_dir() and (folder/'report.json').is_file():
  for filename in ['report.json','generated-loop.json','generated-playback.json']:
   p=folder/filename
   if p.exists():add(folder.name+'/'+filename,p.read_bytes(),True)
for p in sorted((r/'source').rglob('*')):
 if p.is_file():add('source/'+str(p.relative_to(r/'source')),p.read_bytes(),True)
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(row['bytes'] for row in manifest))))
