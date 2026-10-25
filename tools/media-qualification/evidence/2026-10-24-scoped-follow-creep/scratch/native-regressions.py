from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import os,subprocess
s=Path(__file__).resolve().parent
binary=s/'bin-first/deadpan-app'
os.environ['DEADPAN_REQUIRE_PHYSICAL_MAC']='1'
def run(scenario):
 label='native-'+scenario+'-final'
 return subprocess.run(['python',str(s/'run.py'),'--binary',str(binary),label,'--',str(binary),'--ui-check','--scenario',scenario,'--output',str(s/label),'--retain-projects','--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'],check=False).returncode
with ThreadPoolExecutor(max_workers=3) as pool:
 results=dict(zip(['repeat-follow-creep','retime-follow-creep','zoom','sequence-follow-creep','hold-follow-creep'],pool.map(run,['repeat-follow-creep','retime-follow-creep','zoom','sequence-follow-creep','hold-follow-creep'])))
print(results)
if any(results.values()):raise SystemExit(1)
