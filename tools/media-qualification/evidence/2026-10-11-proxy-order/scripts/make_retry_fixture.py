from pathlib import Path
import hashlib,json,os,subprocess,time

root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
worker=repo/'target/debug/deadpan-media-worker'
original=repo/'native/deadpan-source/tests/fixtures/cfr-bframes.mp4'
output=root/'cfr-retry-success.mp4'
request=dict(protocol=2,input_byte_length=original.stat().st_size,stream_index=0,
             source_width=320,source_height=180,width=320,height=180,
             time_base_num=1,time_base_den=30000,sar_num=1,sar_den=1,
             rotation_quarter_turns=0,transfer='bt709',primaries='bt709',quality=60,
             frames=120,decode_threads=4,max_output_bytes=32*1024*1024,
             timeout_ms=60000,output_offset=0,range=None)
start=time.monotonic()
with original.open('rb') as source,os.fdopen(os.open(output,os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600),'w+b') as target,(root/'make-retry-fixture.log').open('x') as log:
    result=subprocess.run([str(worker),'proxy',json.dumps(request,separators=(',',':'))],stdin=source,stdout=target,stderr=log,timeout=65)
events=[]
for line in (root/'make-retry-fixture.log').read_text().splitlines():
    try:events.append(json.loads(line))
    except json.JSONDecodeError:pass
reply=next((x for x in reversed(events) if x.get('status') in ('success','failure')),None)
(root/'make-retry-fixture.json').write_text(json.dumps({'request':request,'exit':result.returncode,'seconds':time.monotonic()-start,'reply':reply,'worker':str(worker),'worker_sha256':hashlib.sha256(worker.read_bytes()).hexdigest(),'original_sha256':hashlib.sha256(original.read_bytes()).hexdigest(),'output_sha256':hashlib.sha256(output.read_bytes()).hexdigest()},indent=2)+'\n')
assert result.returncode==0 and reply['status']=='success',reply
(root/'cfr-retry-success.json').write_text(json.dumps(reply,separators=(',',':'))+'\n')
print(json.dumps(reply))
