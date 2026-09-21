from pathlib import Path
import json,os,signal,subprocess,tempfile,time
root=Path('/tmp/deadpan-ltx2src-3392/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4')
run=Path(tempfile.mkdtemp(prefix='deadpan-mlx-keyframe-',dir='/tmp'))
command=['uv','run','--offline','--locked','--no-dev','--python','3.12.13','python','/tmp/deadpan-mlx-keyframe-smoke.py',str(run)]
env={key:os.environ[key] for key in ['PATH']}
env.update(HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1',DO_NOT_TRACK='1',PYTHONNOUSERSITE='1',PYTHONUNBUFFERED='1',TOKENIZERS_PARALLELISM='false')
(run/'launch.json').write_text(json.dumps({'command':command,'environment':env,'guard_maximum_seconds':1800,'guard_rss_bytes':80*1024**3},indent=2)+'\n')
print('Run: '+str(run),flush=True)
started=time.monotonic()
readings=[]
with (run/'runtime.log').open('w') as log:
    process=subprocess.Popen(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
    reason=None
    while process.poll() is None:
        time.sleep(2)
        if time.monotonic()-started>1800:
            reason='30 minute qualification deadline'
        pid_file=run/'pid'
        if pid_file.exists():
            pid=pid_file.read_text().strip()
            if pid.isdecimal():
                rss=subprocess.run(['ps','-o','rss=','-p',pid],capture_output=True,text=True).stdout.strip()
                if rss.isdecimal():
                    amount=int(rss)*1024
                    readings.append({'seconds':time.monotonic()-started,'rss_bytes':amount})
                    if amount>80*1024**3: reason='resident memory exceeded 80 GiB guard'
        if reason:
            os.killpg(process.pid,signal.SIGKILL)
            break
    code=process.wait()
(run/'supervision.json').write_text(json.dumps({'exit_code':code,'guard_stop':reason,'elapsed_seconds':time.monotonic()-started,'rss_readings':readings},indent=2)+'\n')
print(json.dumps({'run':str(run),'exit_code':code,'guard_stop':reason}),flush=True)
print((run/'runtime.log').read_text()[-7000:],flush=True)
raise SystemExit(code if code>=0 else 1)
