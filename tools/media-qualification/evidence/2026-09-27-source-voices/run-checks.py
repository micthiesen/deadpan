from pathlib import Path
import hashlib,json,os,re,subprocess,sys,time
repo=Path('/Users/michael/Code/deadpan');out=Path(sys.argv[1]);commands=json.loads(sys.argv[2]);out.mkdir(parents=True,exist_ok=False)
results=[]
for index,command in enumerate(commands):
    print(json.dumps({'started':command}),flush=True);start=time.monotonic()
    with (out/f'{index}.log').open('w') as log:
        p=subprocess.run(command,cwd=repo,env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'),stdout=log,stderr=subprocess.STDOUT)
    rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',(out/f'{index}.log').read_text())
    result={'command':command,'exit_code':p.returncode,'seconds':time.monotonic()-start,'log':f'{index}.log'}
    if rows:result['tests']=dict(zip(('passed','failed','ignored'),(sum(int(x[i]) for x in rows) for i in range(3))))
    results.append(result);(out/'report.json').write_text(json.dumps({'commands':results},indent=2)+'\n');print(json.dumps(result),flush=True)
    if p.returncode:raise SystemExit(p.returncode)
