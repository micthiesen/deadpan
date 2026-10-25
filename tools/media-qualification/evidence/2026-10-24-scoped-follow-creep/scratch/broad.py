from pathlib import Path
import os,subprocess
s=Path(__file__).resolve().parent
os.environ['CARGO_TARGET_DIR']='/tmp/deadpan-follow-creep-2026-10-24/target'
os.environ['DEADPAN_FFMPEG_PREFIX']='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
jobs=[
('core-plan-final',['cargo','test','--locked','-p','deadpan-core','-p','deadpan-plan']),
('app-units-final',['cargo','test','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app']),
('release-first',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli','-p','deadpan-media-worker','-p','deadpan-transcribe','-p','deadpan-track','--features','deadpan-app/ui-harness'])]
for label,cmd in jobs:
 code=subprocess.run(['python',str(s/'run.py'),label,'--',*cmd],check=False).returncode
 if code: raise SystemExit(f'{label} exited {code}')
