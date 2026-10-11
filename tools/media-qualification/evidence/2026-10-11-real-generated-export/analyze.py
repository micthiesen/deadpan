from pathlib import Path
import json
root=Path(__file__).parent
results={}
steps=json.loads((root/'chirp-v2-steps.json').read_text())
for name,rate in [('chirp-v2-4k',30),('chirp-v2-1080p',60)]:
 report=json.loads((root/(name+'-verification.json')).read_text())
 total=rate*5//2;hold=rate//2
 assert report['passed'] and not report['failures']
 assert len(report['pictures'])==total
 for p in report['pictures']:
  n=p['project_frame'];generated=rate<=n<rate+hold
  expected=n-rate if generated else n if n<rate else n-hold
  assert p['output_frame']==n and p['provenance']['source_frame']==expected,(name,p)
  assert p['provenance']['kind']==('generated' if generated else 'original'),(name,p)
 assert report['movie']['video_frames']==total==report['movie']['expected_video_frames']
 assert report['movie']['expected_audio_samples']==120000
 summary=report['summary']
 assert summary['offset_verified_windows']==3 and summary['nonzero_offsets']==0 and summary['offset_unobservable_windows']==0
 peaks={}
 for label in ['early-chirp','silent-hold','shifted-chirp']:
  audio=json.loads((root/(name+'-'+label+'.stdout')).read_text())['audio']
  assert len(audio['samples'])==256
  peak=max(abs(channel) for frame in audio['samples'] for channel in frame)
  assert (peak==0 if label=='silent-hold' else peak>0.5),(name,label,peak)
  peaks[label]=peak
 assert json.loads((root/(name+'-accepted.stdout')).read_text())==json.loads((root/(name+'-after.stdout')).read_text())
 results[name]=dict(frames=total,generated_frames=hold,audio_samples=120000,summary=summary,peaks=peaks,seconds={stage:steps[name+'-'+stage]['seconds'] for stage in ['generate','render','verify']},video_edits=report['movie']['video_edits'],audio_edits=report['movie']['audio_edits'])
negative=json.loads((root/'negative-fallback-report.json').read_text())
assert not negative['passed'] and negative['summary']['pictures_failed']>0
assert negative['summary']['audio_windows_checked']==0
results['fallback_negative']=dict(summary=negative['summary'],failures=negative['failures'])
(root/'results.json').write_text(json.dumps(results,indent=2)+'\n')
for name,v in results.items():print(name,json.dumps(v))
