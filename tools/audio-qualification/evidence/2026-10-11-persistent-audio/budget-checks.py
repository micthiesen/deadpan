from pathlib import Path
import subprocess
root=Path(__file__).parent
def run(name, *args):
    subprocess.run(['python3',str(root/'run.py'),name,*args],check=True)
    if args[:2] == ('cargo','test'):
        subprocess.run(['python3',str(root/'pin-binaries.py'),name],check=True)
run('budget-format','cargo','fmt','--all')
run('budget-suites','cargo','test','--locked','-p','deadpan-audio','-p','deadpan-playback')
run('budget-clippy','cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings')
run('budget-release','cargo','build','--release','--locked','-p','deadpan-cli','--bin','deadpan-cli','--example','perf')
