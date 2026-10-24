#!/usr/bin/env python3
"""Freeze one bounded manifest after Sequence qualification and audit finish."""
import hashlib,json
from pathlib import Path
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
prior=Path('/tmp/deadpan-hold-follow-creep-2026-10-24')
for name,sha in [('collect.py','320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e'),('bundle-helpers.py','2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13')]:
 data=(prior/name).read_bytes()
 assert hashlib.sha256(data).hexdigest()==sha
 if (s/name).exists():assert (s/name).read_bytes()==data
 else:(s/name).write_bytes(data)
def pin(path):
 return {'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
inputs=['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md','docs/TARGETS.md','docs/FRAMING.md','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']
excluded={'collect-plan.json','collect.stdout','collect.stderr','package-verification.json','package-verification.stdout','package-verification.stderr','verify-sequence-package.py','publish-verification.py','next-repeat-map.md'}
files=[{'source':str(p),'path':'scratch/'+p.name} for p in sorted(s.iterdir()) if p.is_file() and p.name not in excluded]
for d in sorted(s.glob('native-*')):
 if d.is_dir():
  for name in ['report.json','report.html']:
   p=d/name
   if p.exists():files.append({'source':str(p),'path':d.name+'/'+name})
for name in json.loads((s/'visual-review.json').read_text())['images']:
 files.append({'source':str(s/name),'path':'images/'+name.replace('/','-'),'png':True})
plan={'schema_version':1,'repository':str(repo),'destination':'tools/media-qualification/evidence/2026-10-24-sequence-follow-creep','raw_byte_limit':256*1024*1024,'gzip_threshold_bytes':65536,'inputs':{n:pin(repo/n) for n in inputs},'post_run_sources':{},'files':files,'archives':[
 {'name':'first-source','root':str(s/'source-first')},
 {'name':'worker-final-source','root':str(s/'worker-source-final')},
 {'name':'audit-history','root':str(s/'audit-history')},
 {'name':'final-sequence-project-and-export','root':str(s/'media-final')},
 {'name':'actual-native-sequence-project','root':str(s/'native-sequence-first/projects/sequence-follow-creep/Documents/Deadpan/cfr-bframes.deadpan')}
],'limitations':[
 'Fourteen scoped final source paths and five sibling executable identities, not an attestation of every repository file.',
 'One release candidate and one Sequence workflow. First/final source and all five binary bytes are identical; the passing native-sequence-first is final and is not counted twice.',
 'Focused workers bind their actual narrower owned sources and basis. Core focused1 predates a later CLI fixture correction outside its Core test scope; Plan focused1 basis includes a later Core semantic-test change, with no Plan dependency production change. Final broad Core/Plan2049 and all standard final receipts bind all14 current sources.',
 'The initial CLI reverse-selection fixture cursor mismatch and its exact old test source are retained. Final focused CLI1 and broader13 framing cases pass; these are not the full CLI suite.',
 'Native Sequence uses production keys and manually authored/corrected points, without a scripted detector/tracker. Affected Source/Hold regressions are retained; unrelated face/point/tracking scenarios were not rerun.',
 'Offscreen Metal and scripted native input; no physical display/input, OS IME delivery, VoiceOver speech, listening or latency qualification.',
 'Export comparison uses the committed shared picture/DSP reference. Neighbor observability limits, warnings, failures and existing opt-in ignores are retained.',
 'Actual native and portable projects, immutable history, complete media verification and explicit encoder-delay metadata are retained.',
 'Collector output and later package-verification receipts remain separate to avoid recursive publication.'
]}
raw=sum(Path(x['source']).stat().st_size for x in files)+sum((repo/n).stat().st_size for n in inputs)
assert raw<220*1024*1024,raw
with (s/'collect-plan.json').open('x') as f:json.dump(plan,f,indent=2);f.write('\n')
print(json.dumps({'files':len(files),'listed_raw_bytes':raw,'collector_sha256':pin(s/'collect.py')['sha256'],'plan_sha256':pin(s/'collect-plan.json')['sha256']}))
