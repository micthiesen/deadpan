from pathlib import Path
import subprocess
root=Path(__file__).parent

def run(name,args):
    subprocess.run(['python3',str(root/'run.py'),name,*args],check=True)

run('format',['cargo','fmt','--all'])
run('playback-suite',['cargo','test','--locked','-p','deadpan-playback'])
run('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings'])
run('native-replay',['cargo','xtask','replays','--scenario','audio-warming','--output',str(root/'native-replay'),'--jobs','1'])
