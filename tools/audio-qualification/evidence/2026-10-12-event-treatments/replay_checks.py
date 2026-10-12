import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,command in [
 ('release-replay-final',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
 ('clippy-replay-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('fmt-replay-final',['cargo','fmt','--all','--check']),
]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
sys.exit(subprocess.run([sys.executable,str(s/'replays.py')]).returncode)
