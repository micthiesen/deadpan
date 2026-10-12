import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
commands=[
 ('fmt-final',['cargo','fmt','--all']),
 ('plan-audio-playback',['cargo','test','--locked','-p','deadpan-plan','-p','deadpan-audio','-p','deadpan-playback']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-cache',['cargo','test','--locked','--release','-p','deadpan-audio','--lib','source_projection','--','--nocapture']),
 ('release-nested',['cargo','test','--locked','--release','-p','deadpan-audio','--test','audio_definition','partial_repeat_keeps_equalized_nested_preserve_context','--','--nocapture']),
 ('release-real',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_NESTED_EQ_RESULTS='+str(s/'release-real.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','nested_equalized_sound','--','--nocapture']),
 ('release-export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PARTIAL_REPEAT_RESULTS='+str(s/'release-export.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','partial_repeat_attached','--','--nocapture']),
 ('release-app',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli']),
 ('fmt-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
 print('Starting '+name,flush=True)
 result=subprocess.run(['python3',str(s/'run.py'),name,*command])
 if result.returncode: sys.exit(result.returncode)
