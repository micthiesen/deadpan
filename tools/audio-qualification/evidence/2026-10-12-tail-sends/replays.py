import subprocess,sys
from pathlib import Path
scratch=Path(__file__).resolve().parent
for name,scenario in [('replay-final','tail-sends'),('hold-effects','hold-effects'),('audio-treatments','audio-treatments'),('equalizer','equalizer')]:
 result=subprocess.run([sys.executable,str(scratch/'run.py'),name,'target/release/deadpan-app','--ui-check','--scenario',scenario,'--mode','visual','--output',str(scratch/name),'--retain-projects'])
 if result.returncode:sys.exit(result.returncode)
