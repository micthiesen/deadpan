#!/usr/bin/env python3
"""Prepare two bounded immutable evidence collections after terminal raw audit."""
from pathlib import Path
import hashlib,json,sys
s=Path(__file__).resolve().parent
repo=Path('/Users/michael/Code/deadpan')
prior=Path('/tmp/deadpan-retime-follow-creep-2026-10-24')
def pin(p):
 return {'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
for name,digest in [('collect.py','320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e'),('bundle-helpers.py','2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13')]:
 data=(prior/name).read_bytes();assert hashlib.sha256(data).hexdigest()==digest
 if (s/name).exists():assert (s/name).read_bytes()==data
 else:(s/name).write_bytes(data)
inputs=['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml','crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md','docs/TARGETS.md','docs/FRAMING.md','docs/SEMANTIC_MACROS.md','native/deadpan-source/tests/fixtures/cfr-bframes.mp4']
excluded={'collect-plan.json','regressions-collect-plan.json','collect.stdout','collect.stderr','regressions-collect.stdout','regressions-collect.stderr','package-verification.json','package-verification.stdout','package-verification.stderr','verify-scoped-package.py','publish-verification.py'}
files=[{'source':str(p),'path':'scratch/'+p.name+('.txt' if p.suffix=='.rs' else '')} for p in sorted(s.iterdir()) if p.is_file() and p.name not in excluded and not p.name.startswith('next-')]
primary=['native-scoped-first','native-repeat-follow-creep-final','native-retime-follow-creep-final']
regressions=['native-zoom-final','native-sequence-follow-creep-final','native-hold-follow-creep-final']
def reports(names):
 return [{'source':str(s/name/report),'path':name+'/'+report} for name in names for report in ['report.json','report.html']]
files+=reports(primary)
for row in json.loads((s/'visual-review.json').read_text())['images']:
 p=Path(row['path'])
 files.append({'source':str(p),'path':'native-scoped-first/'+p.name,'png':True})
archives=[]
for d in sorted(s.iterdir()):
 if d.is_dir() and (d.name.endswith('-source') or d.name in ['source-first','native-source','audit-history'] or d.name.startswith('media-')):
  archives.append({'name':d.name,'root':str(d)})
archives.append({'name':'actual-native-scoped-project','root':str(s/'native-scoped-first/projects/scoped-follow-creep/Documents/Deadpan/cfr-bframes.deadpan')})
limits=[
'Twenty-two listed final source paths and five sibling executable identities; not an attestation of every repository file.',
'One release candidate, qualified by first/final source and binary byte equality; native-scoped-first is final if that equality passes.',
'All six native report JSON and HTML pairs are retained across this Primary and its separate regressions package to keep each complete collection below256MiB. Reports are neither truncated nor omitted.',
'Four Root-inspected named PNG checkpoints are retained beside their scoped report. Other intermediate screenshots are not included; raw reports retain their original references.',
'Focused Core3 and scoped59 overlap one case, giving61unique. CLI16 is a framing integration filter, not the complete CLI suite. Plan2 cover200picture samples over four bounded typed target cases.',
'Two initial CLI oracle failures, first Clippy101 and both stopped media drivers are retained with exact prior source. They are not first-pass successes. No product change followed scoped admission except test-only lint syntax.',
'The first media driver stopped before CLI on standard declared harness exclusions. The second stopped after one read-only dump on an inherited fallback expectation. The centered entry fallback and live target center are intentionally distinct.',
'Offscreen Metal and scripted input do not prove physical display/input, OS IME, VoiceOver speech, listening or native OS picker delivery. Exact standard exclusions, warnings and eleven existing ignored App tests remain declared.',
'App unit binary identity was observed during execution and again at terminal completion, not archived or attested continuously.',
'Native all-frame independent pose/time observations and committed shared-reference emitted-picture/full-PCM verification are separate evidence. No independent renderer or acoustic listening is claimed.',
'Actual native and portable projects retain complete immutable history and exact encoder-delay metadata.',
'Collector outputs and later package-verification receipts remain separate to avoid recursive publication.'
]
base={'schema_version':1,'repository':str(repo),'raw_byte_limit':256*1024*1024,'gzip_threshold_bytes':65536,'inputs':{name:pin(repo/name) for name in inputs},'post_run_sources':{},'limitations':limits}
plans=[('collect-plan.json',dict(base,destination='tools/media-qualification/evidence/2026-10-24-scoped-follow-creep',files=files,archives=archives)),('regressions-collect-plan.json',dict(base,destination='tools/media-qualification/evidence/2026-10-24-scoped-follow-creep-regressions',files=reports(regressions)+[{'source':str(s/(name+ext)),'path':'scratch/'+name+ext} for name in regressions for ext in ['.json','.stdout','.stderr']]+[{'source':str(s/name),'path':'scratch/'+name} for name in ['first-identities.json','final-identities.json','candidate-equivalence.json']],archives=[]))]
selected = sys.argv[1]
assert selected in ['primary', 'regressions']
for name,plan in plans:
 if (name == 'collect-plan.json') != (selected == 'primary'):
  continue
 p=s/name
 with p.open('x') as f:json.dump(plan,f,indent=2);f.write('\n')
 listed=sum(Path(row['source']).stat().st_size for row in plan['files'])
 archive=sum(p.stat().st_size for row in plan['archives'] for p in Path(row['root']).rglob('*') if p.is_file())
 print(json.dumps({'plan':name,'sha256':pin(p)['sha256'],'listed_raw_bytes':listed,'archive_raw_bytes':archive,'files':len(plan['files']),'archives':len(plan['archives'])}))
