import collections,json,statistics
from pathlib import Path
P=Path(__file__).resolve().parent;out=P/'development-first'
def stats(xs):
 if not xs:return dict(n=0)
 xs=sorted(xs)
 return dict(n=len(xs),min=xs[0],median=statistics.median(xs),mean=statistics.mean(xs),
 p90=xs[max(0,__import__('math').ceil(.9*len(xs))-1)],max=xs[-1])
summary={}
for alpha in ['0.0','0.6']:
 runs=[json.loads(f.read_text()) for f in sorted(out.glob(f'case??-{alpha}.json'))]
 rows=[m for r in runs for m in r['metrics']]
 offered=[m for m in rows if m['status']=='proposed']
 paired=[m for m in rows if 'whisper' in m]
 targets=[r['wrong_text']['proposal']['words'][len(r['wrong_text']['proposal']['words'])//2] for r in runs]
 result=dict(expected_words=len(rows),offered_words=len(offered),silence_offered_words=sum(w['status']=='proposed' for r in runs for w in r['silence']['words']),
 wrong_changed_word_proposals=sum(w['status']=='proposed' for w in targets),
 wrong_changed_word_diagnostics=[dict(case=r['case'],status=w['status'],mean=w['mean_phone_frame_posterior'],
 minimum=w['minimum_phone_mean_posterior'],reason=w['refusal']) for r,w in zip(runs,targets)],
 raw_metrics={},offered_metrics={},paired_whisper={},paired_candidate={},
 refusals=dict(collections.Counter(w['refusal'] for r in runs for w in r['proposal']['words'] if w['refusal'])))
 for name,values in [('raw_metrics',[m['candidate'] for m in rows]),('offered_metrics',[m['candidate'] for m in offered]),
                     ('paired_whisper',[m['whisper'] for m in paired]),('paired_candidate',[m['candidate'] for m in paired])]:
  for key in values[0] if values else []:
   vs=[m[key] for m in values]
   result[name][key]=dict(signed=stats(vs),absolute=stats([abs(v) for v in vs])) if 'error_ms' in key else stats(vs)
 summary[alpha]=result
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for alpha,s in summary.items():
 print('ALPHA',alpha,{k:s[k] for k in ['expected_words','offered_words','silence_offered_words','wrong_changed_word_proposals','refusals']})
 for mode in ['raw_metrics','offered_metrics','paired_whisper','paired_candidate']:
  print(mode)
  for key in ['onset_support_error_ms','offset_support_error_ms']:
   print(key,s[mode][key]['absolute'])
  if mode in ['raw_metrics','offered_metrics']:
   for key in ['retained_own_energy','retained_support_samples','foreign_word_energy_fraction']:
    print(key,s[mode][key])
