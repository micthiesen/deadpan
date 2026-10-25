"""Evaluate saved fresh outputs only; never invokes alignment or updates policy."""
import ast,collections,hashlib,json,math,statistics
from pathlib import Path
import numpy as np
P=Path(__file__).resolve().parent;ROOT=P.parent;DATA=ROOT/'fresh-corpus';OUT=P/'fresh-heldout-first'
pin=lambda p:dict(path=str(p),bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest())
done=json.loads((OUT/'raw-complete.json').read_text())
assert done['raw_outputs_complete'] and not done['truth_accessed']
assert pin(P/'align.py')['sha256']=='bcc450c5f628b59b22db7f903ccd5d82d4c9f730908f1f81c6b9c1dbdb57e787'
manifest=json.loads((DATA/'manifest.json').read_text())
def stat(xs):
 xs=sorted(xs)
 if not xs:return dict(n=0)
 return dict(n=len(xs),minimum=xs[0],median=statistics.median(xs),mean=statistics.mean(xs),
 p90=xs[math.ceil(.9*len(xs))-1],p95=xs[math.ceil(.95*len(xs))-1],maximum=xs[-1])
# Reuse the already-reviewed exact-sample scorer as a pure function, not its experiment driver.
nodes=ast.parse((P/'develop.py').read_text()).body
pure=[n for n in nodes if isinstance(n,ast.FunctionDef) and n.name=='metric']
ns=dict(np=np);exec(compile(ast.Module(body=pure,type_ignores=[]),'existing_metric_function','exec'),ns)
metric=ns['metric']
results=[];positive=[];natural=[];negative=[]
for info in manifest['cases']:
 oracle=Path(info['oracle']['path'])
 assert pin(oracle)==info['oracle']
 truth=json.loads(oracle.read_text());cid=info['id']
 record=json.loads((OUT/'raw'/(cid+'.json')).read_text())
 row=dict(case=cid,kind=truth['kind'],source=record['source'],oracle=pin(oracle),raw=pin(OUT/'raw'/(cid+'.json')),
 expected_words=len(record['source']['transcript'].split()),
 offers=[dict(index=w['index'],word=w['word'],status=w['status'],interval_samples=w['interval_samples'],
 mean_posterior=w['mean_phone_frame_posterior'],minimum_phone_mean_posterior=w['minimum_phone_mean_posterior'],refusal=w['refusal']) for w in record['proposal']['words']],
 seconds=record['seconds'])
 pcm=np.fromfile(record['source']['pcm_f32le'],dtype='<f4')
 if truth['kind']=='isolated_known_construction':
  assert len(truth['word_oracle'])==len(record['proposal']['words'])
  row['word_metrics']=[]
  for pr,w in zip(record['proposal']['words'],truth['word_oracle']):
   assert pr['word']==w['word'].casefold()
   v=metric(pcm,pr['interval_samples'],w,truth['word_oracle'])
   a,b=pr['interval_samples'];ca,cb=w['construction_interval_samples'];u,z=max(a,ca),min(b,cb)
   kept=pcm[u:z] if z>u else []
   own=pcm[ca:cb]
   v['retained_samples_above_oracle_threshold_0_002']=float(np.count_nonzero(np.abs(kept)>=.002))/int(np.count_nonzero(np.abs(own)>=.002))
   item=dict(case=cid,index=w['index'],word=w['word'],status=pr['status'],metrics=v,
     interval_samples=pr['interval_samples'],construction=w['construction_interval_samples'],
     support=w['energy_support_interval_samples'],mean_posterior=pr['mean_phone_frame_posterior'],
     minimum_phone_mean_posterior=pr['minimum_phone_mean_posterior'])
   row['word_metrics'].append(item);positive.append(item)
 elif truth['kind']=='natural_sentence_utterance_only':
  ws=record['proposal']['words'];a,b=truth['utterance_energy_support']
  value=dict(case=cid,word_count=len(ws),offered_words=sum(w['status']=='proposed' for w in ws),
   raw_first_onset_error_ms=(ws[0]['interval_samples'][0]-a)/16,
   raw_final_offset_error_ms=(ws[-1]['interval_samples'][1]-b)/16,
   first_status=ws[0]['status'],last_status=ws[-1]['status'],
   raw_first_interval=ws[0]['interval_samples'],raw_final_interval=ws[-1]['interval_samples'],
   truth_utterance_support=[a,b],interior_word_timing='UNKNOWN, no oracle',
   own_utterance_energy_retained=float(np.dot(pcm[ws[0]['interval_samples'][0]:ws[-1]['interval_samples'][1]],pcm[ws[0]['interval_samples'][0]:ws[-1]['interval_samples'][1]]))/float(np.dot(pcm[a:b],pcm[a:b])))
  row['utterance_metrics']=value;natural.append(value)
 else:
  row['negative_expectation']=truth['expected_behavior']
  row['negative_offered_words']=sum(w['status']=='proposed' for w in record['proposal']['words'])
  row['negative_reference_ambiguity']=truth.get('reference_uncertainty')
  negative.append(row)
 results.append(row)
def summarize(rows):
 out={}
 for key in rows[0]['metrics'] if rows else []:
  vals=[r['metrics'][key] for r in rows]
  if 'error_ms' in key:
   out[key]=dict(signed=stat(vals),absolute=stat([abs(v) for v in vals]),
    within20ms=sum(abs(v)<=20 for v in vals)/len(vals),within50ms=sum(abs(v)<=50 for v in vals)/len(vals),
    within100ms=sum(abs(v)<=100 for v in vals)/len(vals))
  else:out[key]=stat(vals)
 return out
offered=[r for r in positive if r['status']=='proposed']
bad=[r for r in offered if abs(r['metrics']['onset_support_error_ms'])>100 or abs(r['metrics']['offset_support_error_ms'])>100 or r['metrics']['retained_own_energy']<.95 or r['metrics']['foreign_word_energy_fraction']>.01]
summary=dict(policy=done['policy'],model=done['onnx'],raw_complete=pin(OUT/'raw-complete.json'),manifest=pin(DATA/'manifest.json'),
 inference_run_count=1,policy_unchanged=True,no_threshold_tuning=True,
 isolated=dict(cases=8,words=len(positive),offered_words=len(offered),word_coverage=len(offered)/len(positive),
  all_raw_candidates=summarize(positive),offered=summarize(offered),offered_failures_at_descriptive_100ms_95percent_1percent_foreign=bad),
 natural=natural,negatives=negative,
 latency_total_short_window_seconds=stat([r['seconds']['total'] for r in results]),
 limitations=['TTS construction and threshold support are not human phonetic truth.',
 'Natural sentence cases have only first/last utterance support; interior word accuracy is unknown.',
 'Posterior scores are uncalibrated, especially when forced transcript omits repeated speech.',
 'Safety failure thresholds in descriptive failure list are reporting lenses, not changed policy gates.',
 'No native app integration, real conversational corpus, worker cancellation, or production PCM qualification.'])
(OUT/'evaluation.json').write_text(json.dumps(dict(summary=summary,cases=results),indent=2)+'\n')
print(json.dumps(summary,indent=2),flush=True)
