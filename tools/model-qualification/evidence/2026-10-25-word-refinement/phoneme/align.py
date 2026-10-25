"""Pinned 5M phoneme CTC inference and full transcript-constrained state path."""
import hashlib,importlib.util,itertools,json,math,re,time
from pathlib import Path,PosixPath
import numpy as np
import torch,torchaudio
P=Path(__file__).resolve().parent;UP=P/'upstream';RATE=16000;STEP=320
POLICY=dict(rule='phoneme-prior-pilot-1',sample_rate=RATE,feature_hop=160,output_step=STEP,
 max_audio_samples=30*RATE,max_phones=256,max_pronunciation_paths=128,
 word_mean_posterior_min=.5,phone_mean_posterior_min=.1,word_min_frames=1,
 blank_id=0,normalization='author sequential piecewise transform + pinned global stats; no waveform normalization',
 clock='centered Mel frame0 at input sample0; TDNN stride2; intervals [frame*320,end_frame*320), clipped at PCM end')
torch.set_num_threads(4);torch.use_deterministic_algorithms(True)
def pin(p):
 b=p.read_bytes();return dict(path=str(p),bytes=len(b),sha256=hashlib.sha256(b).hexdigest())
def labels():
 return ['<blk>']+[s.split()[0] for s in (UP/'cmudict.phones').read_text().splitlines() if s.strip()]+['<unk>']
LABELS=labels();IDS={p:i for i,p in enumerate(LABELS)}
def load_dictionary():
 out={}
 for line in (UP/'cmudict.dict').read_text().splitlines():
  if not line or line.startswith(';;;'):continue
  parts=line.split()
  word=re.sub(r'\(\d+\)$','',parts[0]).casefold()
  pron=tuple(re.sub(r'\d','',p) for p in parts[1:] if not p.startswith('#'))
  if not pron or any(p not in IDS for p in pron):continue
  prons=out.setdefault(word,[])
  if pron not in prons:prons.append(pron)
 return out
DICT=load_dictionary()
def words(text):
 result=[]
 for w in text.split():
  value=''.join(c for c in w.casefold().replace('’',"'") if c.isalpha() or c=="'")
  if not value:raise ValueError('empty lexical word')
  result.append(value)
 return result
def pronunciations(text):
 ws=words(text);allp=[]
 for w in ws:
  if w not in DICT:raise ValueError('dictionary OOV: '+w)
  allp.append(DICT[w])
 count=math.prod(map(len,allp))
 if count>POLICY['max_pronunciation_paths']:raise ValueError('pronunciation path budget exceeded')
 paths=[]
 for prons in itertools.product(*allp):
  ps=[IDS[p] for pron in prons for p in pron]
  if len(ps)>POLICY['max_phones']:raise ValueError('phone budget exceeded')
  paths.append((prons,ps))
 return ws,paths
def model(alpha):
 for record in json.loads((P/'download-pins.json').read_text()):
  assert pin(UP/record['name'])['sha256']==record['sha256']
 path=UP/f'model-{alpha}.ckpt'
 assert torch.serialization.get_unsafe_globals_in_checkpoint(path)==['pathlib.PosixPath']
 with torch.serialization.safe_globals([(PosixPath,'pathlib.PosixPath')]):
  checkpoint=torch.load(path,map_location='cpu',weights_only=True)
 spec=importlib.util.spec_from_file_location('pinned_model',UP/'models.py')
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 m=module.tdnn_blstm_ctc_model_base(len(LABELS))
 selected={k[len('model.'):]:v for k,v in checkpoint['state_dict'].items() if k.startswith('model.')}
 m.load_state_dict(selected,strict=True);m.eval()
 return m
def features(pcm):
 x=torch.tensor(np.asarray(pcm,dtype=np.float32)).unsqueeze(0)
 transform=torchaudio.transforms.MelSpectrogram(sample_rate=RATE,n_fft=400,n_mels=80,hop_length=160)
 # Match author's feature order and sequential mask mutation exactly.
 f=transform(x.squeeze(0)).transpose(1,0).unsqueeze(0)
 gain=pow(10,.05*(2*20*math.log10(torch.iinfo(torch.int16).max)))
 f=f*gain
 mask=f>math.e;f[mask]=torch.log(f[mask])
 mask=f<=math.e;f[mask]=f[mask]/math.e
 stats=json.loads((UP/'global_stats.json').read_text())
 f=(f-torch.tensor(stats['mean']))*torch.tensor(stats['invstddev'])
 return f,torch.tensor([f.shape[1]],dtype=torch.int64)
def emissions(m,pcm):
 if not 400<len(pcm)<=POLICY['max_audio_samples']:raise ValueError('audio bound')
 if not np.isfinite(pcm).all():raise ValueError('nonfinite audio')
 f,lengths=features(pcm)
 started=time.monotonic()
 with torch.inference_mode():logp,olen=m(f,lengths)
 result=logp[:int(olen[0].item()),0,:].numpy().astype(np.float64)
 assert result.ndim==2 and result.shape[1]==len(LABELS) and np.isfinite(result).all()
 assert result.shape[0]==math.ceil(f.shape[1]/2)
 return result,dict(feature_shape=list(f.shape),output_shape=list(result.shape),seconds=time.monotonic()-started,
  input_samples=len(pcm),feature_sha256=hashlib.sha256(f.numpy().tobytes()).hexdigest(),
  emission_sha256=hashlib.sha256(result.tobytes()).hexdigest())
def viterbi(logp,phones):
 """CTC blank/label states with stays, advances and legal repeat-aware skips."""
 if not phones:raise ValueError('no phones')
 T=len(logp);S=2*len(phones)+1
 state_labels=np.zeros(S,dtype=np.int64);state_labels[1::2]=phones
 prev=np.full(S,-np.inf);prev[0]=logp[0,0];prev[1]=logp[0,phones[0]]
 trace=np.zeros((T,S),dtype=np.uint8)
 skip=np.zeros(S,dtype=bool)
 for s in range(3,S,2):skip[s]=state_labels[s]!=state_labels[s-2]
 for t in range(1,T):
  stay=prev
  advance=np.r_[-np.inf,prev[:-1]]
  jump=np.r_[-np.inf,-np.inf,prev[:-2]]
  jump[~skip]=-np.inf
  candidates=np.stack([stay,advance,jump])
  choice=np.argmax(candidates,axis=0).astype(np.uint8)
  prev=candidates[choice,np.arange(S)]+logp[t,state_labels]
  trace[t]=choice
 last=S-1 if prev[S-1]>=prev[S-2] else S-2
 score=float(prev[last])
 if not math.isfinite(score):raise ValueError('CTC target cannot be aligned')
 path=np.empty(T,dtype=np.int64);path[-1]=last
 for t in range(T-1,0,-1):path[t-1]=path[t]-int(trace[t,path[t]])
 assert path[0] in [0,1] and path[-1] in [S-2,S-1]
 spans=[]
 for i,p in enumerate(phones):
  occupied=np.flatnonzero(path==2*i+1)
  if not len(occupied):raise ValueError('missing phone in path')
  a,b=int(occupied[0]),int(occupied[-1])+1
  assert np.all(path[a:b]==2*i+1)
  probs=np.exp(logp[a:b,p])
  spans.append(dict(phone=LABELS[p],start_frame=a,end_frame=b,frames=b-a,
                    mean_posterior=float(np.mean(probs)),minimum_posterior=float(np.min(probs))))
 return score,path,state_labels[path],spans
def propose(logp,text,n_samples):
 ws,choices=pronunciations(text);best=None
 for prons,ps in choices:
  score,path,states,spans=viterbi(logp,ps)
  if best is None or score>best[0]:best=(score,path,states,spans,prons)
 score,path,states,spans,prons=best
 out=[];offset=0
 for wi,(w,pron) in enumerate(zip(ws,prons)):
  selected=spans[offset:offset+len(pron)];offset+=len(pron)
  start,end=selected[0]['start_frame'],selected[-1]['end_frame']
  denom=sum(s['frames'] for s in selected)
  mean=sum(s['mean_posterior']*s['frames'] for s in selected)/denom
  minimum=min(s['mean_posterior'] for s in selected)
  reason=None
  if mean<POLICY['word_mean_posterior_min']:reason='word path posterior below threshold'
  elif minimum<POLICY['phone_mean_posterior_min']:reason='a phone path posterior below threshold'
  a,b=start*STEP,min(n_samples,end*STEP)
  if a>=b:reason='empty output interval'
  out.append(dict(index=wi,word=w,phones=selected,start_frame=start,end_frame=end,
   interval_samples=[a,b],mean_phone_frame_posterior=mean,minimum_phone_mean_posterior=minimum,
   status='proposed' if reason is None else 'refused',refusal=reason,
   confidence='uncalibrated CTC path posterior, not a correctness probability'))
 return dict(words=out,pronunciation_choices=len(choices),selected_pronunciations=[list(x) for x in prons],
   path_score=score,path_score_per_frame=score/len(logp),frame_state_path=path.tolist(),
   frame_label_path=states.tolist(),frame_blank_fraction=float(np.mean(states==0)),
   raw_candidate_intervals='always retained even when refused; not automatically accepted')
def checks():
 lp=np.log(np.array([[.1,.8,.1],[.1,.8,.1],[.9,.05,.05],[.1,.8,.1],[.8,.1,.1]]))
 score,path,_,spans=viterbi(lp,[1,1])
 assert spans[0]['end_frame']<=spans[1]['start_frame'] and any(path==2)
 assert len(spans)==2 and all(s['frames']>=1 for s in spans)
 lp2=np.log(np.array([[.8,.1,.1],[.1,.8,.1],[.1,.8,.1],[.1,.1,.8],[.1,.1,.8],[.8,.1,.1]]))
 _,path,_,spans=viterbi(lp2,[1,2])
 assert spans[0]['start_frame']==1 and spans[0]['end_frame']==3
 assert spans[1]['start_frame']==3 and spans[1]['end_frame']==5
 return dict(repeated_phone_requires_blank=True,stays_keep_duration=True,exclusive_offsets=True)
