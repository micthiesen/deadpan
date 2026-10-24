import subprocess, sys
from pathlib import Path
p=Path(__file__).resolve().parent
for name in ['targets','faces','zoom']:
    subprocess.run([sys.executable,str(p/'run.py'),'--binary',str(p/'bin-final/deadpan-app'),'native-'+name+'-final','--',str(p/'bin-final/deadpan-app'),'--ui-check','--scenario',name,'--output',str(p/('native-'+name+'-final')),'--retain-projects','--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'],check=True)
