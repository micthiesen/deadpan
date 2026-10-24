#!/usr/bin/env python3
"""Independent bounded offline audit of ordinary Repeat qualification receipts."""
import collections, copy, datetime, difflib, hashlib, json, re, sqlite3, stat, struct, sys, traceback, zlib
from fractions import Fraction as F
from pathlib import Path
S=Path('/tmp/deadpan-repeat-follow-creep-2026-10-24')
R=Path('/Users/michael/Code/deadpan')
PINS={}
def need(v,m):
    if not v: raise AssertionError(m)
def meta(p):
    p=Path(p); need(stat.S_ISREG(p.lstat().st_mode),f'not regular: {p}')
    h=hashlib.sha256(); n=0
    with p.open('rb') as f:
        for b in iter(lambda:f.read(1048576),b''): h.update(b); n+=len(b)
    return dict(bytes=n,sha256=h.hexdigest())
def read(p):
    p=Path(p); p=p if p.is_absolute() else S/p
    m=meta(p); need(m['bytes']<=268435456,'bounded input'); PINS[str(p)]=m
    return p.read_bytes()
def obj(p): return json.loads(read(p))
def pinread(p,m):
    read(p); need(meta(p if Path(p).is_absolute() else S/p)==m,f'pin: {p}')
def tests(n):
    t=read(n+'.stdout').decode(); matches=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',t,re.M)
    need(matches,f'raw test summary: {n}')
    names=re.findall(r'^test (.+) \.\.\. ok$',t,re.M)
    ignores=re.findall(r'^test (.+) \.\.\. ignored(?:, (.*))?$',t,re.M)
    counts=[sum(int(m[i]) for m in matches) for i in (1,2,3)]
    need(len(names)==counts[0] and len(ignores)==counts[2],f'named test counts: {n}')
    return dict(passed=counts[0],failed=counts[1],ignored=counts[2],summaries=len(matches),filtered=[int(m[5]) for m in matches],passed_names=names,ignored_tests=[dict(name=n,reason=r) for n,r in ignores])
def receipt(n,code=0):
    d=obj(n+'.json')
    need(d['cwd']==str(R) and d['exit_code']==code and 'finished_at' in d and d['seconds']>=0,f'terminal: {n}')
    need(d['runner_sha256']==meta(S/'run.py')['sha256'],f'runner: {n}')
    need(d['source_before']==d['source_after'] and d['sources_unchanged'] is True,f'source scope: {n}')
    need(d['binary_before']==d['binary_after'] and d['binaries_unchanged'] is True,f'binary scope: {n}')
    for p,m in d['streams'].items(): pinread(p,m)
    for p,m in d['binary_before'].items(): need(meta(p)==m,f'retained binary: {n}')
    return d
def rows(root):
    db=root/'project.sqlite'; wal=root/'project.sqlite-wal'
    need(not wal.exists() or meta(wal)['bytes']==0,'empty WAL before immutable SQLite')
    m=meta(db)
    with sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True) as c:
        need(c.execute('pragma quick_check').fetchall()==[('ok',)],'SQLite integrity')
        tables={t:sorted(c.execute('SELECT * FROM "'+t+'"').fetchall(),key=repr) for t in ('revisions','revision_patches','history','history_receipt','redo','state','transaction_steps')}
    need(meta(db)==m,'SQLite unchanged')
    PINS[str(db)]=m
    return tables
def png(p,m):
    b=read(p); need(meta(S/p)==m and b[:8]==b'\x89PNG\r\n\x1a\n','Root PNG identity')
    i=8; kinds=[]; size=None
    while i<len(b):
        n=struct.unpack('>I',b[i:i+4])[0]; tag=b[i+4:i+8]; end=i+12+n
        need(end<=len(b) and n<=16777216,'bounded PNG chunk')
        need(zlib.crc32(b[i+4:i+8+n])&0xffffffff==struct.unpack('>I',b[i+8+n:end])[0],'PNG CRC')
        if tag==b'IHDR': size=struct.unpack('>II',b[i+8:i+16])
        kinds.append(tag); i=end
        if tag==b'IEND': break
    need(i==len(b) and kinds[0]==b'IHDR' and kinds[-1]==b'IEND' and all(0<x<=8192 for x in size),'complete PNG')
    return dict(path=p,**m,width=size[0],height=size[1])
def dictionaries(v):
    if isinstance(v,dict):
        yield v
        for x in v.values(): yield from dictionaries(x)
    elif isinstance(v,list):
        for x in v: yield from dictionaries(x)
def media(folder,final,movie_name,movie_pin,revisions,history_count):
    cmds=obj(folder+'/commands.json')
    need([c['name'] for c in cmds]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'] and all(c['exit_code']==0 and c['seconds']>=0 for c in cmds),'nine terminal CLI media commands')
    for c in cmds:
        need(meta(c['command'][0])=={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')},'media actual CLI')
        read(folder+'/'+c['name']+'.stdout'); read(folder+'/'+c['name']+'.stderr')
    doc=obj(folder+'/dump.stdout')
    need(doc==obj(folder+'/portable-dump.stdout'),'portable exact document')
    valid=obj(folder+'/validate.stdout')
    need(valid==obj(folder+'/portable-validate.stdout') and valid['valid'] is True and valid['revision_id']==doc['revision_id'],'real Store validation')
    v=obj(folder+'/verification.json')
    need(v==obj(folder+'/verify.stdout') and v['passed'] is True and v['failures']==[] and v['range']==[0,80] and v['frame_rate']==[30000,1001] and v['revision_id']==doc['revision_id'],'full movie verified')
    need(len(v['pictures'])==80 and all(p['passed'] is True and p['flags']==[] for p in v['pictures']) and [p['output_frame'] for p in v['pictures']]==list(range(80)),'all80 picture rows')
    obs=collections.Counter(p[k]['status'] for p in v['pictures'] for k in ('previous_index','next_index') if p[k] is not None)
    need(sum(obs.values())==158 and obs['observable']==v['summary']['index_observable_comparisons'] and obs['unobservable']==v['summary']['index_unobservable_comparisons'],'observability raw/summary')
    a=v['audio']; need(len(a)==1,'one complete audio window'); a=a[0]
    need(a['passed'] is True and a['flags']==[] and a['window']==a['project_samples']==[0,128128] and a['uncovered_samples']==0 and a['offset_status']=='verified_zero' and a['measured_offset_samples']==0 and all(x['status']=='verified_zero' and x['peak_lag']==0 for x in a['segments']),'full zero-offset stereo audio')
    movie=meta(S/folder/movie_name); need(movie==movie_pin,'movie bytes')
    m=v['movie']
    need(m['bytes']==movie['bytes'] and m['video_frames']==m['expected_video_frames']==80 and m['picture_anomalies']==[] and m['audio']['gaps']==[] and m['audio']['presented_end']==m['expected_audio_samples']==128128 and m['audio']['first_pts']==-2048 and m['audio']['physical_samples']==131072 and m['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=128128) and m['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=80080),'AAC/video edit metadata')
    pubs=[d for line in read(folder+'/render.stdout').splitlines() for d in dictionaries(json.loads(line)) if 'movie_sha256' in d]
    need(pubs and all(x==pubs[0] for x in pubs) and pubs[0]['movie_sha256']==movie['sha256'] and pubs[0]['movie_bytes']==movie['bytes'],'raw render publication')
    pub=pubs[0]; pinread(S/folder/Path(pub['report']).name,dict(bytes=pub['report_bytes'],sha256=pub['report_sha256']))
    copy=obj(folder+'/copy.stdout')['portable_copy']
    need(copy['verified_history']==dict(replayed=0,revisions=revisions,verified_by_receipt=revisions),'portable full history receipt')
    original=Path(cmds[0]['command'][3]); portable=S/folder/'portable.deadpan'
    aa,bb=rows(original),rows(portable)
    need(aa==bb and len(aa['revisions'])==revisions and len(aa['history'])==history_count,'complete immutable history table equality')
    return doc,dict(commands=9,movie=movie,picture_rows=80,picture_neighbor_observability=dict(obs),audio_window=[0,128128],audio_offset=0,movie_metadata=m,portable_authored_document_equal=True,history_tables_counts={k:len(v) for k,v in aa.items()},all_selected_history_tables_equal=True,project=str(original),portable=str(portable))

def clipped(final,native_document):
    folder='clipped-media-final'; root=S/folder
    run=receipt('clipped-media-final',1)
    need(run['source_before']=={str(R/p):m for p,m in final['sources'].items()},'clipped final21 receipt')
    inputs=obj(folder+'/inputs.json'); term=obj(folder+'/terminal.json')
    freeze=obj('clipped-media-script-freeze.json')
    need(meta(S/'clipped-media.py')==inputs['script']==term['script_after']=={k:freeze[k] for k in ('bytes','sha256')} and run['command'][-1]==inputs['script']['sha256'],'actual clipped script pin')
    need(term['status']=='failed' and term['commands']==181 and term['native_authoritative_equal'] is False and not (root/'result.json').exists(),'original failed terminal remains explicit')
    failure=obj(folder+'/failure.json');need(failure['error']=="AssertionError('persistent native package bytes changed')",'exact late guard failure')
    cmds=obj(folder+'/commands.json')
    need(len(cmds)==181 and len({c['name'] for c in cmds})==181 and all(c['exit_code']==0 and c['seconds']>=0 for c in cmds),'all181 actual CLI commands passed')
    cli={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')}
    for c in cmds:
        need(meta(c['argv'][0])==cli,'derived CLI actual binary')
        for suffix in ('stdout','stderr'):pinread(root/(c['name']+'.'+suffix),c[suffix])
    need(inputs['binaries']==term['binaries_after'] and all(meta(p)==m for p,m in inputs['binaries'].items()),'derived runtime helpers unchanged')
    old=inputs['native_authoritative_before'];end=term['native_authoritative_after']
    empty=dict(bytes=0,sha256=hashlib.sha256(b'').hexdigest())
    need(set(end)-set(old)=={'project.sqlite-wal'} and end['project.sqlite-wal']==empty and all(end[p]==m for p,m in old.items()),'only absent-to-empty native WAL changed')
    native=Path(inputs['native_project'])
    live={str(p.relative_to(native)):meta(p) for p in sorted(native.rglob('*')) if p.is_file() and p.name!='project.sqlite-shm'}
    need(live==end,'current native persistent bytes match terminal')
    before=obj(folder+'/native-document-before.stdout');after=obj(folder+'/copy-document-after.stdout')
    need(before==native_document==obj(folder+'/copy-document-before.stdout')==obj(folder+'/native-document-after.stdout'),'native authored document unchanged')
    expected=copy.deepcopy(before);expected['revision_id']=after['revision_id'];expected['targets']['target-1']['corrections']=[dict(at=12012,region=dict(center=[980000,500000]))]
    need(after==expected,'derived target-only correction and revision')
    need(obj(folder+'/portable-dump.stdout')==after,'copied derived document equality before undo')
    def no_revision(x):
        x=copy.deepcopy(x);x.pop('revision_id',None);return x
    need(no_revision(obj(folder+'/portable-undone-dump.stdout'))==no_revision(before) and no_revision(obj(folder+'/portable-redone-dump.stdout'))==no_revision(after),'actual portable undo/redo content')
    aa,bb=obj(folder+'/plan-before.stdout'),obj(folder+'/plan-after.stdout')
    for v in (aa,bb):v['plan']['metadata'].pop('revision_id')
    need(aa==bb,'complete inspect-plan metadata preserved')
    need(no_revision(obj(folder+'/audio-plan-before.stdout')['audio'])==no_revision(obj(folder+'/audio-plan-after.stdout')['audio']),'complete audio plan preserved')
    owner=next(n for n,v in before['nodes'].items() if v['kind']['type']=='repeat')
    fallback=before['nodes'][owner]['framing']['value']['fallback']
    q=1<<32
    def ratio(x):return F(int(x['numerator']),int(x['denominator']))
    def grid(x):return F(round(x*q),q)
    observations=obj(folder+'/independent-plan-observations.json');need(len(observations)==80,'80 retained pose observations')
    clipped_frames=[]
    for j in range(80):
        oldsample=obj(folder+f'/frame-before-{j:03}.stdout')['sample']
        sample=obj(folder+f'/frame-after-{j:03}.stdout')['sample']
        play,within=divmod(j,28);gap=within>=24;ordinal=23 if gap else within
        percent=58 if ordinal<12 else 98
        zoom=F(8+play,8);projected=F(1,2)+(F(percent,100)-F(1,2))*zoom;visible=0<=projected<=1
        if not visible:clipped_frames.append(j)
        layers=[x for x in sample['framing'] if x['instance']['node']==owner and not x['escalation']]
        need(len(layers)==1,'exact outer owner layer')
        layer=layers[0];pose=layer['pose']
        wantx=grid(projected) if visible else ratio(fallback['center_x'])
        progress=grid(F(2*j+1,160));base=grid(F(11,10));endpoint=grid(F(11,5));scale=grid(base+(endpoint-base)*progress)
        need(ratio(layer['local_position'])==F(2*j+1,2) and ratio(layer['effective_duration'])==80 and ratio(pose['center_x'])==wantx and ratio(pose['center_y'])==F(1,2) and ratio(pose['scale'])==scale,'independent half-even Q32 pose/clock')
        obs=observations[j]
        need(obs['Edit']==j and obs['Original']==ordinal and obs['play']==play and obs['gap']==gap and obs['visible']==visible and ratio(obs['projected_before_clip'])==projected and ratio(obs['expected_center'])==wantx and obs['actual_pose']==pose,'raw observations equal independent recomputation')
        esc=[x for x in sample['framing'] if x['escalation']]
        need(len(esc)==(0 if play==0 else 1),'separate escalation layer count')
        if esc:need(esc[0]['instance']==layer['instance'] and ratio(esc[0]['pose']['scale'])==zoom and ratio(esc[0]['pose']['center_x'])==ratio(esc[0]['pose']['center_y'])==F(1,2),'unchanged inner centered escalation')
        for v in sample['framing']:
            if v is not layer and not v['escalation']:need(v['pose'] is None,'all other layers identity')
        picture=sample['picture'];need(picture['asset']==before['targets']['target-1']['asset'] and picture['point']['time_base']==dict(numerator=1,denominator=30000),'source identity/timebase')
        if gap:
            need(picture['type']=='freeze' and ratio(picture['point']['ticks'])==23023 and sample['picture_context']==before['nodes'][owner]['kind']['gap']['picture_context'] and sample['gap_after']['ordinal']==play and sample['instance']==dict(node=owner,repeats=[]),'captured default gap ownership/source')
        else:
            need(picture['type']=='source' and ratio(picture['point']['ticks'])==F((2*ordinal+1)*1001,2) and sample.get('picture_context') is None and sample.get('gap_after') is None and sample['instance']['repeats'][0]['iteration']['ordinal']==play,'play resets Original coordinates')
        x,y=no_revision(oldsample),no_revision(sample)
        for value in (x,y):
            for v in value['framing']:
                if v['instance']['node']==owner and not v['escalation']:v.pop('pose')
        need(x==y,'complete non-outer sample fields preserved')
    need(clipped_frames==list(range(40,56))+list(range(68,80)),'exact28 clipped frames')
    verify=obj(folder+'/verification.json');need(verify==obj(folder+'/verify.stdout') and verify['passed'] is True and verify['failures']==[] and verify['revision_id']==after['revision_id'] and verify['range']==[0,80] and verify['frame_rate']==[30000,1001],'derived full movie result')
    need(len(verify['pictures'])==80 and all(p['passed'] is True and p['flags']==[] for p in verify['pictures']) and [p['output_frame'] for p in verify['pictures']]==list(range(80)),'derived80 pictures')
    for j,p in enumerate(verify['pictures']):
        ordinal=23 if j%28>=24 else j%28
        need(p['provenance']==dict(kind='original',asset=before['targets']['target-1']['asset'],source_frame=ordinal,source_pts=ordinal*1001),'all derived encoded picture provenance')
    obs=collections.Counter(p[k]['status'] for p in verify['pictures'] for k in ('previous_index','next_index') if p[k] is not None)
    need(obs==dict(observable=104,unobservable=54),'derived observability limits')
    a=verify['audio'];need(len(a)==1 and a[0]['passed'] is True and a[0]['flags']==[] and a[0]['window']==a[0]['project_samples']==[0,128128] and a[0]['uncovered_samples']==0 and a[0]['offset_status']=='verified_zero' and a[0]['measured_offset_samples']==0 and all(x['status']=='verified_zero' and x['peak_lag']==0 for x in a[0]['segments']),'derived full zero-offset audio')
    movie=meta(root/'cli-derived-clipped-repeat.mp4');m=verify['movie']
    need(m['bytes']==movie['bytes']==91654 and m['video_frames']==m['expected_video_frames']==80 and m['picture_anomalies']==[] and m['audio']['gaps']==[] and m['audio']['presented_end']==m['expected_audio_samples']==128128 and m['audio']['first_pts']==-2048 and m['audio']['physical_samples']==131072 and m['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=128128) and m['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=80080),'derived actual movie/AAC clocks')
    pubs=[d for line in read(folder+'/render.stdout').splitlines() for d in dictionaries(json.loads(line)) if 'movie_sha256' in d]
    need(pubs and all(p==pubs[0] for p in pubs) and pubs[0]['movie_sha256']==movie['sha256'] and pubs[0]['movie_bytes']==movie['bytes'],'derived publication hash')
    pub=pubs[0];pinread(root/Path(pub['report']).name,dict(bytes=pub['report_bytes'],sha256=pub['report_sha256']))
    derived=rows(root/'cli-derived-clipped.deadpan');portable=rows(root/'cli-derived-history-portable.deadpan')
    need(len(derived['revisions'])==21 and len(portable['revisions'])==23 and len(derived['history'])==len(portable['history'])==13,'derived/portable actual history counts')
    for t in ('history','redo','transaction_steps'):need(derived[t]==portable[t],'copied complete history unchanged after undo/redo')
    for t in ('revisions','revision_patches'):need(set(derived[t])<=set(portable[t]) and len(set(portable[t])-set(derived[t]))==2,'all copied revision rows plus exact two undo/redo rows')
    need(derived['state'][0][2:]==portable['state'][0][2:] and derived['state'][0][1]==after['revision_id'] and portable['state'][0][1]==obj(folder+'/portable-redone-dump.stdout')['revision_id'],'history cursor unchanged/current redo head')
    need(derived['history_receipt'][0][2]==21 and portable['history_receipt'][0][2]==23 and derived['history_receipt'][0][1]==portable['history_receipt'][0][1] and derived['history_receipt'][0][5:]==portable['history_receipt'][0][5:],'current independently validated history receipt revision/head advance')
    need(obj(folder+'/copy-derived.stdout')['portable_copy']['verified_history']==dict(replayed=0,revisions=21,verified_by_receipt=21),'copy derived21 receipt')
    reconciliation=obj('clipped-media-reconciliation.json')
    pinread(S/'clipped-media-reconciliation.py',reconciliation['reconciler'])
    need(reconciliation['reconciliation_status']=='passed_with_original_guard_failure_preserved' and reconciliation['original_run']['exit_code']==1 and reconciliation['original_run']['commands']==181 and reconciliation['original_run']['all_product_commands_exit_zero'] is True and reconciliation['original_run']['failure_verbatim']==read(folder+'/failure.json').decode() and reconciliation['original_run']['outer_stderr_verbatim']==read('clipped-media-final.stderr').decode(),'honest separate reconciliation status')
    need(reconciliation['only_allowed_guard_delta']=={'project.sqlite-wal':dict(before=None,after=empty)} and reconciliation['native_authored_document_equal'] is True and reconciliation['all_preexisting_native_persistent_files_equal'] is True,'exact reconciliation metadata allowance')
    recovered=reconciliation['recovered_result']
    need(recovered['clipped_frames']==clipped_frames and recovered['clipped_count']==28 and recovered['frames_checked']==80 and recovered['movie']==movie and recovered['saved_fallback_pose']==fallback and recovered['audio_samples_verified']==128128 and recovered['audio_offset_samples']==0 and recovered['revision']==after['revision_id'],'reconciliation result equals independent recomputation')
    for p,m in reconciliation['verified_input_pins'].items():need(meta(p)==m,'reconciliation input bytes still exact')
    return dict(label='CLI-derived, not native-authored',actual_run_exit=1,late_guard_error=failure['error'],actual_commands=181,all_command_exits_zero=True,only_native_persistent_delta=dict(path='project.sqlite-wal',before='absent',after=empty),native_authored_document_unchanged=True,clipped_frames=clipped_frames,clipped_count=28,saved_fallback_center=dict(numerator='311385129',denominator='536870912'),independent_raw80_pose_recompute=True,full_audio_plan_equal=True,movie=movie,pictures=80,audio_window=[0,128128],audio_offset=0,picture_neighbor_observability=dict(obs),derived_history_counts={k:len(v) for k,v in derived.items()},portable_history_counts={k:len(v) for k,v in portable.items()},all_copied_revision_history_rows_preserved=True,portable_authored_equal_before_undo=True,undo_redo_content_equal=True,reconciliation=reconciliation)

def main():
    out=dict(schema_version=1,status='running',findings=[],base='cb89b8b79a01a4ad59fc29dc572ed65015a3c3af',started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    before={}
    try:
        final,first=obj('final-identities.json'),obj('first-identities.json'); paths=obj('source-files.json')
        need(len(paths)==len(set(paths))==21 and set(paths)==set(final['sources']),'final21 scope')
        before={p:meta(R/p) for p in paths}
        need(before==final['sources']==first['sources'],'current/first/final21 source bytes')
        for phase,ids in [('first',first),('final',final)]:
            for p,m in ids['sources'].items(): pinread(S/('source-'+phase)/(p+'.txt'),m)
            for n,m in ids['binaries'].items(): need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'retained executed binary')
        need(set(final['binaries'])==set(first['binaries']) and len(final['binaries'])==5 and all({k:final['binaries'][n][k] for k in ('bytes','sha256')}=={k:first['binaries'][n][k] for k in ('bytes','sha256')} for n in final['binaries']),'one identical five-binary candidate')
        eq=obj('candidate-equivalence.json')
        need(eq['source_paths']==21 and eq['all_source_bytes_identical'] is True and eq['all_five_binary_bytes_identical'] is True,'candidate equivalence')
        read('run.py'); read('media.py'); read('worker-run.py'); read('plan-run.py')
        camera='crates/deadpan-app/src/preview/camera.rs'; note=obj('camera-help-source-note.json')
        old=read('camera-before-live-clip-help.rs.txt'); current=read(R/camera)
        need(meta(S/'camera-before-live-clip-help.rs.txt')==note['before'] and meta(R/camera)==note['after'],'Camera hint pins')
        a=b'when clipped by a captured view'; b=b'when an earlier crop hides it'
        need(old.count(a)==2 and old.replace(a,b)==current,'exactly two Camera hint string replacements')
        absfinal={str(R/p):m for p,m in final['sources'].items()}
        early={**absfinal,str(R/camera):note['before']}
        runs={}
        for n in ['core-plan-final','app-units-final','release-final','clippy-first','format-final','app-check-first','native-repeat-run','native-regressions','native-zoom-final','native-hold-follow-creep-final','media-final']:
            runs[n]=receipt(n); need(runs[n]['source_before']==absfinal,f'final21 receipt:{n}')
        runs['commands-write']=receipt('commands-write',101)
        runs['live-clip-fixed']=receipt('live-clip-fixed')
        need(runs['live-clip-fixed']['source_before']==early,'Render fixed early21 hint-only scope')
        initial=receipt('live-clip-before',101)
        delta={p.removeprefix(str(R)+'/'):m for p,m in initial['source_before'].items() if m!=absfinal[p]}
        expected={camera,'crates/deadpan-cli/tests/project_commands/macros/framing/repeat_follow_creep.rs','crates/deadpan-plan/src/plan.rs','crates/deadpan-plan/tests/picture_plan/captured_follow.rs','crates/deadpan-plan/tests/picture_plan/follow.rs'}
        need(set(delta)==expected,'initial actual five pre-fix source differences')
        old_fix=obj('before-live-clip-fix.json')
        need(len(old_fix)==3,'pre-fix helper and render source pins')
        for p,m in old_fix.items(): pinread(S/'before-live-clip-fix'/(p+'.txt'),m)
        for p in ('crates/deadpan-plan/tests/picture_plan/captured_follow.rs','crates/deadpan-plan/tests/picture_plan/follow.rs'):
            pinread(S/'before-live-clip-fix'/(p+'.txt'),delta[p])
        unit=obj('app-unit-executable-terminal.json')
        need(meta(unit['path'])=={k:unit[k] for k in ('bytes','sha256')} and 'No live hash' in unit['scope'],'terminal-only App unit observation')
        results={}
        for n,expected in [('core-plan-final',(2053,0,0)),('app-units-final',(1304,0,11)),('worker-core-1',(2,0,0)),('worker-cli-1',(14,0,0)),('plan-focused-1',(3,0,0)),('live-clip-before',(0,1,0)),('live-clip-fixed',(3,0,0))]:
            t=tests(n); need(tuple(t[k] for k in ('passed','failed','ignored'))==expected,f'raw test totals:{n}');results[n]=t
        worker=obj('worker-repeat-final-freeze.json'); need(worker['base']==out['base'],'worker base')
        wscopes={}
        for n in ['worker-core-1','worker-cli-1']:
            d=obj(n+'.json')
            need(d['cwd']==str(R) and d['exit_code']==0 and 'finished_at' in d and d['owned_unchanged'] is True and d['basis_unchanged'] is True and d['global_source_unchanged'] is True and d['owned_before']==d['owned_after'] and d['basis_before']==d['basis_after'] and d['global_source_before']==d['global_source_after'],'focused worker terminal/scopes')
            need(len(d['basis_before'])==477 and all(meta(R/p)['sha256']==h for p,h in d['basis_before'].items()),'current Core/Plan/CLI477 basis')
            need({p:meta(R/p)['sha256'] for p in d['owned_before']}==d['owned_before'],'current worker7')
            need({p:h for p,h in d['global_source_before'].items() if final['sources'][p]['sha256']!=h}=={camera:note['before']['sha256']},'worker global hint-only delta')
            for suffix,h in d['stream_pins'].items(): need(meta(S/(n+suffix))['sha256']==h,'worker raw stream hashes')
            for p,h in d['owned_before'].items(): need(meta(Path(d['source_copies'])/(p+'.txt'))['sha256']==h,'worker source snapshots')
            wscopes[n]=dict(owned=7,basis=477,global_scope=21,later_hint_only_delta=[camera],executed_binary_observations=d['executed_binary_pins'])
        pd=obj('plan-focused-1.json')
        need(pd['exit_code']==0 and 'finished_at' in pd and pd['owned_sources_unchanged'] is True and pd['production_basis_unchanged'] is True and pd['main_manifest_unchanged'] is True and pd['source_before']==pd['source_after'] and pd['basis_before']==pd['basis_after'] and pd['main_manifest_before']==pd['main_manifest_after']==final['sources'],'Plan terminal/current21')
        need(len(pd['basis_before'])==287 and all(meta(R/p)==m for p,m in pd['basis_before'].items()) and all(final['sources'][p]==m for p,m in pd['source_before'].items()),'current Plan2/CorePlan287')
        need(pd['runner_sha256']==meta(S/'plan-run.py')['sha256'],'Plan runner')
        for n in results['plan-focused-1']['passed_names']: need(results['core-plan-final']['passed_names'].count(n)==1,'focused Plan exact name in final broad')
        for name in results['worker-core-1']['passed_names']: need(results['core-plan-final']['passed_names'].count(name)==1,'focused Core exact name in final broad')
        need(results['worker-cli-1']['filtered']==[46] and any('repeat_follow_creep' in n for n in results['worker-cli-1']['passed_names']),'new/existing fourteen CLI framing cases')
        live=read(R/'crates/deadpan-render/tests/follow_letterbox/live.rs'); capture=read(R/'crates/deadpan-render/tests/follow_letterbox/captured.rs')
        need(b'assert_eq!(visible + clipped, 600)' in live and b'for is_point in [false, true]' in capture and b'for context in &contexts' in capture and b'for inner in [vec![], vec![Some(pose(q(2, 5), q(3, 2))), None]]' in capture,'cross-renderer live600/captured grid source')
        launch=obj('native-repeat-first.json'); launchnote=obj('native-first-launch-error.json')
        need('pid' not in launch and 'exit_code' not in launch and launch['command'][0]=='--binary' and read('native-repeat-first.stdout')==read('native-repeat-first.stderr')==b'' and 'before app launch' in launchnote['failure'],'pre-spawn wrapper failure truthful incomplete receipt')
        scope=obj('native-regressions-scope-note.json'); need(scope['actual_scope']==['sequence-follow-creep','kestrel-shortcuts'],'regression scenario scope note')
        native=[]
        for n,scenario,count,warnings in [('native-repeat-run','repeat-follow-creep',129,3),('native-regressions','sequence-follow-creep',96,3),('native-zoom-final','zoom',213,3),('native-hold-follow-creep-final','hold-follow-creep',67,2)]:
            d=obj(n+'/report.json'); md=d['metadata']
            need(d['failed'] is False and md['build_profile']=='release' and md['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and md['physical_display_measured'] is False and md['gpu_readback_in_timing_run'] is False,'native report metadata')
            scenes={x['name']:x for x in d['scenarios']}; need(set(scenes)=={scenario,'kestrel-shortcuts'} and len(scenes[scenario]['checks'])==count,'actual scenario/count')
            need(all(c['passed'] is True for x in scenes.values() for c in x['checks']) and not any(f['severity']=='failure' for x in scenes.values() for f in x['findings']),'all native checks passed')
            k=scenes['kestrel-shortcuts']['checks']; need(len(k)==1 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['conflicts']==[],'actual Kestrel routing')
            argv=runs[n]['command']; need(meta(argv[0])['sha256']==md['binary_sha256'] and meta(argv[argv.index('--kestrel-source')+1])['sha256']==k[0]['actual']['source_sha256']==k[0]['actual']['live_source_sha256'],'invoked App/local Kestrel')
            findings=scenes[scenario]['findings']; need(sum(f['severity']=='warning' for f in findings)==warnings,'warning counts')
            native.append(dict(name=n,scenario=scenario,checks=count,kestrel_checks=1,findings=findings,skipped=scenes[scenario]['skipped']))
        doc,mm=media('media-final',final,'repeat-follow-creep.mp4',dict(bytes=105129,sha256='9c8b9d241299e9c4e50ce48cc66849285ab637d06ceb27853f9d71a50cfc5915'),20,12)
        owners=[(n,x) for n,x in doc['nodes'].items() if x['kind']['type']=='repeat']
        need(len(owners)==1,'one Repeat')
        owner,repeat=owners[0]; k=repeat['kind']; f=repeat['framing']['value']
        need(doc['nodes'][doc['root']]['kind']==dict(type='sequence',children=[owner]) and k['iterations']['runs'][0]['count']==3 and len(k['iterations']['runs'])==1 and k['gap']['duration']==4 and k['gap']['audio']==dict(type='silence') and k['gap']['video']['timestamp']==dict(ticks=23023,time_base=dict(numerator=1,denominator=30000)) and k['escalation']==dict(zoom=dict(step=dict(numerator='1',denominator='8'))),'native 3plays/default4gaps/centered escalation')
        need(doc['nodes'][k['child']]['kind']['duration']==24 and 'clock' not in repeat['framing'] and all(not n.get('framing') for i,n in doc['nodes'].items() if i!=owner),'24frame child/owner output only framing')
        need(f['type']=='follow' and f['target']=='target-1' and f['scale']==f['fallback']['scale']==dict(numerator='11',denominator='10') and f['fallback']['center_x']==dict(numerator='311385129',denominator='536870912') and f['scale_transition']==dict(factor=dict(numerator='2',denominator='1'),curve='linear'),'distinct live/fallback bases and outer creep')
        need(doc['targets']['target-1']['region']==dict(center=[580000,500000]) and doc['targets']['target-1']['corrections']==[dict(at=12012,region=dict(center=[680000,500000]))],'native target corrections')
        mr=obj('media-final/result.json');need(mr==json.loads(read('media-final.stdout')) and mr['movie_sha256']==mm['movie']['sha256'] and mr['movie_bytes']==105129 and mr['commands']==9 and mr['expected_frames']==80 and mr['expected_audio_samples']==128128 and mr['portable_document_equal'] is True,'media producer/raw summary')
        visual=obj('visual-review.json'); need(len(visual['images'])==4,'four Root image identities')
        images=[png(p,m) for p,m in visual['images'].items()]
        review=obj('review-notes.json');need(review['base']==out['base'],'scope review base')
        host=obj('host.json');need(host['hw.model']=='Mac17,7' and host['hw.memsize']=='137438953472' and host['hw.ncpu']=='18' and host['kern.hv_vmm_present']=='0','fresh host metadata')
        diagnostics={}
        for p in sorted(S.iterdir()):
            if p.suffix in ('.stdout','.stderr') and not p.name.startswith('audit-repeat'):
                ls=[x for x in read(p).decode(errors='replace').splitlines() if x.startswith(('error:','error[','warning:','warning[','test result: FAILED'))]
                if ls: diagnostics[p.name]=ls
        need(read('format-final.stdout')==read('format-final.stderr')==b'' and 'Finished' in read('clippy-first.stderr').decode(),'final format/strict terminal streams')
        clipped_result=clipped(final,doc)
        out.update(status='no_findings',clipped_media=clipped_result,final_source_count=21,final_sources=final['sources'],final_binaries=final['binaries'],candidate_equivalence=eq,early_camera_hint_only_delta=note,initial_pre_fix_scope=delta,focused_worker_scopes=wscopes,plan_scope=dict(owned=2,basis=287,all_current=True),tests={n:{k:v for k,v in t.items() if k!='passed_names' or t['passed']<=14} for n,t in results.items()},native_reports=native,media_native=mm,unit_terminal_observation=unit,root_images=images,root_visual_review=visual,host=host,retained_diagnostics=diagnostics,wrapper_launch_error=launchnote,regression_scope=scope)
    except Exception as e:
        traceback.print_exc();out['status']='findings';out['findings'].append(dict(type=type(e).__name__,message=str(e)))
    finally:
        out['current_scoped_sources_unchanged']=before=={p:meta(R/p) for p in before}
        if not out['current_scoped_sources_unchanged']:out['status']='findings';out['findings'].append(dict(message='source changed during audit'))
        out['input_file_pins']=PINS;out['verifier_sha256']=meta(Path(__file__))['sha256'];out['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        out['limitations']=['Offline data audit only; no product/Cargo/test/worker/model/decoder/Git invocation.','Twenty-one scoped source witnesses, not a complete repository binary claim. One identical release candidate, not duplicate workflows.','App unit executable observed only after terminal run. No live/execution-time hash is claimed; focused mutable Cargo hashes are historical observations.','Native scripted input/offscreen Metal and Root-selected PNG identities do not qualify physical display/input, VoiceOver, IME, listening or latency.','Movie uses shared committed picture/limited-audio comparison, not independent DSP/listening or encoded PCM bit equality.','All native warnings/skips, eleven existing ignored App tests, old live-clip failure and pre-spawn wrapper error are retained.','Derived CLI clipping run actually exited1 at the final package-byte guard after181 successful commands. A separate offline reconciliation permits only absent-to-empty WAL; original failed terminal remains unchanged. No separate canonical PCM/gain-bit before/after comparison.']
        with (S/'audit-repeat-evidence.json').open('x') as f:json.dump(out,f,indent=2,sort_keys=True);f.write('\n')
        print(json.dumps({k:out.get(k) for k in ('status','findings','final_source_count','current_scoped_sources_unchanged')},sort_keys=True))
    return 0 if out['status']=='no_findings' else 1
if __name__=='__main__':sys.exit(main())
