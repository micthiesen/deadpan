from pathlib import Path
import hashlib,json,shutil,struct,subprocess
root=Path('/tmp/deadpan-word-refinement-2026-10-25/signature-layout')
source=Path('/Users/michael/Code/deadpan/target/bundle/release/deadpan-align')
ort=Path('/Users/michael/Library/Developer/Deadpan/onnxruntime-1.31.0-qualified-1/prefix/lib/libonnxruntime.1.31.0.dylib')
records=[]
def describe(path):
 b=path.read_bytes()
 ncmds=struct.unpack_from('<I',b,16)[0]
 at=32; sig=None;linkedit=None
 for _ in range(ncmds):
  cmd,size=struct.unpack_from('<II',b,at)
  if cmd==0x1d:
   off,count=struct.unpack_from('<II',b,at+8);sig=(at,off,count)
  if cmd==0x19 and b[at+8:at+24].rstrip(b'\0')==b'__LINKEDIT':
   linkedit=(at,*struct.unpack_from('<QQ',b,at+40))
  at+=size
 d={'path':str(path),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest(),'signature':sig,'linkedit':linkedit}
 if sig:
  at,off,count=sig
  magic,length,n=struct.unpack_from('>III',b,off)
  tail=b[off+length:off+count]
  nz=[i for i,x in enumerate(tail) if x]
  blobs=[]
  if magic==0xfade0cc0 and n<100:
   for i in range(n):
    typ,rel=struct.unpack_from('>II',b,off+12+8*i)
    bm,bl=struct.unpack_from('>II',b,off+rel)
    blobs.append({'type':typ,'offset':rel,'magic':hex(bm),'length':bl})
  d.update({'superblob_magic':hex(magic),'superblob_length':length,'signature_end':off+count,'superblob_end':off+length,'tail_bytes':len(tail),'tail_nonzero_count':len(nz),'tail_nonzero_first_relative':nz[:10],'tail_hex':tail.hex(),'blobs':blobs})
  if magic==0xfade0cc0 and length<=count and not nz and linkedit:
   canonical=bytearray(b[:off])
   canonical[at+12:at+16]=b'\0'*4
   la=linkedit[0]
   canonical[la+32:la+40]=b'\0'*8
   canonical[la+48:la+56]=b'\0'*8
   cpu,subtype=struct.unpack_from('<II',b,4)
   d['strict_content_sha256']=hashlib.sha256(b'deadpan-macho-content-v1\0'+struct.pack('>IIQ',cpu,subtype,len(canonical))+canonical).hexdigest()
 return d
def run(label,args):
 result=subprocess.run(args,capture_output=True)
 records.append({'label':label,'command':list(map(str,args)),'exit_code':result.returncode,'stdout':result.stdout.decode(errors='replace'),'stderr':result.stderr.decode(errors='replace')})
 if result.returncode: raise RuntimeError(records[-1])
for mode in ['force','remove-first','remove-first-repeat']:
 stage=root/mode/'Deadpan.app/Contents'
 (stage/'MacOS').mkdir(parents=True)
 (stage/'Frameworks').mkdir()
 worker=stage/'MacOS/deadpan-align'; library=stage/'Frameworks/libonnxruntime.1.dylib'
 shutil.copyfile(source,worker);worker.chmod(0o755)
 shutil.copyfile(ort,library);library.chmod(0o644)
 snapshots={'original_worker':describe(worker),'original_ort':describe(library)}
 run(mode+'-rewrite-ort',['install_name_tool','-id','@rpath/libonnxruntime.1.dylib',str(library)])
 run(mode+'-rewrite-worker',['install_name_tool','-change','@rpath/libonnxruntime.1.dylib','@rpath/libonnxruntime.1.dylib','-delete_rpath',str(ort.parent),'-add_rpath','@executable_path/../Frameworks',str(worker)])
 snapshots['relocated_worker']=describe(worker);snapshots['relocated_ort']=describe(library)
 for path,identifier in [(library,None),(worker,'dev.deadpan.Deadpan.deadpan-align')]:
  if mode!='force':run(mode+'-remove-'+path.name,['codesign','--remove-signature',str(path)])
  cmd=['codesign','--force','--options','runtime','--timestamp=none','-s','-']
  if identifier:cmd+=['-i',identifier]
  run(mode+'-sign-'+path.name,cmd+[str(path)])
  run(mode+'-verify-'+path.name,['codesign','--verify','--strict',str(path)])
 snapshots['signed_worker']=describe(worker);snapshots['signed_ort']=describe(library)
 (root/(mode+'.json')).write_text(json.dumps(snapshots,indent=2)+'\n')
(root/'commands.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps({mode: {key:{k:v for k,v in item.items() if k in ['bytes','sha256','signature','superblob_length','tail_bytes','tail_nonzero_count','strict_content_sha256']} for key,item in json.loads((root/(mode+'.json')).read_text()).items()} for mode in ['force','remove-first','remove-first-repeat']},indent=2))
