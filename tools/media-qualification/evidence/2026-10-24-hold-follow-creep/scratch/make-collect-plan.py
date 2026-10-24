#!/usr/bin/env python3
"""Build the bounded task evidence manifest after all qualification is terminal."""
import hashlib,json,shutil
from pathlib import Path
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
prior=Path('/tmp/deadpan-point-targets-2026-10-24')
for name,sha in [('collect.py','320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e'),('bundle-helpers.py','2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13')]:
 data=(prior/name).read_bytes()
 assert hashlib.sha256(data).hexdigest()==sha
 if (s/name).exists():
  assert (s/name).read_bytes()==data
 else:
  (s/name).write_bytes(data)
def pin(path):
 return {'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
inputs=['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md','docs/TARGETS.md','docs/FRAMING.md','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']
excluded={'collect-plan.json','collect.stdout','collect.stderr','package-verification.json','package-verification.stdout','package-verification.stderr','verify-hold-package.py','publish-verification.py'}
files=[{'source':str(p),'path':'scratch/'+p.name} for p in sorted(s.iterdir()) if p.is_file() and p.name not in excluded]
for d in sorted(s.glob('native-*')):
 if d.is_dir():
  for name in ['report.json','report.html']:
   p=d/name
   if p.exists():files.append({'source':str(p),'path':d.name+'/'+name})
visual=json.loads((s/'visual-review.json').read_text())
for name in visual['images']:
 files.append({'source':str(s/name),'path':'images/'+name.replace('/','-'),'png':True})
plan={'schema_version':1,'repository':str(repo),'destination':'tools/media-qualification/evidence/2026-10-24-hold-follow-creep','raw_byte_limit':256*1024*1024,'gzip_threshold_bytes':65536,'inputs':{n:pin(repo/n) for n in inputs},'post_run_sources':{},'files':files,'archives':[{'name':'first-source','root':str(s/'source-first')},{'name':'audit-history','root':str(s/'audit-history')},{'name':'final-hold-project-and-export','root':str(s/'media-final')},{'name':'actual-native-hold-project','root':str(s/'native-hold-final/projects/hold-follow-creep/Documents/Deadpan/cfr-bframes.deadpan')}],'limitations':[
 'Twenty scoped final source paths and five sibling executable identities, not an attestation of every repository file.',
 'Earlier Core/Plan16, renderer17 and App/CLI19-path receipts retain their actual scopes. Only relevant unchanged paths are applied to final20; final-second checks and native/media receipts bind20.',
 'The first passing Hold replay had overlapping target labels. Final overlay/paging fixes and a painted-shape regression are exercised by the final build and replays.',
 'Positive face regression observations use the existing scripted seam; the Hold scenario uses manually authored points and real no-face detection. No person-quality or tracking-accuracy claim.',
 'Offscreen Metal and scripted native input; no physical display/input, OS IME delivery, VoiceOver speech, listening or latency qualification.',
 'Export comparison uses the committed shared picture/DSP reference. Neighbor observability limits, all warnings, failures and opt-in ignored tests are retained.',
 'Actual native and portable projects, authoritative immutable history, complete media verification and explicit encoder-delay metadata are retained.',
 'Collector output and later independent package-verification receipts are retained separately to avoid recursive publication.'
 ]}
raw=sum(Path(x['source']).stat().st_size for x in files)+sum((repo/n).stat().st_size for n in inputs)
assert raw<220*1024*1024,raw
with (s/'collect-plan.json').open('x') as f:
 json.dump(plan,f,indent=2);f.write('\n')
print(json.dumps({'files':len(files),'listed_raw_bytes':raw,'collector_sha256':pin(s/'collect.py')['sha256'],'plan_sha256':pin(s/'collect-plan.json')['sha256']}))
