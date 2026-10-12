import hashlib,json,pathlib,subprocess
s=pathlib.Path(__file__).resolve().parent
old=s/'old-deadpan-cli'; new=s/'qualified-debug-cli'; package=s/'old-baseline.deadpan'
def run(binary,*args):
 r=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True)
 return dict(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),exit=r.returncode,stdout=r.stdout,stderr=r.stderr)
before=run(new,'project','dump',package,'--json'); assert before['exit']==0
result=run(old,'project','dump',package,'--json')
assert result['exit']!=0
assert 'split sound owners inside Repeat need an occurrence route' in result['stderr']
after=run(new,'project','dump',package,'--json'); assert after['exit']==0 and after['stdout']==before['stdout']
(s/'old-baseline-result.json').write_text(json.dumps(dict(old=result,new_before=before,new_after=after,historical_replay_refused=True,document_unchanged=True),indent=2)+'\n')
print(result['stderr'])
