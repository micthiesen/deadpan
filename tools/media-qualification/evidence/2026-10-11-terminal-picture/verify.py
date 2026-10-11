from pathlib import Path
import subprocess
r=Path(__file__).parent
steps=[
 ('format-final',['cargo','fmt','--all']),
 ('clippy-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('transport-tests-final',['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness','transport::','--','--test-threads=1']),
 ('presentation-tests-final',['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness','presentation::','--','--test-threads=1']),
 ('app-build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
 ('format-check',['cargo','fmt','--all','--check']),
]
for name,command in steps:
 subprocess.run(['python3',str(r/'run.py'),name,*command],check=True)
