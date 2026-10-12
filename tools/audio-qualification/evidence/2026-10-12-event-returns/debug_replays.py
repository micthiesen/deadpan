import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
commands=[('fmt-harness-fix',['cargo','fmt','--all']),('debug-harness',['cargo','build','--locked','-p','deadpan-app','--features','ui-harness'])]
for name,scenario in [('wet-debug','wet-returns'),('sound-debug','sound-placement')]:
 commands.append((name,['target/debug/deadpan-app','--ui-check','--scenario',scenario,'--mode','visual','--output',str(s/name),'--retain-projects']))
for name,command in commands:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
