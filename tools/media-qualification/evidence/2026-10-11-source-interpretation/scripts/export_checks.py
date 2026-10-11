from pathlib import Path
import os, subprocess, json, hashlib, time, re
root=Path(__file__).parent
env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', DEADPAN_PREVIEW_EXPORT_KEEP='1')
steps=[
 ('export-fmt',['cargo','fmt','--all']),
 ('export-build',['cargo','build','--locked','--workspace']),
 ('export-corrected',['cargo','test','--locked','-p','deadpan-cli','--test','preview_export','source_interpretation_exports','--','--nocapture']),
 ('source-sanitizers',['python3','tools/media-qualification/host/build_sanitized.py','--work',str(root/'source-sanitizers'),'--package','deadpan-source']),
]
rows=[]
for name,command in steps:
 print('START',name,flush=True)
 start=time.monotonic(); path=root/(name+'.log')
 with path.open('w') as log:
  result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
 output=path.read_text(); passed=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',output)))
 rows.append(dict(name=name,exit=result.returncode,command=command,passed=passed,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 (root/'export-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
 print('FINISH',name,result.returncode,'passed',passed,flush=True)
 if result.returncode:
  print(output[-16000:]); raise SystemExit(result.returncode)
