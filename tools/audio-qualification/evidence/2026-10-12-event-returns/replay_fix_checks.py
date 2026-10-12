import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,command in [
 ('clippy-replay-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-replay-final',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness'])]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
r=subprocess.run([sys.executable,str(s/'replays.py')]);sys.exit(r.returncode)
