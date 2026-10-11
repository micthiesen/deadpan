from pathlib import Path
import hashlib,json,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
pins=json.loads((r/'source-pins.json').read_text())
for path,digest in pins['files'].items():
 assert hashlib.sha256((repo/path).read_bytes()).hexdigest()==digest,path
b=r/'bin';b.mkdir()
for old in Path('/tmp/deadpan-gain-waveform-20261011/bin-release').iterdir():
 source=repo/'target/release'/old.name if old.name in ['deadpan-app','deadpan-cli'] else old
 shutil.copy2(source,b/old.name)
(r/'binary-pins.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in b.iterdir()},indent=2)+'\n')
for scenario in ['audio-treatments','gain']:
 subprocess.run(['python3',str(r/'run.py'),scenario,str(b/'deadpan-app'),'--ui-check','--scenario',scenario,'--mode','visual','--hz','120','--output',str(r/scenario)],check=True)
