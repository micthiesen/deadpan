from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows=[]
for name,args in [
    ('native-tests',['test','--locked','-p','deadpan-source']),
    ('native-clippy',['clippy','--locked','-p','deadpan-source','--all-targets','--','-D','warnings']),
]:
    print('START',name,flush=True);start=time.monotonic()
    with (root/(name+'.log')).open('w') as log:
        result=subprocess.run(['cargo',*args],cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
    rows.append(dict(name=name,command=['cargo',*args],exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256((root/(name+'.log')).read_bytes()).hexdigest()))
    (root/'native-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
    print('FINISH',name,result.returncode,flush=True)
    if result.returncode:raise SystemExit(result.returncode)
