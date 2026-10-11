from pathlib import Path
import subprocess
root=Path(__file__).parent

def run(name,args):
    subprocess.run(['python3',str(root/'run.py'),name,*args],check=True)
    if name=='transport-final':
        subprocess.run(['python3',str(root/'pin-binaries.py'),name],check=True)

run('format-check-final',['cargo','fmt','--all','--','--check'])
run('transport-final',['cargo','test','--locked','-p','deadpan-app','--bin','deadpan-app','--features','ui-harness','transport::tests'])
run('clippy-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings'])
run('regression-replays',['cargo','xtask','replays','--scenario','original-layout','--scenario','sound-playback','--output',str(root/'regression-replays'),'--jobs','1'])
