from pathlib import Path
import subprocess
r=Path(__file__).parent
steps=[
('app-final',['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app']),
('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
('release-build',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','--features','ui-harness']),
('export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PREVIEW_EXPORT_RESULTS='+str(r/'export-report.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','stereo_balance_keeps_channels_through_limited_preview_and_render','--','--nocapture']),
('format-check',['cargo','fmt','--all','--check']),
]
for name,cmd in steps:
 subprocess.run(['python3',str(r/'run.py'),name,*cmd],check=True)
