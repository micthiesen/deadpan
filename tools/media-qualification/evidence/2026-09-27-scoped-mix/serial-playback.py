from pathlib import Path
import hashlib,json,os,re,subprocess,time
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-sound-events-20260927');out=s/'playback-serial';out.mkdir(exist_ok=False)
binary=json.loads((s/'playback-retry/report.json').read_text())['command'][0];command=[binary,'--test-threads=1'];start=time.monotonic()
with (out/'test.log').open('w') as log:
    p=subprocess.run(command,cwd=r,env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix',RUST_BACKTRACE='0'),stdout=log,stderr=subprocess.STDOUT)
rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',(out/'test.log').read_text())
record={'command':command,'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'exit_code':p.returncode,'seconds':time.monotonic()-start,'tests':dict(zip(('passed','failed','ignored'),(sum(int(x[i]) for x in rows) for i in range(3))))}
(out/'report.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));raise SystemExit(p.returncode)
