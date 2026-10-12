import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
checks=[
 ('fmt', ['cargo','fmt','--all','--check']),
 ('audio-playback', ['cargo','test','--locked','-p','deadpan-audio','-p','deadpan-playback']),
 ('clippy', ['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-loop', ['cargo','test','--locked','--release','-p','deadpan-playback','treated_sound_loops_keep_full_history_across_seams_and_resumed_laps','--','--nocapture']),
 ('release-tail', ['cargo','test','--locked','--release','-p','deadpan-audio','--test','stages','allocated_tails_use_treated_event_input_and_keep_history_on_cold_seek','--','--nocapture']),
 ('release', ['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','--features','ui-harness']),
 ('export', ['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PREVIEW_EXPORT_RESULTS='+str(s/'export-results.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','treated_event_preserves_preview_and_verified_export','--','--nocapture']),
]
for name,command in checks:
 print('Running '+name,flush=True)
 result=subprocess.run([sys.executable,str(s/'run.py'),name,*command])
 if result.returncode:sys.exit(result.returncode)
print('All checks passed',flush=True)
