from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
cli=Path('/tmp/deadpan-generated-playback-performance-20261011/bin-verified/deadpan-cli')
env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
step_path=root/'chirp-v2-steps.json'
steps=json.loads(step_path.read_text()) if step_path.exists() else {}
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def result(label):
 text=(root/(label+'.stdout')).read_text().lstrip();decoder=json.JSONDecoder();value=None
 while text:
  value,length=decoder.raw_decode(text);text=text[length:].lstrip()
 return value
def run(label,command):
 command=list(map(str,command))
 if label in steps:
  s=steps[label];assert s['exit']==0 and s['command']==command,label
  assert s['stdout_sha256']==sha(root/(label+'.stdout')) and s['stderr_sha256']==sha(root/(label+'.stderr')),label
  return result(label)
 start=time.monotonic()
 with (root/(label+'.stdout')).open('x') as out,(root/(label+'.stderr')).open('x') as err:
  proc=subprocess.run(command,cwd=repo,env=env,stdout=out,stderr=err)
 steps[label]=dict(command=command,exit=proc.returncode,seconds=time.monotonic()-start,stdout_sha256=sha(root/(label+'.stdout')),stderr_sha256=sha(root/(label+'.stderr')))
 step_path.write_text(json.dumps(steps,indent=2)+'\n');print(label,proc.returncode,round(steps[label]['seconds'],3),flush=True)
 assert proc.returncode==0,label
 return result(label)
def invoke(label,*args):return run(label,[cli,*args])
for name,size,rate,seed in [('chirp-v2-4k','3840x2160',30,813),('chirp-v2-1080p','1920x1080',60,814)]:
 media=root/(name+'.mp4');package=root/(name+'.deadpan');exports=root/(name+'-exports');exports.mkdir(exist_ok=True)
 def chirp(gain):return f'if(between(mod(t,0.5),0.2,0.3),{gain}*sin(2*PI*(300*(mod(t,0.5)-0.2)+6000*pow(mod(t,0.5)-0.2,2))),0)'
 audio=f"aevalsrc=exprs='{chirp(0.7)}|{chirp(-0.6)}':s=48000:d=2:c=stereo"
 run(name+'-media',['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-f','lavfi','-i',f'testsrc2=size={size}:rate={rate}:duration=2','-f','lavfi','-i',audio,'-frames:v',rate*2,'-c:v','libx264','-preset','veryfast','-g','150','-bf','3','-pix_fmt','yuv420p','-bsf:v','h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1','-color_range','tv','-colorspace','bt709','-color_primaries','bt709','-color_trc','bt709','-chroma_sample_location','left','-c:a','aac','-b:a','192k','-movflags','+faststart',media])
 invoke(name+'-create','project','create-original',package,media)
 original=invoke(name+'-original','project','dump',package,'--json')
 request=dict(protocol=1,project_id=original['project_id'],expected_revision=original['revision_id'],expected_bank_version=0,operation=dict(type='apply',parent=original['root'],cursor=rate,selected_child=None,visual_selection=None,new_revision=name+'-pause',program=dict(instructions=[dict(type='insert_pause',length=dict(unit='milliseconds',milliseconds=500))])))
 request_path=root/(name+'-pause-request.json');request_path.write_text(json.dumps(request,indent=2)+'\n')
 invoke(name+'-pause','macro',package,'--json',request_path)
 fallback=invoke(name+'-fallback','project','dump',package,'--json')
 holds=[node for node,value in fallback['nodes'].items() if value['kind']['type']=='hold']
 assert len(holds)==1,holds
 generation=invoke(name+'-generate','generate-hold',package,'--hold',holds[0],'--seed',seed)
 assert generation['state']=='Ready',generation
 invoke(name+'-accept','accept-hold',package,'--request',generation['request_id'])
 accepted=invoke(name+'-accepted','project','dump',package,'--json')
 rendered=invoke(name+'-render','render',package,'--output',exports,'--name','accepted.mp4')
 assert rendered['status']['outcome']=='published',rendered
 movie=rendered['status']['receipt']['movie']
 invoke(name+'-verify','verify-export',package,'--movie',movie,'--revision',accepted['revision_id'],'--every',1,'--report',root/(name+'-verification.json'))
 for label,start in [('early-chirp',9700),('silent-hold',57700),('shifted-chirp',81700)]:
  invoke(name+'-'+label,'inspect-audio',package,'--samples',start,start+256,'--limited')
 after=invoke(name+'-after','project','dump',package,'--json');assert accepted==after
print('both chirp exports completed',flush=True)
