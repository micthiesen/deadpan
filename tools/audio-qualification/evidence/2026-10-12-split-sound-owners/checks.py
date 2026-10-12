import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
commands=[
 ('fmt-final',['cargo','fmt','--all']),
 ('core-audio-store',['cargo','test','--locked','-p','deadpan-core','-p','deadpan-plan','-p','deadpan-audio','-p','deadpan-playback','-p','deadpan-store']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-audio',['cargo','test','--locked','--release','-p','deadpan-audio','--test','audio_definition','split_owner','--','--nocapture']),
 ('release-store',['cargo','test','--locked','--release','-p','deadpan-store','--test','source_registration','split_sound_clocks','--','--nocapture']),
 ('release-export',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_SPLIT_SOUND_RESULTS='+str(s/'release-export.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','split_attached_sounds','--','--nocapture']),
 ('release-app',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli']),
 ('fmt-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
 print('Starting '+name,flush=True)
 result=subprocess.run(['python3',str(s/'run.py'),name,*command])
 if result.returncode: sys.exit(result.returncode)
