import importlib.util,json,sys,time,hashlib
from pathlib import Path
import numpy as np
import align
P=Path(__file__).resolve().parent;ROOT=P.parent
def load_module(path,name):
 spec=importlib.util.spec_from_file_location(name,path)
 m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
old=load_module(ROOT/'policy.py','old_policy')
def metric(pcm,interval,word,allwords):
 # Old helper expects centiseconds. This exact-sample scorer does not round model edges.
 a,b=interval;ca,cb=word['construction_interval_samples'];sa,sb=word['energy_support_interval_samples']
 own=pcm[ca:cb];u,v=max(ca,a),min(cb,b);kept=pcm[u:v] if v>u else []
 power=float(np.dot(own,own));energy=float(np.dot(pcm[a:b],pcm[a:b]))
 foreign=0.
 for w in allwords:
  if w['index']==word['index']:continue
  x,y=w['construction_interval_samples'];u,v=max(a,x),min(b,y)
  if v>u:foreign+=float(np.dot(pcm[u:v],pcm[u:v]))
 return dict(onset_support_error_ms=(a-sa)/16,offset_support_error_ms=(b-sb)/16,
  onset_construction_error_ms=(a-ca)/16,offset_construction_error_ms=(b-cb)/16,
  retained_own_energy=float(np.dot(kept,kept))/power if power else None,
  retained_support_samples=float(np.count_nonzero(np.abs(kept)>=1e-4))/int(np.count_nonzero(np.abs(own)>=1e-4)) if np.count_nonzero(np.abs(own)>=1e-4) else None,
  foreign_word_energy_fraction=foreign/energy if energy else 0,
  below_threshold_ms=float(np.count_nonzero(np.abs(pcm[a:b])<1e-4))/16,
  outside_construction_ms=((b-a)-max(0,min(b,cb)-max(a,ca)))/16)
out=P/'development-first';out.mkdir(exist_ok=False)
results=[]
for alpha in ['0.0','0.6']:
 m=align.model(alpha)
 for caseid in range(1,11):
  case=ROOT/'corpus'/f'case{caseid:02}'
  source=json.loads((case/'aligner-input.json').read_text())
  pcm=np.fromfile(source['pcm_f32le'],dtype='<f4')
  logp,timing=align.emissions(m,pcm)
  np.save(out/f'case{caseid:02}-{alpha}-emissions.npy',logp)
  proposal=align.propose(logp,source['transcript'],len(pcm))
  # Negative text and silence are diagnostics, never used to train/tune a passing score.
  altered=source['transcript'].split();altered[len(altered)//2]='banana'
  wrong=align.propose(logp,' '.join(altered),len(pcm))
  silence_lp,silence_time=align.emissions(m,np.zeros_like(pcm))
  silence=align.propose(silence_lp,source['transcript'],len(pcm))
  response=json.loads((ROOT/'comparison-first'/f'case{caseid:02}-ordinary.stdout').read_text())
  oldwords=old.production_words(response)
  oldmatch=[old.norm(w['text']) for w in oldwords]==[old.norm(w) for w in source['transcript'].split()]
  # Oracle accessed only after acoustic inference and all proposals above.
  oracle=json.loads((case/'oracle.json').read_text())
  rows=[]
  for word,pr in zip(oracle['word_oracle'],proposal['words']):
   row=dict(index=word['index'],word=word['word'],status=pr['status'],
    candidate=metric(pcm,pr['interval_samples'],word,oracle['word_oracle']))
   if oldmatch:
    ow=oldwords[word['index']]
    row['whisper']=metric(pcm,[ow['start_cs']*160,ow['end_cs']*160],word,oracle['word_oracle'])
   rows.append(row)
  result=dict(case=case.name,alpha=alpha,input=align.pin(case/'aligner-input.json'),
    pcm=align.pin(Path(source['pcm_f32le'])),oracle=align.pin(case/'oracle.json'),timing=timing,
    proposal=proposal,wrong_text=dict(text=' '.join(altered),proposal=wrong),silence=silence,
    old_lexical_match=oldmatch,metrics=rows)
  (out/f'{case.name}-{alpha}.json').write_text(json.dumps(result,indent=2)+'\n')
  results.append(result)
  print(json.dumps(dict(case=case.name,alpha=alpha,offered=sum(w['status']=='proposed' for w in proposal['words']),
    words=len(proposal['words']),wrong_offered=sum(w['status']=='proposed' for w in wrong['words']),
    silence_offered=sum(w['status']=='proposed' for w in silence['words']),seconds=timing['seconds'])),flush=True)
summary=dict(policy=align.POLICY,code=align.pin(P/'align.py'),experiment=align.pin(Path(__file__)),
 checks=align.checks(),scope='cases01-10 DEVELOPMENT ONLY; prior heldout consumed; no new heldout read',
 models=json.loads((P/'inspection.json').read_text()),runs=[dict(case=r['case'],alpha=r['alpha'],words=len(r['metrics']),
  offered=sum(w['status']=='proposed' for w in r['proposal']['words'])) for r in results])
(out/'manifest.json').write_text(json.dumps(summary,indent=2)+'\n')
