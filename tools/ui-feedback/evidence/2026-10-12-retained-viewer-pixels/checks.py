import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,command in [
 ('release',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('fmt',['cargo','fmt','--all','--check']),
 ('replay',['target/release/deadpan-app','--ui-check','--scenario','sound-placement','--mode','visual','--output',str(s/'replay'),'--retain-projects']),
]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
