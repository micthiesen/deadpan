from pathlib import Path
import subprocess,shutil,hashlib,json
r=Path(__file__).parent
steps=[('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),('release-build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),('format-check',['cargo','fmt','--all','--check'])]
for name,command in steps:subprocess.run(['python3',str(r/'run.py'),name,*command],check=True)
b=r/'bin-release';b.mkdir()
for p in (r/'bin-gestures').iterdir():shutil.copy2(Path('target/release/deadpan-app') if p.name=='deadpan-app' else p,b/p.name)
(r/'release-pins.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in b.iterdir()},indent=2)+'\n')
for hz in [60,120]:
 name=f'release-{hz}'
 subprocess.run(['python3',str(r/'run.py'),name,str(b/'deadpan-app'),'--ui-check','--scenario','gain','--mode','performance','--hz',str(hz),'--output',str(r/name)],check=True)
