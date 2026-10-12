import hashlib,json,math,pathlib,struct,subprocess
s=pathlib.Path(__file__).resolve().parent
p=pathlib.Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
command=['clang','-O2','-Wall','-Wextra','-Werror',str(s/'probe.c'),'-I'+str(p/'include'),'-L'+str(p/'lib'),'-Wl,-rpath,'+str(p/'lib'),'-lavformat','-lavcodec','-lavutil','-o',str(s/'probe')]
calls=[]
def call(command):
 r=subprocess.run(command,capture_output=True,text=True)
 calls.append(dict(command=command,exit=r.returncode,stdout=r.stdout,stderr=r.stderr))
 assert r.returncode==0,r.stderr
call(command)
reference=list(struct.iter_unpack('<ff',(s/'reference.f32le').read_bytes()))
metadata=json.loads((s/'reference.json').read_text())
variants={'production':None,'default':[], 'no_pns':['aac_pns=0'], 'no_ms':['aac_ms=0'], 'no_tns':['aac_tns=0'], 'no_is':['aac_is=0'], 'no_pns_ms':['aac_pns=0','aac_ms=0'], 'fast':['aac_coder=fast']}
rows={}
for name,options in variants.items():
 output=s/(name+'.decoded')
 call([str(s/'probe'),'decode' if options is None else 'encode',metadata['movie'] if options is None else str(s/'reference.f32le'),str(output),*(options or [])])
 data=output.read_bytes(); offset=0; decoded={}; frames=[]
 while offset<len(data):
  pts,count=struct.unpack_from('<qI',data,offset); offset+=12
  samples=list(struct.iter_unpack('<ff',data[offset:offset+count*8])); offset+=count*8
  assert len(samples)==count
  for i,pair in enumerate(samples):
   assert pts+i not in decoded
   decoded[pts+i]=pair
  frames.append([pts,count])
 assert all(i in decoded for i in range(len(reference)))
 blocks=[]
 for start in range(0,len(reference),480):
  end=min(start+480,len(reference)); n=(end-start)*2
  ref=sum(v*v for pair in reference[start:end] for v in pair)/n
  actual=sum(v*v for i in range(start,end) for v in decoded[i])/n
  error=sum((reference[i][ch]-decoded[i][ch])**2 for i in range(start,end) for ch in range(2))/n
  level=lambda v:10*math.log10(max(v,1e-300))
  blocks.append(dict(start=start,reference_dbfs=level(ref),decoded_dbfs=level(actual),snr_db=level(ref)-level(error),level_difference_db=abs(level(ref)-level(actual))))
 gated=[b for b in blocks if b['reference_dbfs']>=-60]
 rows[name]=dict(frames=frames,blocks=blocks,compared=len(gated),min_snr_db=min(b['snr_db'] for b in gated),low_snr=[b for b in gated if b['snr_db']<1.5],max_level_db=max(b['level_difference_db'] for b in gated),decoded_sha256=hashlib.sha256(data).hexdigest())
 print(name,rows[name]['min_snr_db'],len(rows[name]['low_snr']),flush=True)
(s/'probe-results.json').write_text(json.dumps(dict(variants=rows,calls=calls,probe_sha256=hashlib.sha256((s/'probe').read_bytes()).hexdigest()),indent=2)+'\n')
