from pathlib import Path
import struct
p=Path('native/deadpan-source/tests/audio-fixtures/opus-mp4-mono-60.mp4')
b=bytearray(p.read_bytes())
ctts=struct.pack('>I4sIIII',24,b'ctts',0x01000000,1,3,(-4800)&0xffffffff)
for pos in [2201,2317,2453,2538,2598]:
 struct.pack_into('>I',b,pos,struct.unpack_from('>I',b,pos)[0]+24)
b[2859:2859]=ctts
Path('/tmp/deadpan-word-refinement-2026-10-25/negative-fixture/negative-opus.mp4').write_bytes(b)
