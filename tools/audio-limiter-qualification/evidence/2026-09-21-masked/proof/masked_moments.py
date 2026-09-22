#!/usr/bin/env python3
import json, math
from pathlib import Path
import numpy as np

COND = Path('/tmp/deadpan-conditioner-20260921')
OUT = Path('/tmp/deadpan-joint-proof-20260921')
c = np.array(json.loads((COND/'kaiser255-beta16.json').read_text())['coefficients_f64'])
co = np.arange(-127,128)

def measure(name, kernels, qo, phases):
    rows=[]
    for r,p in zip(kernels,phases):
        # Any binary output mask m[q]:
        # sum_k |sum_q r[q]m[q]c[k-q]| |k|
        # <= sum_q |r[q]| sum_j |c[j]| |q+j|.
        per_q=np.array([math.fsum(np.abs(c)*np.abs(co+int(q))) for q in qo])
        d=math.fsum(np.abs(r)*per_q)
        a=math.fsum(np.abs(r))*math.fsum(np.abs(c))
        rows.append(dict(phase=float(p), raw_nonzero_support=[int(qo[np.flatnonzero(r)[0]]),int(qo[np.flatnonzero(r)[-1]])], arbitrary_mask_l1_bound=a, arbitrary_mask_integer_origin_distance_bound=d))
    return dict(name=name, phases=rows, max_arbitrary_mask_l1_bound=max(x['arbitrary_mask_l1_bound'] for x in rows), max_arbitrary_mask_integer_origin_distance_bound=max(x['arbitrary_mask_integer_origin_distance_bound'] for x in rows))

source=json.loads((COND/'combined-kernels.json').read_text())
bs=np.array(source['bs1770_table_rows_oldest_to_newest']).T
# Parent prototype convention: table row k has integer offset k-6.
bs_result=measure('bs1770_rows_k_minus_6',bs,np.arange(-6,6),[-1/8,-3/8,-5/8,-7/8])

radius=256
qo=np.arange(1-radius,radius+1)
ks=[]
for p in np.arange(8)/8:
    delta=p-qo
    window=np.i0(10*np.sqrt(np.maximum(0,1-(delta/radius)**2)))/np.i0(10)
    r=np.where(np.abs(delta)<radius,np.sinc(delta)*window,0)
    r/=sum(r)
    if p==0:
        r[:]=0; r[radius-1]=1
    ks.append(r)
k_result=measure('kaiser256',ks,qo,np.arange(8)/8)
raw_result=measure('raw_output',[np.array([1.0])],np.array([0]),[0])

d=max(raw_result['max_arbitrary_mask_integer_origin_distance_bound'],bs_result['max_arbitrary_mask_integer_origin_distance_bound'],k_result['max_arbitrary_mask_integer_origin_distance_bound'])
C=10**(-1/20); B=16
out=dict(conditioner_support=[-127,127],input_magnitude_bound=B,ceiling_linear=C,families=[raw_result,bs_result,k_result],worst_arbitrary_mask_distance_bound=d,worst_W_bound=B*d,slope_candidates=[])
for den in [4096,8192,16384]:
    L=1/den
    out['slope_candidates'].append(dict(L=f'1/{den}',lookahead=den,lookahead_ms=den/48,strict_margin=C-L*B*d))
(OUT/'masked-moments.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
