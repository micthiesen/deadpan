from pathlib import Path
import subprocess
root=Path(__file__).parent
for name,command in [
 ('telemetry-clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','deadpan-app/ui-harness','--','-D','warnings']),
 ('telemetry-format-check',['cargo','fmt','--all','--check']),
 ('telemetry-tests',['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness','preview::harness::telemetry::tests','--','--test-threads=1']),
 ('telemetry-build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
]:
 subprocess.run(['python3',str(root/'run.py'),name,*command],check=True)
