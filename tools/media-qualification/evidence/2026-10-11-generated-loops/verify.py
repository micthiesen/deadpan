from pathlib import Path
import os,subprocess
root=Path(__file__).parent
env=dict(os.environ,DEADPAN_GENERATED_TRANSITIONS_PACKAGE='/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan',DEADPAN_GENERATED_PREVIEW_PACKAGE='/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan')
base=['cargo','test','--locked','--release','-p','deadpan-app','--features','ui-harness']
commands=[
 ('worker-tests',base+['worker::','--','--test-threads=1']),
 ('fixture-tests',base+['generated_playback_tests','--','--ignored','--nocapture','--test-threads=1']),
 ('original-4k-test',base+['starting_on_generated_prepares_original','--','--ignored','--nocapture','--test-threads=1']),
 ('option-tests',base+['ui_harness::tests','--','--test-threads=1']),
 ('coverage-tests',base+['preview::harness::generated_playback::tests','--','--test-threads=1']),
 ('build',['cargo','build','--locked','--release','-p','deadpan-app','--features','ui-harness']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('format-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
    step_env=dict(env)
    if name=='original-4k-test':step_env['DEADPAN_GENERATED_TRANSITIONS_PACKAGE']=str(root/'4k/two-holds.deadpan')
    subprocess.run(['python3',str(root/'run.py'),name,*command],env=step_env,check=True)
