from pathlib import Path
import subprocess
root=Path(__file__).parent
commands=[
 ('format-check',['cargo','fmt','--all','--check']),
 ('harness-tests',['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness','ui_harness::tests','--','--test-threads=1']),
 ('build-final',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
]
for name,command in commands:
 subprocess.run(['python3',str(root/'run.py'),name,*command],check=True)
