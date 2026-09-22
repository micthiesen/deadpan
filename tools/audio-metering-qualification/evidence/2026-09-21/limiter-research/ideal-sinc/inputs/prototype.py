import json
import math
import time
import sys
from pathlib import Path
import numpy as np

RATE=48000
RADIUS=int(sys.argv[1]) if len(sys.argv)>1 else 64
OVERSAMPLE=8
ATTACK=int(sys.argv[2]) if len(sys.argv)>2 else 512
RELEASE=4800
DB=float(sys.argv[3]) if len(sys.argv)>3 else -1.25
KIND=sys.argv[4] if len(sys.argv)>4 else 'bh4'
LIMIT=10**(DB/20)-1e-6

def kernels(radius, phases, kind):
    offsets=np.arange(1-radius, radius+1)
    result=[]
    for phase in range(phases):
        t=phase/phases
        d=t-offsets
        if kind=='bh4':
            w=.35875+.48829*np.cos(np.pi*d/radius)+.14128*np.cos(2*np.pi*d/radius)+.01168*np.cos(3*np.pi*d/radius)
        else:
            w=np.i0(10*np.sqrt(np.maximum(0,1-(d/radius)**2)))/np.i0(10)
        h=np.where(np.abs(d)<radius,np.sinc(d)*w,0)
        h/=sum(h)
        if phase==0:
            h[:]=0
            h[radius-1]=1
        result.append(h)
    return offsets,np.array(result)

J,H=kernels(RADIUS,OVERSAMPLE,KIND)

def peak(x,radius=256,phases=32,kind='kaiser'):
    _,hh=kernels(radius,phases,kind)
    xp=np.pad(x,((2*radius,2*radius),(0,0)))
    return max(np.max(np.abs(np.correlate(xp[:,ch],h,'valid'))) for ch in range(2) for h in hh)

def limit(x):
    n=len(x)
    # Control coordinates -R .. N+R-1 include all leading/trailing FIR context.
    xp=np.pad(x,((2*RADIUS-1,2*RADIUS),(0,0)))
    b=np.ones(n+2*RADIUS)
    for ch in range(2):
        for h in H:
            value=np.abs(np.correlate(xp[:,ch],h,'valid'))
            weighted=np.correlate(np.abs(xp[:,ch]),np.abs(h)*np.abs(J),'valid')
            margin=LIMIT-weighted/ATTACK
            assert min(margin)>0
            bound=np.divide(margin,value,out=np.ones_like(value),where=value>0)
            b=np.minimum(b,bound)
    f=np.ones_like(b)
    for i in range(len(b)-1,-1,-1):
        f[i]=min(b[i], (f[i+1]+1/ATTACK) if i+1<len(b) else 1)
    g=np.ones_like(b)
    for i in range(len(b)):
        g[i]=min(f[i], (g[i-1]+1/RELEASE) if i else 1)
    assert min(g)>0 and max(g)<=1
    assert np.max(np.abs(np.diff(g)))<=1/ATTACK+1e-12
    y=(x*g[RADIUS:RADIUS+n,None]).astype(np.float32)
    return y,g[RADIUS:RADIUS+n]

n=8192
t=np.arange(n)
rng=np.random.default_rng(682301)
cases={}
for frequency in [200,3000,12000,21600,23000,23990]:
 for amplitude in [.1,.86,1,4,16]:
  mono=amplitude*np.sin(2*np.pi*frequency*(t+.5)/RATE)
  cases[f'tone-{frequency}-{amplitude}']=np.array([mono,mono*.25]).T
cases['random16']=rng.uniform(-16,16,(n,2))
cases['impulse16']=np.zeros((n,2));cases['impulse16'][n//2]=[16,-4]
cases['alternating']=np.repeat((.95*(-1.)**t)[:,None],2,axis=1)
cases['two-positive-full-scale-samples']=np.zeros((n,2));cases['two-positive-full-scale-samples'][n//2:n//2+2]=1
cases['edge-burst']=np.zeros((n,2));cases['edge-burst'][:7]=16;cases['edge-burst'][-3:]=-16
results=[]
start=time.monotonic()
for name,xx in cases.items():
 x=xx.astype(np.float32)
 y,g=limit(x)
 q=peak(y)
 item={'case':name,'sample_peak':float(abs(y).max()),'oracle_true_peak':float(q),'oracle_dbtp':float(20*math.log10(q)) if q else None,'gain_min':float(g.min()),'gain_max':float(g.max()),'input_unchanged':bool(np.array_equal(x,y))}
 assert item['sample_peak']<=1
 item['passes_ceiling']=bool(q<=10**(-1/20))
 if KIND=='kaiser':
  import struct
  data=y.astype('<f4').tobytes()
  header=struct.pack('<4sI4s4sIHHIIHH4sI',b'RIFF',36+len(data),b'WAVE',b'fmt ',16,3,2,RATE,RATE*8,8,32,b'data',len(data))
  Path('/tmp/deadpan-master-20260921/'+name+'.wav').write_bytes(header+data)
 results.append(item)
print(json.dumps({'numpy':np.__version__,'kind':KIND,'radius':RADIUS,'attack':ATTACK,'internal_dbtp':DB,'seconds':time.monotonic()-start,'kernel_l1':float(np.max(np.sum(abs(H),axis=1))),'kernel_distance_sum':float(np.max(np.sum(abs(H)*abs(J),axis=1))),'worst_margin_for_input16':LIMIT-16*np.max(np.sum(abs(H)*abs(J),axis=1))/ATTACK,'results':results},indent=2))
