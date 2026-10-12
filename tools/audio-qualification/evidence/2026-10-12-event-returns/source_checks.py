import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,command in [('fmt-final',['cargo','fmt','--all','--check']),('whitespace-final',['git','diff','--check'])]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
