from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
REPO=Path('/Users/michael/Code/deadpan')
REL=Path('tools/media-qualification/evidence/2026-10-11-source-operations')
committed='--committed' in sys.argv
def read(rel):
    return subprocess.check_output(['git','show','HEAD:'+str(rel)],cwd=REPO) if committed else (REPO/rel).read_bytes()
manifest={name:sha for sha,name in (row.split('  ',1) for row in read(REL/'SHA256SUMS').decode().splitlines())}
def data(rel):
    if str(rel) in manifest:return read(REL/rel)
    return gzip.decompress(read(REL/(str(rel)+'.gz')))
def obj(rel):return json.loads(data(rel))
for name,sha in manifest.items():assert hashlib.sha256(read(REL/name)).hexdigest()==sha,name
for row in obj('compression.json'):
    b=gzip.decompress(read(REL/row['path']));assert len(b)==row['bytes'] and hashlib.sha256(b).hexdigest()==row['sha256']
for name,sha in obj('source-identity.json')['files'].items():assert hashlib.sha256(read(name)).hexdigest()==sha,name
s=obj('checks/matrix-summary.json');assert (s['cases'],s['pictures'],s['signal_windows'],s['zero_offsets'],s['silent_windows'])==(40,1040,36,36,4)
assert len({r['source'] for r in s['rows']})==20
for row in s['rows']:
    prefix='runs/'+row['source']+'/'+row['policy']+'/'
    d=obj(prefix+'verification.json');assert d['passed'] and not d['failures']
    assert d['range']==[0,26] and d['summary']['pictures_checked']==26
    samples={(30000,1001):41642,(50,1):24960,(60000,1001):20821}[tuple(d['frame_rate'])]
    assert d['movie']['expected_audio_samples']==d['movie']['audio']['presented_end']==samples
    assert d['movie']['audio_edits']['media_time']==2048
    assert d['summary']['nonzero_offsets']==0
    for a in d['audio']:
        assert a['passed']
        if a['kind']=='signal':assert a['offset_status']=='verified_zero' and a['measured_offset_samples']==0
        else:assert a['offset_status']=='not_applicable' and a['measured_offset_samples'] is None
    publications=[n for n in manifest if n.startswith(prefix+'exports/') and n.endswith(('.json','.json.gz'))]
    assert len(publications)==1
    pub=obj(publications[0].removesuffix('.gz'))
    movie=data(prefix+'exports/'+pub['movie_filename'])
    assert hashlib.sha256(movie).hexdigest()==pub['destination_readback']['sha256']
    verification=pub['provenance']['verification']
    assert verification['video_frames']==26 and verification['audio_samples']==samples
assert obj('checks/release-matrix-2.json')['exit']==0
assert obj('checks/clippy.json')['exit']==0 and obj('checks/fmt-check.json')['exit']==0
assert obj('checks/release-matrix.json')['exit']!=0
assert 'InvalidIdentity' in data('checks/release-matrix.log').decode()
req=read('docs/REQUIREMENTS.md').decode();rows=[r for r in req.splitlines() if re.match(r'^\| DP-\d\d \|',r)]
assert len(rows)==24 and sum('| Complete |' in r for r in rows)==8
progress=json.loads(read('docs/progress.json'));assert progress['history'][-1]['complete_sections']==8
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/qualification/exact-timing-acceptance-2026-10-11.md']:
    for target in re.findall(r'\]\(([^)]+)\)',read(name).decode()):
        if target.startswith(('http:','https:','#','mailto:')):continue
        target=target.split('#')[0]
        if target:assert (REPO/Path(name).parent/target).exists(),(name,target);links+=1
print(json.dumps({'status':'pass','committed':committed,'manifest_files':len(manifest),'links_checked':links,'exports':40,'pictures':1040,'zero_offset_signal_windows':36,'closed_requirements':8}))
