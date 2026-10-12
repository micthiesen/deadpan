import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
checks=[
 ('store-final',['cargo','test','--locked','-p','deadpan-store']),
 ('doc-tests',['cargo','test','--locked','--doc','-p','deadpan-core','-p','deadpan-plan','-p','deadpan-store','-p','deadpan-audio','-p','deadpan-playback']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','--features','ui-harness']),
 ('export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1',f'DEADPAN_PREVIEW_EXPORT_RESULTS={s}/export-results.json','cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','continuous_owner_return_preserves_preview_and_verified_export','--','--nocapture']),
]
for name,command in checks:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
