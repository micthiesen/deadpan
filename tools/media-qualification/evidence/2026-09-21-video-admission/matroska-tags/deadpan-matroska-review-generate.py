from pathlib import Path
b = Path('native/deadpan-source/tests/fixtures/limited709.mkv').read_bytes()
def vint(b,at,keep=False):
    n = 1
    while not b[at] & (1 << (8-n)): n += 1
    value = int.from_bytes(b[at:at+n], 'big')
    return (value if keep else value & ((1 << (7*n))-1)), n

def children(b,start,end):
    out=[]
    while start<end:
        identity,i=vint(b,start,True); length,s=vint(b,start+i)
        out.append((identity,start,start+i+s,start+i+s+length));start+=i+s+length
    return out

def size(n):
    k=next(k for k in range(1,9) if n<(1<<(7*k))-1)
    return (n|(1<<(7*k))).to_bytes(k,'big')

def element(i,body): return i.to_bytes((i.bit_length()+7)//8,'big')+size(len(body))+body

def uint(i,n): return element(i,n.to_bytes(max(1,(n.bit_length()+7)//8),'big'))
roots=children(b,0,len(b)); ebml=b[roots[0][1]:roots[0][3]]
segment=roots[1];parts={i:b[start:end] for i,start,body,end in children(b,segment[2],segment[3])}
for depth in (1,10,13):
    nested=b''
    for i in reversed(range(depth)):
        nested=element(0x67c8, element(0x45a3,f'N{i}'.encode())+element(0x4487,b'x'*64)+element(0x447a,b'en')+uint(0x4484,1)+nested)
    tags=element(0x1254c367,element(0x7373,nested))
    data=ebml+element(0x18538067,parts[0x1549a966]+parts[0x1654ae6b]+tags+parts[0x1f43b675])
    Path(f'/tmp/deadpan-matroska-tag-amplification-{depth}.mkv').write_bytes(data)
    print(depth,len(data))
