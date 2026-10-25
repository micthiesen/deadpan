from pathlib import Path
import struct
b=Path('native/deadpan-source/tests/fixtures/cfr-bframes.mp4').read_bytes()
def boxes(b):
 p=0
 while p<len(b):
  n,k=struct.unpack_from('>I4s',b,p);assert n>=8 and p+n<=len(b)
  yield k,b[p+8:p+n]
  p+=n
def atom(k,p):return struct.pack('>I4s',len(p)+8,k)+p
ctts=struct.pack('>I4sIIII',24,b'ctts',0x01000000,1,189,(-4800)&0xffffffff)
def rewrite(data,path=()):
 out=b''
 for k,p in boxes(data):
  q=path+(k,)
  if k in [b'moov',b'trak',b'mdia',b'minf',b'stbl']:
   if k==b'trak':p=rewrite(p,q+(b'audio' if b'soun' in p else b'video',))
   else:p=rewrite(p,q)
   if k==b'stbl' and b'audio' in q:p+=ctts
  elif k==b'stco':
   p=bytearray(p)
   count=struct.unpack_from('>I',p,4)[0]
   for i in range(count):
    o=8+i*4;struct.pack_into('>I',p,o,struct.unpack_from('>I',p,o)[0]+24)
  out+=atom(k,p)
 return out
Path('/tmp/deadpan-word-refinement-2026-10-25/negative-fixture/negative-aac.mp4').write_bytes(rewrite(b))
