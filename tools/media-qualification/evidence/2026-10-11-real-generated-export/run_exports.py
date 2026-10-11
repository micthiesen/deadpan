from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
previous=Path('/tmp/deadpan-generated-playback-performance-20261011')
cli=previous/'bin-verified/deadpan-cli'
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps=json.loads((root/'steps.json').read_text()) if (root/'steps.json').exists() else {}
def read_result(label):
 text=(root/(label+'.stdout')).read_text().lstrip()
 decoder=json.JSONDecoder();value=None
 while text:
  value,length=decoder.raw_decode(text);text=text[length:].lstrip()
 return value
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def run(label,args):
 command=[str(cli),*map(str,args)]
 if label in steps:
  previous=steps[label]
  assert previous['exit']==0 and previous['command']==command,label
  assert previous['stdout_sha256']==sha(root/(label+'.stdout')),label
  assert previous['stderr_sha256']==sha(root/(label+'.stderr')),label
  return read_result(label)
 start=time.monotonic()
 with (root/(label+'.stdout')).open('x') as out,(root/(label+'.stderr')).open('x') as err:
  result=subprocess.run(command,cwd=repo,env=env,stdout=out,stderr=err)
 steps[label]=dict(command=command,exit=result.returncode,seconds=time.monotonic()-start,stdout_sha256=sha(root/(label+'.stdout')),stderr_sha256=sha(root/(label+'.stderr')))
 (root/'steps.json').write_text(json.dumps(steps,indent=2)+'\n')
 print(label,json.dumps(steps[label]),flush=True)
 assert result.returncode==0,label
 return read_result(label)
for label,total,hold in [('4k',615,15),('1080p',1830,30)]:
 package=root/(label+'.deadpan');exports=root/(label+'-exports');exports.mkdir(exist_ok=True)
 run(label+'-copy',['project','copy-portable',previous/(label+'-fallback.deadpan'),package])
 document=run(label+'-before',['project','dump',package,'--json'])
 result=run(label+'-render',['render',package,'--output',exports,'--name','accepted.mp4'])
 assert result['status']['outcome']=='published',result
 movie=result['status']['receipt']['movie']
 selected=sorted(set([0,14,15+hold,total-1,*range(15,15+hold)]))
 run(label+'-verify',['verify-export',package,'--movie',movie,'--revision',document['revision_id'],'--frames',','.join(map(str,selected)),'--report',root/(label+'-verification.json')])
 after=run(label+'-after',['project','dump',package,'--json'])
 assert document==after,'render changed authored document'
print('both completed',flush=True)
