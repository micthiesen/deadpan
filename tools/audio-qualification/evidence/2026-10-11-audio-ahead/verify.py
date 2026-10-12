from pathlib import Path
import subprocess
root=Path(__file__).parent

def run(name,args,pin=False):
 subprocess.run(['python3',str(root/'run.py'),name,*args],check=True)
 if pin:subprocess.run(['python3',str(root/'pin-binaries.py'),name],check=True)

run('format',['cargo','fmt','--all'])
run('suites',['cargo','test','--locked','-p','deadpan-playback','-p','deadpan-diagnostics'],True)
run('doctor',['cargo','test','--locked','-p','deadpan-cli','--test','doctor'],True)
run('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings'])
run('release-build',['cargo','build','--release','--locked','-p','deadpan-cli','--bin','deadpan-cli','--example','perf'])
