from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
REPO=Path('/Users/michael/Code/deadpan')
REL=Path('tools/media-qualification/evidence/2026-10-11-local-import')
E=REPO/REL
committed='--committed' in sys.argv
def read(rel):
    if committed:return subprocess.check_output(['git','show','HEAD:'+str(rel)],cwd=REPO)
    return (REPO/rel).read_bytes()
def data(rel):return read(REL/rel)
def obj(rel):
    try:b=data(rel)
    except (FileNotFoundError,subprocess.CalledProcessError):b=gzip.decompress(data(str(rel)+'.gz'))
    return json.loads(b)
manifest=data('SHA256SUMS').decode().splitlines()
for line in manifest:
    sha,name=line.split('  ',1);assert hashlib.sha256(data(name)).hexdigest()==sha,name
for row in obj('compression.json'):
    b=gzip.decompress(data(row['path']));assert len(b)==row['original_bytes'];assert hashlib.sha256(b).hexdigest()==row['original_sha256']
identity=obj('source-identity.json')
for name,sha in identity['files'].items():assert hashlib.sha256(read(name)).hexdigest()==sha,name
base=obj('checks/placed.stdout')
for name in ['missing-dump','corrupt-dump','restored-dump','portable-dump','admission-before','dedup-dump','wrong-stream-dump']:
    assert obj('checks/'+name+'.stdout')==base,name
records=obj('checks/portable-originals.stdout')['originals'];assert len(records)==2
assert all(x['managed'] and x['linked'] is None for x in records)
for name in ['missing-render','corrupt-render','source-offline-render']:
    assert obj('checks/'+name+'.json')['exit']!=0
    lines=data('checks/'+name+'.stdout').decode().splitlines();last=json.loads(lines[-1])['status']
    assert last['outcome']=='failed' and last['diagnostic']['code']=='render_audio_failed'
    assert last['publication_id'] is None and last['receipt'] is None and not last['observed_movie_commit'] and last['cleanup_confirmed']
for name in ['restored','portable']:
    report=obj('checks/'+name+'-render.json');assert report['exit']==0
    assert report['binary_sha256']==identity['binary_sha256']
    lines=data('checks/'+name+'-render.stdout').decode().splitlines();last=json.loads(lines[-1])['status']
    assert last['outcome']=='published' and last['attempt_state']=='verified' and last['observed_movie_commit'] and last['cleanup_confirmed']
    assert hashlib.sha256(data('output/'+name+'.mp4')).hexdigest()==last['receipt']['movie_sha256']
for name in ['restored','portable','catalog']:
    report=obj('checks/'+name+'-verification.json');assert report['passed'] and not report['failures']
    s=report['summary'];assert s['pictures_checked']==120 and s['audio_windows_checked']==3
    assert s['signal_windows']==2 and s['offset_verified_windows']==2 and s['nonzero_offsets']==0
assert obj('checks/acceptance-result.json')['status']=='pass'
assert obj('checks/admission-result.json')['status']=='pass'
assert obj('checks/dedup.stdout')['committed'] is False
assert '36 tests run: 36 passed, 0 skipped' in data('checks/store-regression.log').decode()
req=read('docs/REQUIREMENTS.md').decode();rows=[x for x in req.splitlines() if re.match(r'^\| DP-\d\d \|',x)]
assert len(rows)==24 and sum('| Complete |' in x for x in rows)==7
progress=json.loads(read('docs/progress.json'));assert progress['history'][-1]['complete_sections']==7
assert next(x for x in progress['features'] if x['id']=='import')['status']=='done'
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/SINGLE_ORIGINAL.md','docs/qualification/local-import-acceptance-2026-10-11.md']:
    for target in re.findall(r'\]\(([^)]+)\)',read(name).decode()):
        if target.startswith(('http:','https:','#','mailto:')):continue
        target=target.split('#')[0]
        if target:assert (REPO/Path(name).parent/target).exists(),(name,target);links+=1
print(json.dumps({'status':'pass','committed':committed,'evidence_files':len(manifest),'links_checked':links,'closed_requirements':7,'picture_comparisons':360,'zero_offset_signal_windows':6}))
