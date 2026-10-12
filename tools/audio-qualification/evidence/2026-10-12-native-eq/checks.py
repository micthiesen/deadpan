import subprocess, sys
from pathlib import Path
root = Path(__file__).parent
commands = [
 ('gain-tests', ['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','gain']),
 ('eq-tests', ['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','eq_']),
 ('registry-tests', ['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','navigation::registry::tests::']),
 ('clippy', ['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('normal-app', ['cargo','check','--locked','-p','deadpan-app']),
 ('format', ['cargo','fmt','--all','--','--check']),
 ('release-final', ['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
]
for name, command in commands:
 subprocess.run([sys.executable,str(root/'run.py'),name,*command],check=True)
