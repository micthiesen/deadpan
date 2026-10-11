from pathlib import Path
import os,subprocess
r=Path(__file__).parent
env=dict(os.environ,DEADPAN_GENERATED_TRANSITIONS_PACKAGE='/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan',DEADPAN_GENERATED_PREVIEW_PACKAGE='/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan')
base=['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness']
steps=[
 ('native-full',['cargo','test','--locked','--release','-p','deadpan-source','--','--test-threads=1']),
 ('worker-tests',base+['worker::','--','--test-threads=1']),
 ('fixture-tests',base+['generated_playback_tests','--','--ignored','--nocapture','--test-threads=1']),
 ('slices-4k-test',base+['generated_preview_slices_equal','--','--ignored','--nocapture','--test-threads=1']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('format-check',['cargo','fmt','--all','--check']),
 ('sanitizers',['python3','tools/media-qualification/host/build_sanitized.py','--work',str(r/'sanitizers'),'--package','deadpan-source']),
]
for name,command in steps:
 step_env=dict(env)
 if name=='slices-4k-test':step_env['DEADPAN_GENERATED_TRANSITIONS_PACKAGE']='/tmp/deadpan-generated-loops-20261011/4k/two-holds.deadpan'
 subprocess.run(['python3',str(r/'run.py'),name,*command],env=step_env,check=True)
