from pathlib import Path
import gzip,hashlib,json,subprocess
r=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan');root=repo/'tools/audio-qualification/evidence/2026-10-11-gain-scroll';root.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
identity=dict(base=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),os=subprocess.check_output(['sw_vers'],text=True),hardware=subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.ncpu','machdep.cpu.brand_string'],text=True),rust=subprocess.check_output(['rustc','--version'],text=True).strip())
(r/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
manifest=[]
def add(name,data,compress=False):
 output=gzip.compress(data,mtime=0) if compress else data
 name+='.gz' if compress else ''
 p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(output)
 manifest.append(dict(path=name,bytes=len(output),sha256=sha(output),original_sha256=sha(data)))
for p in sorted(r.iterdir()):
 if p.is_file() and p.suffix in {'.py','.json','.log'}:add(p.name,p.read_bytes(),p.suffix=='.log')
for name in ['diagnose-visual','fixed-visual','gestures-visual','release-60','release-120']:
 add(name+'/report.json',(r/name/'report.json').read_bytes(),True)
for p in sorted((r/'source').iterdir()):add('source/'+p.name,p.read_bytes(),True)
for dest,src in {'failure-compact':'diagnose-visual/gain-073.png','waveform-default':'gestures-visual/gain-073.png','waveform-compact':'gestures-visual/gain-086.png','retry-compact':'gestures-visual/gain-097.png','curve-compact':'gestures-visual/gain-121.png','curve-default':'gestures-visual/gain-122.png'}.items():add('captures/'+dest+'.png',(r/src).read_bytes())
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(dict(files=len(manifest),bytes=sum(p['bytes'] for p in manifest))))
