from pathlib import Path
import subprocess
root=Path(__file__).parent

def run(name,args,pin=False):
    subprocess.run(['python3',str(root/'run.py'),name,*args],check=True)
    if pin:
        subprocess.run(['python3',str(root/'pin-binaries.py'),name],check=True)

run('audio-playback', ['cargo','test','--locked','--no-fail-fast','-p','deadpan-audio','-p','deadpan-playback'],True)
run('transport', ['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--lib','transport::'],True)
run('clippy', ['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings'])
run('format-check',['cargo','fmt','--all','--','--check'])
run('replays',['cargo','xtask','replays','--scenario','original-layout,original-layout-long,sound-playback,room-tone','--output',str(root/'replays'),'--jobs','1'])
