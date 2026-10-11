from pathlib import Path
import gzip,hashlib,json,re,sys
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-loops'
def sha(data):return hashlib.sha256(data).hexdigest()
manifest=json.loads((root/'manifest.json').read_text());raw={}
assert len({r['path'] for r in manifest})==len(manifest)
for row in manifest:
    data=(root/row['path']).read_bytes()
    assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
    plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
    assert sha(plain)==row['original_sha256'],row['path']
    raw[row['path'].removesuffix('.gz')]=plain
def read(name):return json.loads(raw[name])
identity=read('identity.json')
for name,expected in identity['sources'].items():
    assert sha(raw['source/'+name])==expected,name
    if '--current' in sys.argv: assert sha((repo/name).read_bytes())==expected,name
for label in ['baseline','fixed','verified']:
    pins=read(label+'-pins.json')
    for name,expected in pins['source'].items():assert sha(raw[label+'-source/'+name])==expected,name
    for name,expected in pins['binary'].items():assert sha(Path(name).read_bytes())==expected,name
assert sha(Path(identity['old_control_app']).read_bytes())==identity['old_control_app_sha256']
assert sha(Path(identity['test_executable']).read_bytes())==identity['test_executable_sha256']
for name,expected in identity['libraries'].items():assert sha(Path(name).read_bytes())==expected,name
for name,path in identity['fixtures'].items():
    for relative,pin in read(name+'-fixture-pins.json').items():
        data=(Path(path)/relative).read_bytes()
        assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],relative
for name,step in read('4k/fixture-steps.json').items():
    assert step['exit']==0,name
    for suffix in ['stdout','stderr']: assert sha(raw['4k/'+name+'.'+suffix])==step[suffix+'_sha256'],name
assert read('4k/generate.stdout')['state']=='Ready'
before=read('4k/before.stdout');after=read('4k/accepted.stdout')
for node,value in before['nodes'].items():
    if value['kind']['type']=='hold':assert after['nodes'][node]==value
for name in ['worker-tests','fixture-tests','original-4k-test','option-tests','coverage-tests','delivery-coverage-tests','build','clippy','format-check','verified-clippy','verified-format','fixed-1080p','fixed-4k','verified-1080p','verified-4k','control-old','control-repeat']:
    result=read(name+'.json');assert result['exit']==0,name
    assert sha(raw[name+'.log'])==result['log_sha256'],name
for name,count in [('worker-tests',45),('fixture-tests',6),('original-4k-test',1),('option-tests',6),('coverage-tests',1),('delivery-coverage-tests',1)]:
    assert f'{count} passed; 0 failed'.encode() in raw[name+'.log'],name
for name in ['baseline-1080p','baseline-4k','control']:
    result=read(name+'.json');assert result['exit']==1,name
    assert sha(raw[name+'.log'])==result['log_sha256'],name
assert read('initial-1080p-coverage.json')['missing']==[]
baseline=read('baseline-4k/generated-loop.json')
assert baseline['missing_frames']==[16] and baseline['native_picture_counts']['skipped']==1
assert read('measurements.json')['baseline-4k']['boundary_groups']['45']['first']['worker_ms']>60
for size,start,end in [('1080p',60,121),('4k',30,61)]:
    name='verified-'+size
    value=read(name+'/generated-loop.json');report=read(name+'/report.json')
    assert value['ended'] and value['looping'] and not value['lost_device_lap'] and value['stop_preserved_picture']
    assert value['window_frames']==[start,end] and value['expected_laps']==30
    assert value['pictures'][-1]['heard_lap']>=30 and value['elapsed_ms']>=30_000
    assert value['missing_frames']==[]
    assert {(p['lap'],p['frame']) for p in value['pictures']}=={(lap,frame) for lap in range(30) for frame in range(start,end)}|{(30,start)}
    assert value['audio']['underruns']==value['audio']['faults']==value['audio']['silent_padding_frames']==0
    assert value['native_picture_counts']['skipped']==value['native_picture_counts']['reduced']==0
    assert report['metadata']['fixture_sha256']==read(size+'-fixture-pins.json')['project.sqlite']['sha256']
    assert report['metadata']['binary_sha256']==next(v for k,v in read('verified-pins.json')['binary'].items() if k.endswith('/deadpan-app'))
    measurement=read('measurements.json')[name]
    assert measurement['worker_ms']['count']==measurement['composition_ms']['count']==len(value['pictures'])
    for scenario in report['scenarios']:
        assert all(c['passed'] for c in scenario['checks']),name
        assert all(f['severity']!='failure' for f in scenario['findings']),name
assert read('control/generated-playback.json')['missing_frames']==[63]
for name in ['control-old','control-repeat']:
    value=read(name+'/generated-playback.json')
    assert value['missing_frames']==[] and value['frames']==180
    report=read(name+'/report.json')
    for scenario in report['scenarios']:assert all(c['passed'] for c in scenario['checks']),name
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/PLAYBACK.md','docs/UI_FEEDBACK.md','docs/PERFORMANCE.md','docs/qualification/generated-loops-2026-10-11.md']:
    page=repo/name
    for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
        if ':' in target or target.startswith('#'):continue
        path=target.split('#',1)[0].strip('<>')
        if path:assert (page.parent/path).exists(),(name,target)
        links+=1
print(json.dumps(dict(files=len(manifest),sources=len(identity['sources']),links=links,loop_pictures=2762)))
