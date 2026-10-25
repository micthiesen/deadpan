import hashlib,json,math,shutil,sys
from pathlib import Path
import numpy as np
import torch,torchaudio
P=Path(__file__).resolve().parent;BASE=P.parent;ROOT=BASE.parent
sys.path.insert(0,str(BASE));import align
sys.path.insert(0,str(BASE/'candidate-v2'));import guard
assert align.pin(BASE/'align.py')['sha256']=='bcc450c5f628b59b22db7f903ccd5d82d4c9f730908f1f81c6b9c1dbdb57e787'
assert align.pin(BASE/'candidate-v2/guard.py')['sha256']=='6c10053ba98f2b700f191473ddbebb915cf3c793d60a51c1507cd9e2ecf0bdc7'
OUT=P/'goldens';OUT.mkdir(exist_ok=False)
def pin(p):return {'file':str(p.relative_to(P)), 'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
def binary(p,a,dtype):
 a=np.asarray(a,dtype=dtype,order='C');a.tofile(p)
 return dict(pin(p),shape=list(a.shape),dtype=dtype,layout='C row-major little-endian')
def save(p,x):p.write_text(json.dumps(x,indent=2)+'\n');return pin(p)
constants=P/'constants';constants.mkdir(exist_ok=False)
transform=torchaudio.transforms.MelSpectrogram(sample_rate=16000,n_fft=400,n_mels=80,hop_length=160)
stats=json.loads((align.UP/'global_stats.json').read_text())
c={}
c['hann']=binary(constants/'hann.f32le',transform.spectrogram.window.numpy(),'<f4')
c['mel_filter']=binary(constants/'mel-filter.f32le',transform.mel_scale.fb.numpy(),'<f4')
c['mean']=binary(constants/'mean.f32le',stats['mean'],'<f4')
c['inverse_stddev']=binary(constants/'inverse-stddev.f32le',stats['invstddev'],'<f4')
c['gain']=float(pow(10,.05*(2*20*math.log10(torch.iinfo(torch.int16).max))))
save(constants/'manifest.json',c)
records=[]
for cid in [f'case{i:02}' for i in range(1,11)]+[f'held{i:02}' for i in range(1,15)]:
 if cid.startswith('case'):
  sourcepath=ROOT/'corpus'/cid/'aligner-input.json'
  source=json.loads(sourcepath.read_text())
  old=json.loads((BASE/'development-first'/f'{cid}-0.6.json').read_text())
  oldproposal=old['proposal']
  lp=np.load(BASE/'development-first'/f'{cid}-0.6-emissions.npy')
  provenance='frozen PyTorch alpha0.6 development emission'
 else:
  old=json.loads((BASE/'fresh-heldout-first/raw'/f'{cid}.json').read_text())
  source=old['source'];oldproposal=old['proposal']
  lp=np.load(BASE/'fresh-heldout-first/raw'/f'{cid}-emissions.npy')
  provenance='frozen folded ONNX alpha0.6 observed-development emission'
 pcm=np.fromfile(source['pcm_f32le'],dtype='<f4');f,length=align.features(pcm)
 case=OUT/cid;case.mkdir()
 pcmfile=case/'audio.f32le';shutil.copyfile(source['pcm_f32le'],pcmfile)
 clean=dict(schema_version=1,sample_rate=16000,channels=1,pcm_f32le='audio.f32le',transcript=source['transcript'])
 inp=save(case/'input.json',clean)
 feats=binary(case/'features.f32le',f.numpy(),'<f4')
 emissions=binary(case/'emissions.f64le',lp,'<f8')
 proposal=align.propose(lp,source['transcript'],len(pcm))
 assert proposal==oldproposal,(cid,'proposal changed')
 inspection=guard.inspect(lp,source['transcript'],1)
 filtered=guard.filter_offers(proposal,inspection)
 raw=save(case/'raw-suggestion.json',proposal)
 gf=save(case/'guard-output.json',inspection)
 final=save(case/'filtered-suggestion.json',filtered)
 extra={}
 if cid=='case01':
  spec=transform.spectrogram(torch.tensor(pcm))
  mel=transform.mel_scale(spec).transpose(1,0)
  extra['power_spectrum']=binary(case/'power.f32le',spec.numpy(),'<f4')
  extra['mel_before_gain']=binary(case/'mel.f32le',mel.numpy(),'<f4')
 records.append(dict(case=cid,input=inp,pcm=pin(pcmfile),features=feats,feature_length=int(length[0]),emissions=emissions,
  emission_origin=provenance,raw=raw,guard=gf,filtered=final,extra=extra,
  base_offers=sum(w['status']=='proposed' for w in proposal['words']),
  filtered_offers=sum(w['status']=='proposed' for w in filtered['words'])))
 print(json.dumps(dict(case=cid,base=records[-1]['base_offers'],filtered=records[-1]['filtered_offers'])),flush=True)
# Synthetic deterministic algorithm vectors are not acoustic evidence.
algorithm=P/'algorithm';algorithm.mkdir()
vectors=[]
for name,target,lp in [
 ('repeat-phone',[1,1],np.log(np.array([[.1,.8,.1],[.1,.8,.1],[.9,.05,.05],[.1,.8,.1],[.8,.1,.1]]))),
 ('duration-stays',[1,2],np.log(np.array([[.8,.1,.1],[.1,.8,.1],[.1,.8,.1],[.1,.1,.8],[.1,.1,.8],[.8,.1,.1]]))),
 ('tie-stay-first',[1,2],np.full((7,3),-math.log(3))),
 ('tie-repeat',[1,1],np.full((7,3),-math.log(3)))]:
 score,state,labels,spans=align.viterbi(lp,target)
 v={'name':name,'target_phone_ids':target,'emissions':binary(algorithm/(name+'.f64le'),lp,'<f8'),
    'score':score,'state_path':state.tolist(),'label_path':labels.tolist(),'phone_spans':spans}
 vectors.append(v)
save(algorithm/'viterbi.json',vectors)
normal=[]
for text in ["Copper birds cross quiet roofs","can't","I’m","co-op","...",'123','Straße',' '.join(['the']*8)]:
 row={'input':text}
 try:
  ws,paths=align.pronunciations(text);row.update(words=ws,alternatives=[{'pronunciations':[list(x) for x in prons],'phone_ids':ids} for prons,ids in paths])
 except ValueError as e:row['error']=str(e)
 normal.append(row)
save(algorithm/'normalization.json',normal)
result=dict(kind='oracle-free deterministic backend parity goldens; not a new quality evaluation',
 source_policy=align.pin(BASE/'align.py'),guard_policy=align.pin(BASE/'candidate-v2/guard.py'),
 labels=align.LABELS,constants=c,records=records,
 versions={'torch':torch.__version__,'torchaudio':torchaudio.__version__,'numpy':np.__version__},
 no_truth_files_read=True,no_model_inference_rerun=True)
save(P/'manifest.json',result)
print(json.dumps({'cases':len(records),'manifest':pin(P/'manifest.json')}),flush=True)
