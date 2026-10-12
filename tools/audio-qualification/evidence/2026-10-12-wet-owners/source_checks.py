import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,command in [
 ('clippy-verified',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('fmt-final',['cargo','fmt','--all','--check']),
 ('whitespace-final',['git','diff','--check']),
]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
