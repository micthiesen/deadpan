from pathlib import Path
import hashlib,json,os,re,subprocess,time
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-sound-events-20260927')
out=s/'playback-retry';out.mkdir(exist_ok=False)
log=(s/'gate-01/2.log').read_text()
paths=re.findall(r'Running unittests src/lib.rs \((target/debug/deps/deadpan_playback-[a-z0-9]+)\)',log)
assert len(paths)==1,paths
binary=r/paths[0]
cmd=[str(binary)]
start=time.monotonic()
with (out/'test.log').open('w') as f:
 p=subprocess.run(cmd,cwd=r,env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix',RUST_BACKTRACE='1'),stdout=f,stderr=subprocess.STDOUT)
rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',(out/'test.log').read_text())
record={'command':cmd,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'exit_code':p.returncode,'seconds':time.monotonic()-start,'tests':dict(zip(('passed','failed','ignored'),(sum(int(x[i]) for x in rows) for i in range(3)))),'test_threads':'default; unchanged'}
(out/'report.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
print((out/'test.log').read_text()[-6500:])
raise SystemExit(p.returncode)
