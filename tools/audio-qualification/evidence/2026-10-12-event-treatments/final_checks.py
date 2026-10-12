import subprocess, sys
from pathlib import Path
s=Path(__file__).resolve().parent
commands=[
 ('release-final',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','--features','ui-harness']),
 ('export-fixed',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1',f'DEADPAN_PREVIEW_EXPORT_RESULTS={s}/export-results.json','cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','treated_event_preserves_preview_and_verified_export','--','--nocapture']),
 ('headless-sound',['cargo','test','--locked','--release','-p','deadpan-cli','--test','headless_parity','placed_sound_edits_derive_the_native_events']),
 ('clippy-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('fmt-final',['cargo','fmt','--all','--check']),
 ('whitespace-final',['git','diff','--check']),
]
for name,command in commands:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if r.returncode:sys.exit(r.returncode)
