from pathlib import Path
import gzip
import hashlib
import json
import re
import subprocess
import sys

repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-transitions'
def sha(data): return hashlib.sha256(data).hexdigest()
manifest=json.loads((root/'manifest.json').read_text())
assert len({r['path'] for r in manifest})==len(manifest)
raw={}
for row in manifest:
    data=(root/row['path']).read_bytes()
    assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
    plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
    assert sha(plain)==row['original_sha256'],row['path']
    raw[row['path'].removesuffix('.gz')]=plain
def read(name): return json.loads(raw[name])
identity=read('identity.json')
for name,expected in identity['source_sha256'].items():
    assert sha(raw['source/'+name])==expected,name
    assert subprocess.check_output(['git','show',identity['base']+':'+name],cwd=repo)==raw['base/'+name],name
    if '--current' in sys.argv: assert sha((repo/name).read_bytes())==expected,name
assert sha(Path(identity['baseline_app']).read_bytes())==identity['baseline_app_sha256']
for name,expected in read('binary-pins.json').items(): assert sha(Path(name).read_bytes())==expected,name
pins=read('verification-pins.json')
assert sha(Path(pins['test_executable']).read_bytes())==pins['test_executable_sha256']
for name,expected in pins['libraries_sha256'].items(): assert sha(Path(name).read_bytes())==expected,name
for name,pin in read('fixture-pins.json').items():
    data=(Path(identity['fixture'])/name).read_bytes()
    assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],name
for name,step in read('fixture-steps.json').items():
    assert step['exit']==0,name
    for suffix in ['stdout','stderr']: assert sha(raw[name+'.'+suffix])==step[suffix+'_sha256'],name
assert read('generate.stdout')['state']=='Ready'
before=read('before.stdout');accepted=read('accepted.stdout')
for node,value in before['nodes'].items():
    if value['kind']['type']=='hold': assert accepted['nodes'][node]==value
artifacts=[v['kind']['recipe']['video']['accepted']['artifact'] for v in accepted['nodes'].values() if v['kind']['type']=='hold']
assert len(artifacts)==2 and artifacts[0]['sampled_object']!=artifacts[1]['sampled_object']
for name in ['fixture-tests','final-worker-tests','build','clippy','format-check','fixed-1','fixed-2']:
    result=read(name+'.json');assert result['exit']==0,name
    assert sha(raw[name+'.log'])==result['log_sha256'],name
assert b'5 passed; 0 failed' in raw['fixture-tests.log']
assert b'45 passed; 0 failed' in raw['final-worker-tests.log']
app_hash=read('binary-pins.json')[str(Path(identity['fixture']).parent/'bin/deadpan-app')]
for name in ['baseline','baseline-2','fixed-1','fixed-2']:
    summary=read(name+'/generated-playback.json');report=read(name+'/report.json')
    measured=read('measurements.json')[name]
    assert summary['frames']==180 and summary['generated_frames']==60 and summary['ended']
    assert summary['audio']['underruns']==summary['audio']['faults']==0
    coverage={p['frame'] for p in summary['pictures']}
    assert summary['missing_frames']==sorted(set(range(180))-coverage)
    assert measured['request_to_composed_ms']['count']==measured['worker_ms']['count']==len(coverage)
    entry=next(t for t in measured['transitions'] if t['frame']==91)
    if name.startswith('baseline'):
        assert read(name+'.json')['exit']==1 and entry['worker_ms']>150
        assert set(range(92,102))<=set(summary['missing_frames'])
        assert report['metadata']['binary_sha256']==identity['baseline_app_sha256']
    else:
        assert coverage==set(range(180)) and entry['worker_ms']<1
        assert summary['native_picture_counts']['skipped']==summary['native_picture_counts']['reduced']==0
        assert report['metadata']['binary_sha256']==app_hash
        for scenario in report['scenarios']:
            assert all(c['passed'] for c in scenario['checks']),name
            assert all(f['severity']!='failure' for f in scenario['findings']),name
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/PLAYBACK.md','docs/PROJECT_PICTURES.md','docs/PERFORMANCE.md','docs/qualification/generated-transitions-2026-10-11.md']:
    page=repo/name
    for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
        if ':' in target or target.startswith('#'): continue
        path=target.split('#',1)[0].strip('<>')
        if path: assert (page.parent/path).exists(),(name,target)
        links+=1
print(json.dumps(dict(files=len(manifest),sources=len(identity['source_sha256']),links=links,final_pictures=360)))
