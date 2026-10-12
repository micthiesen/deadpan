import os,subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
commands=[
 ('full-tests-final',['cargo','test','--locked','-p','deadpan-core','-p','deadpan-plan','-p','deadpan-store','-p','deadpan-audio','-p','deadpan-playback','-p','deadpan-app','--features','ui-harness']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','--features','ui-harness']),
 ('export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1',f'DEADPAN_PREVIEW_EXPORT_RESULTS={s}/export-results.json','cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','continuous_owner_return_preserves_preview_and_verified_export','--','--nocapture']),
]
for name,command in commands:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
