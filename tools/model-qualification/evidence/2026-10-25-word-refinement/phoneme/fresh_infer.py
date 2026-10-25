import json,time
from pathlib import Path
import numpy as np,onnxruntime as ort
import align
P=Path(__file__).resolve().parent;DATA=P.parent/'fresh-corpus'
EXPECTED_POLICY='bcc450c5f628b59b22db7f903ccd5d82d4c9f730908f1f81c6b9c1dbdb57e787'
EXPECTED_MODEL='640bdf5da6928ce096c8d4d9216fb0613dfc7310b42d9182bd64214fa4719cbb'
assert align.pin(P/'align.py')['sha256']==EXPECTED_POLICY
assert align.pin(P/'export-first/phoneme-alpha-0.6.onnx')['sha256']==EXPECTED_MODEL
assert align.pin(DATA/'inputs.json')['sha256']=='e7e7e3052fbdf52983a405aa2972765aeb00d7c2d598e80ea575ec6fa4d88c76'
assert align.pin(DATA/'freeze.json')['sha256']=='daa1596b4cea45b35074b1eb14026f6e45b84de2aaa222dd7fdc9d3f530d6efd'
inputs=json.loads((DATA/'inputs.json').read_text())
out=P/'fresh-heldout-first';out.mkdir(exist_ok=False);raw=out/'raw';raw.mkdir()
started=time.monotonic()
session=ort.InferenceSession(str(P/'export-first/phoneme-alpha-0.6.onnx'),providers=['CPUExecutionProvider'])
loaded=time.monotonic()-started
records=[]
for case in inputs['cases']:
 for key in ['input','pcm']:
  artifact=case[key]
  assert align.pin(Path(artifact['path']))==artifact
 source=json.loads(Path(case['input']['path']).read_text())
 assert source['sample_rate']==16000 and source['channels']==1
 pcm=np.fromfile(source['pcm_f32le'],dtype='<f4')
 start=time.monotonic()
 feature,length=align.features(pcm)
 prepared=time.monotonic()
 logp,olen=session.run(None,dict(features=feature.numpy(),feature_lengths=length.numpy()))
 inferred=time.monotonic()
 logp=logp[:int(olen[0]),0,:].astype(np.float64)
 np.save(raw/(case['id']+'-emissions.npy'),logp)
 try:
  proposal=align.propose(logp,source['transcript'],len(pcm))
  status='completed'
 except ValueError as error:
  proposal=dict(words=[],refusal=str(error));status='refused'
 ended=time.monotonic()
 record=dict(case=case['id'],source=source,inputs={k:case[k] for k in ['input','pcm']},
  feature_shape=list(feature.shape),output_shape=list(logp.shape),output_length=float(olen[0]),
  proposal=proposal,status=status,seconds=dict(features=prepared-start,onnx=inferred-prepared,alignment=ended-inferred,total=ended-start))
 (raw/(case['id']+'.json')).write_text(json.dumps(record,indent=2)+'\n')
 records.append(dict(case=case['id'],output=align.pin(raw/(case['id']+'.json')),
  emissions=align.pin(raw/(case['id']+'-emissions.npy'))))
 print(json.dumps(dict(case=case['id'],status=status,words=len(proposal['words']),offered=sum(w['status']=='proposed' for w in proposal['words']),seconds=record['seconds'])),flush=True)
manifest=dict(policy=align.pin(P/'align.py'),onnx=align.pin(P/'export-first/phoneme-alpha-0.6.onnx'),
 inputs=align.pin(DATA/'inputs.json'),dataset_freeze=align.pin(DATA/'freeze.json'),
 script=align.pin(Path(__file__)),policy_configuration=align.POLICY,
 records=records,raw_outputs_complete=True,truth_accessed=False,session_load_seconds=loaded,
 inference_run_count=1,provider=session.get_providers())
(out/'raw-complete.json').write_text(json.dumps(manifest,indent=2)+'\n')
assert align.pin(P/'align.py')['sha256']==EXPECTED_POLICY
print('RAW_COMPLETE_NO_TRUTH_ACCESS',flush=True)
