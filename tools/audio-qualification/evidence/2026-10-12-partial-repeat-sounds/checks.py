import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
commands=[
 ('core-plan',['cargo','test','--locked','-p','deadpan-core','-p','deadpan-plan']),
 ('audio-clocks',['cargo','test','--locked','-p','deadpan-audio','--test','audio_definition','sound_clocks']),
 ('store-clocks',['cargo','test','--locked','-p','deadpan-store','--test','source_registration','sound_clocks']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('debug-export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PARTIAL_REPEAT_RESULTS='+str(s/'debug-export.json'),'cargo','test','--locked','-p','deadpan-cli','--test','preview_export','partial_repeat_attached','--','--nocapture']),
 ('release-audio',['cargo','test','--locked','--release','-p','deadpan-audio','--test','audio_definition','partial_repeat','--','--nocapture']),
 ('release-export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PARTIAL_REPEAT_RESULTS='+str(s/'release-export.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','partial_repeat_attached','--','--nocapture']),
 ('release-app',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli']),
 ('fmt-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
 print('Starting '+name,flush=True)
 result=subprocess.run(['python3',str(s/'run.py'),name,*command])
 if result.returncode: sys.exit(result.returncode)
