#!/usr/bin/env python3
"""Independent bounded offline audit of ordinary Retime qualification receipts."""
import collections, copy, datetime, difflib, hashlib, json, re, sqlite3, stat, struct, sys, traceback, zlib
from fractions import Fraction as F
from pathlib import Path
S=Path('/tmp/deadpan-retime-follow-creep-2026-10-24')
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
def focused(final,broad_names):
    workers={}
    draft='crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs'
    for n,count in [('worker-core-1',2),('worker-cli-1',15)]:
        d=obj(n+'.json')
        need(d['cwd']==str(R) and d['exit_code']==0 and 'finished_at' in d and d['elapsed_seconds']>=0,'focused terminal')
        need(d['owned_unchanged'] is True and d['basis_unchanged'] is True and d['owned_before']==d['owned_after'] and d['basis_before']==d['basis_after'] and len(d['owned_before'])==8 and len(d['basis_before'])==9,'actual own8/named9 scope')
        need(all(meta(R/p)['sha256']==h for p,h in d['owned_before'].items()) and all(meta(R/p)['sha256']==h for p,h in d['basis_before'].items()),'focused current owned and named dependency bytes')
        need(d['global_complete_before'] is True and d['global_complete_after'] is True and d['global_missing_before']==d['global_missing_after']==[] and len(d['global_source_before'])==len(d['global_source_after'])==17,'all17 existed before and after')
        delta={p:dict(before=h,after=d['global_source_after'][p]) for p,h in d['global_source_before'].items() if h!=d['global_source_after'][p]}
        if n=='worker-core-1':
            need(d['global_source_unchanged'] is False and delta=={draft:dict(before='3d7b380519fa7b311a815a2986acd5ab81c90d61b3006e36bd0cc4f5c31549aa',after='3fba182c5c07571e5dbfbda4f532ae2c259f2191c07e85508f982ad1f2c5880c')},'exact unrelated Plan test draft drift')
        else:need(d['global_source_unchanged'] is True and delta=={},'CLI actual global17 unchanged')
        need({p:h for p,h in d['global_source_after'].items()}=={p:m['sha256'] for p,m in final.items()},'focused after map equals final17')
        for p,h in d['owned_before'].items():need(meta(Path(d['source_copies'])/(p+'.txt'))['sha256']==h,'focused retained source copy')
        for suffix,h in d['stream_pins'].items():need(meta(S/(n+suffix))['sha256']==h,'focused exact raw streams')
        t=tests(n);need((t['passed'],t['failed'],t['ignored'])==(count,0,0),'focused actual test totals')
        if n=='worker-core-1':
            need(all(broad_names.count(x)==1 for x in t['passed_names']),'both exact focused Core cases in final broad')
        workers[n]=dict(owned_paths=8,named_dependency_basis_paths=9,global_paths=17,global_complete=True,global_unchanged=d['global_source_unchanged'],global_drift=delta,actual_tests=t,observed_test_executable_pins=d['executed_binary_pins'])
    pd=obj('plan-focused-1.json')
    need(pd['exit_code']==0 and 'finished_at' in pd and pd['owned_sources_unchanged'] is True and pd['core_plan_basis_unchanged'] is True and pd['global17_unchanged'] is True and pd['source_before']==pd['source_after'] and pd['core_plan_basis_before']==pd['core_plan_basis_after'] and pd['global17_before']==pd['global17_after'],'Plan focused terminal guards')
    need(len(pd['source_before'])==2 and len(pd['core_plan_basis_before'])==287,'Plan own2/CorePlan287')
    need(all(meta(R/p)==m for p,m in pd['source_before'].items()) and all(meta(R/p)==m for p,m in pd['core_plan_basis_before'].items()) and pd['global17_before']==final,'Plan current own/basis/global')
    need(pd['runner_sha256']==meta(S/'plan-run.py')['sha256'],'Plan runner identity')
    for p,m in pd['streams'].items():pinread(p,m)
    t=tests('plan-focused-1');need((t['passed'],t['failed'],t['ignored'])==(3,0,0) and t['filtered']==[133] and all(broad_names.count(x)==1 for x in t['passed_names']),'three exact Plan cases in final broad')
    for p,m in pd['source_before'].items():pinread(S/'plan-focused-1-source'/(p+'.txt'),m)
    pf=obj('plan-freeze.json');need(pf['owned_sources']==pd['source_before'] and pf['actual_global17_unchanged_during_focused_run'] is True and pf['all_core_plan_source_basis_unchanged'] is True,'Plan final freeze')
    return dict(workers=workers,plan=dict(owned_paths=2,core_plan_basis_paths=287,global_paths=17,all_unchanged=True,actual_tests=t,observed_test_executable=pd['executed_binary']))
# Final receipt/source/native/media entry point is completed only after Root
# supplies terminal broad/native/media records and final binary equivalence.
def media(final):
    folder='media-final';commands=obj(folder+'/commands.json')
    need([x['name'] for x in commands]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'] and all(x['exit_code']==0 and x['seconds']>=0 for x in commands),'nine actual terminal media commands')
    for x in commands:
        need(meta(x['command'][0])=={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')},'actual executed media CLI')
        read(folder+'/'+x['name']+'.stdout');read(folder+'/'+x['name']+'.stderr')
    doc=obj(folder+'/dump.stdout');need(doc==obj(folder+'/portable-dump.stdout'),'portable complete authored document')
    valid=obj(folder+'/validate.stdout');need(valid==obj(folder+'/portable-validate.stdout') and valid['valid'] is True and valid['revision_id']==doc['revision_id'],'actual Store validate')
    root=doc['nodes'][doc['root']]['kind'];need(root['type']=='sequence' and len(root['children'])==1,'ordinary owner is sole root child')
    owner=root['children'][0];r=doc['nodes'][owner];k=r['kind']
    need(k==dict(type='retime',child=k['child'],duration=120,mapping=dict(start=0,end=80),pitch='follow_speed'),'actual2/3 Retime120 tape wire/default Edit purpose')
    group=doc['nodes'][k['child']]['kind'];need(group['type']=='sequence' and len(group['children'])==2 and not any(n['kind']['type']=='repeat' for n in doc['nodes'].values()),'two-child input Sequence without Repeat')
    def ratio(x):return F(int(x['numerator']),int(x['denominator']))
    def duration(i):
        n=doc['nodes'][i]['kind']
        if n['type']=='source':return n['source']['duration']
        if n['type']=='retime':return n['duration']
        need(n['type']=='sequence','ordinary tree kind');return sum(duration(x) for x in n['children'])
    need([duration(x) for x in group['children']]==[40,40] and duration(k['child'])==80 and duration(owner)==duration(doc['root'])==120,'input80/output120 exact child durations')
    f=r['framing']['value'];need('clock' not in r['framing'] and f['type']=='follow' and f['target']=='target-1' and f['scale']==f['fallback']['scale']==dict(numerator='11',denominator='10') and f['fallback']==dict(center_x=dict(numerator='311385129',denominator='536870912'),center_y=dict(numerator='1',denominator='2'),scale=dict(numerator='11',denominator='10')) and f['scale_transition']==dict(factor=dict(numerator='2',denominator='1'),curve='linear') and all(not n.get('framing') for i,n in doc['nodes'].items() if i!=owner),'one owner-output Follow, both saved bases and separate outer animation')
    target=doc['targets']['target-1'];need(len(doc['targets'])==1 and target['region']==dict(center=[580000,500000]) and target['corrections']==[dict(at=80080,region=dict(center=[680000,500000]))] and not target.get('samples',[]),'actual target before/after Original80')
    clock=dict(numerator=1,denominator=30000)
    need(target['span']==dict(start=dict(ticks=0,time_base=clock),end=dict(ticks=120120,time_base=clock)) and target['display_aspect']==dict(numerator='16',denominator='9'),'target Original span and aspect')
    def original_bounds(i,start,end):
        n=doc['nodes'][i]['kind']
        if n['type']=='retime':
            need(n['purpose']=='partition' and n['pitch']=='preserve' and F(n['mapping']['end']-n['mapping']['start'],n['duration'])==1,'unity partition only below Retime')
            return original_bounds(n['child'],n['mapping']['start']+start,n['mapping']['start']+end)
        need(n['type']=='source','actual leaf Source');source=n['source'];v=source['video']
        need(v['type']=='stream' and v['asset']==target['asset'] and v['span']['start']['time_base']==v['span']['end']['time_base']==clock,'measured CFR source clock/identity')
        m=source['video_mapping']
        if m['type']=='fit_beat':placement,frames=F(0),F(source['duration'])
        else:
            need(m['type'] in ('duration','placement','selected_placement'),'source mapping wire')
            placement=ratio(m['start']) if 'start' in m else F(0);frames=ratio(m['frames'])
        origin=F(v['span']['start']['ticks'],1001);length=F(v['span']['end']['ticks']-v['span']['start']['ticks'],1001)
        return origin+(start-placement)*length/frames,origin+(end-placement)*length/frames
    ranges=[original_bounds(x,F(0),F(40)) for x in group['children']]
    need(ranges==[(F(0),F(40)),(F(80),F(120))],'actual Original[0,40),[80,120) ranges')
    v=obj(folder+'/verification.json')
    need(v==obj(folder+'/verify.stdout') and v['passed'] is True and v['failures']==[] and v['revision_id']==doc['revision_id'] and v['range']==[0,120] and v['frame_rate']==[30000,1001],'actual full movie verification')
    need(len(v['pictures'])==120 and all(p['passed'] is True and p['flags']==[] for p in v['pictures']) and [p['output_frame'] for p in v['pictures']]==list(range(120)),'complete120 picture rows')
    fractional=0;cut=[]
    for j,p in enumerate(v['pictures']):
        q=F(2*j+1,3);o=q+(40 if j>=60 else 0);ordinal=o.numerator//o.denominator
        fractional+=o.denominator!=1
        need(p['project_frame']==j and p['provenance']==dict(kind='original',asset=target['asset'],source_frame=ordinal,source_pts=ordinal*1001),'all120 exact source ordinal/PTS mappings')
        if j in (59,60):cut.append(dict(edit=j,exact_original=str(o),decoded_ordinal=ordinal))
    need(fractional==80 and [x['decoded_ordinal'] for x in cut]==[39,80],'nonunit80 fractional coordinates and exact mapped cut')
    obs=collections.Counter(p[x]['status'] for p in v['pictures'] for x in ('previous_index','next_index') if p[x] is not None)
    need(sum(obs.values())==238 and obs['observable']==v['summary']['index_observable_comparisons'] and obs['unobservable']==v['summary']['index_unobservable_comparisons'],'actual neighbor observability limits')
    a=v['audio'];need(len(a)==1 and a[0]['passed'] is True and a[0]['flags']==[] and a[0]['window']==a[0]['project_samples']==[0,192192] and a[0]['uncovered_samples']==0 and a[0]['offset_status']=='verified_zero' and a[0]['measured_offset_samples']==0 and all(x['status']=='verified_zero' and x['peak_lag']==0 for x in a[0]['segments']),'full192192 samples with verified zero offset')
    movie=meta(S/folder/'retime-follow-creep.mp4');m=v['movie']
    need(movie==dict(bytes=172802,sha256='6fac1365b2ce192eebe16dccf01883af4041e9d4c1d238d5dc09d689afb3f4f1') and m['audio']['physical_samples']==194560,'frozen actual native movie and coded sample count')
    need(m['bytes']==movie['bytes'] and m['video_frames']==m['expected_video_frames']==120 and m['picture_anomalies']==[] and m['audio']['gaps']==[] and m['audio']['presented_end']==m['expected_audio_samples']==192192 and m['audio']['first_pts']==-2048 and m['audio']['physical_samples']>=192192+2048 and m['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=192192) and m['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=120120),'actual movie/AAC edit delay/timing')
    pubs=[d for line in read(folder+'/render.stdout').splitlines() for d in dictionaries(json.loads(line)) if 'movie_sha256' in d]
    need(pubs and all(x==pubs[0] for x in pubs) and pubs[0]['movie_sha256']==movie['sha256'] and pubs[0]['movie_bytes']==movie['bytes'],'actual render publication identity')
    pub=pubs[0];pinread(S/folder/Path(pub['report']).name,dict(bytes=pub['report_bytes'],sha256=pub['report_sha256']))
    histories=[rows(Path(commands[0]['command'][3])),rows(S/folder/'portable.deadpan')]
    need(histories[0]==histories[1],'all immutable Native/portable history tables equal')
    count=len(histories[0]['revisions']);copyreceipt=obj(folder+'/copy.stdout')['portable_copy']
    need(copyreceipt['verified_history']['revisions']==count and copyreceipt['verified_history']['replayed']+copyreceipt['verified_history']['verified_by_receipt']==count,'portable complete history receipt matches actual immutable rows')
    summary=obj(folder+'/result.json')
    need(summary==json.loads(read('media-final.stdout')) and summary['commands']==9 and summary['expected_frames']==120 and summary['expected_audio_samples']==192192 and summary['portable_document_equal'] is True and summary['movie_sha256']==movie['sha256'] and summary['movie_bytes']==movie['bytes'] and summary['fixture_origin']=='native replay project; this driver constructs no document' and summary['derived_fixture'] is False and summary['asserted_original_ranges']==[['0','40'],['80','120']],'actual native media producer/raw summary counterpart')
    return dict(actual_commands=9,authored_owner=owner,root_duration=120,input_duration=80,input_speed='2/3',pitch_wire='follow_speed',actual_original_ranges=[['0','40'],['80','120']],fractional_provider_coordinates=80,cut=cut,movie=movie,picture_rows=120,neighbor_observability=dict(obs),audio_window=[0,192192],audio_offset=0,movie_metadata=m,portable_document_exact_equal=True,all_selected_history_tables_equal=True,history_counts={k:len(x) for k,x in histories[0].items()},portable_history_receipt=copyreceipt['verified_history'])
def main():
    out=dict(schema_version=1,status='running',findings=[],base='aa80624cbafda91a1c9d6f8d13b749586133be41',started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    before={}
    try:
        final,first=obj('final-identities.json'),obj('first-identities.json');paths=obj('source-files.json');freeze=obj('source-review-freeze.json')
        need(len(paths)==len(set(paths))==17 and set(paths)==set(final['sources']) and freeze['source_paths']==17 and freeze['base']==out['base'],'actual final17 scope/base')
        before={p:meta(R/p) for p in paths};need(before==final['sources']==first['sources']==freeze['sources'],'current/first/final/review17 exact')
        for phase,ids in [('first',first),('final',final)]:
            for p,m in ids['sources'].items():pinread(S/('source-'+phase)/(p+'.txt'),m)
            for n,m in ids['binaries'].items():need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'actual retained candidate binary')
        need(set(first['binaries'])==set(final['binaries']) and len(final['binaries'])==5 and all({k:first['binaries'][n][k] for k in ('bytes','sha256')}=={k:final['binaries'][n][k] for k in ('bytes','sha256')} for n in final['binaries']),'five first/final sibling identities equal')
        eq=obj('candidate-equivalence.json');need(eq['source_paths']==17 and eq['all_source_bytes_identical'] is True and eq['all_five_binary_bytes_identical'] is True,'one candidate equivalence')
        need(final['binaries']['deadpan-app']['sha256']=='fb540703bc8d21c223d45855f5a2c54a8efd74601df25fe6a482c29e23c6c172' and final['binaries']['deadpan-cli']['sha256']=='b1b5b6b0ecc4fb729cd9996d5b7ed823d0af9947549527c38679997d5c50eb08','actual final App/CLI')
        for n in ('run.py','checks.py','worker-run.py','plan-run.py','media.py'):read(n)
        need(meta(S/'media.py')['sha256']=='e46dccbfaaec39a34fa97b1a326c40211d535dcf41bfa0e6308e28af054286cc','prepared media driver actual bytes')
        runs={}
        for n in ('format-final','app-check-first','clippy-first','core-plan-final','app-units-final','release-final','native-retime-first','native-sequence-follow-creep-final','native-repeat-follow-creep-final','native-zoom-final','media-final'):
            runs[n]=receipt(n);need(runs[n]['source_before']=={str(R/p):m for p,m in final['sources'].items()},'current17 actual standard receipt:'+n)
        runs['commands-write']=receipt('commands-write',101)
        need(runs['commands-write']['source_before']=={str(R/p):m for p,m in final['sources'].items()},'intentional reference generation17 source guards')
        need(runs['format-final']['command'][:7]==['rustfmt','--check','--edition','2024','--config','skip_children=true',paths[0]] and runs['format-final']['command'][6:]==paths,'actual scoped format invocation')
        need('--all-targets' in runs['clippy-first']['command'] and runs['clippy-first']['command'][-3:]==['--','-D','warnings'],'strict actual all-targets invocation')
        need(read('format-final.stdout')==read('format-final.stderr')==b'' and 'Finished' in read('clippy-first.stderr').decode(),'successful format/strict terminal streams')
        testresults={}
        for n,count,ignored in [('core-plan-final',2056,0),('app-units-final',1304,11)]:
            t=tests(n);need((t['passed'],t['failed'],t['ignored'])==(count,0,ignored),'actual broad named test totals');testresults[n]=t
        focused_results=focused(final['sources'],testresults['core-plan-final']['passed_names'])
        live=obj('app-unit-executable-live.json');terminal=obj('app-unit-executable-terminal.json')
        need(terminal['observed_live']==live and terminal['unchanged'] is True and terminal['terminal']=={k:live[k] for k in ('bytes','sha256')} and meta(live['path'])==terminal['terminal'] and live['sha256']=='ae505efaf475bdbc5b43c3c8616deb3b8400dc5157aef02fe4f4ec3a43fea29c','actual live/terminal unit executable byte equality')
        need(datetime.datetime.fromisoformat(runs['app-units-final']['started_at'])<datetime.datetime.fromisoformat(live['observed_at'])<datetime.datetime.fromisoformat(runs['app-units-final']['finished_at']) and Path(live['path']).name in read('app-units-final.stderr').decode() and 'Running unittests src/main.rs' in read('app-units-final.stderr').decode(),'observed running test executable path and timing window')
        wf=obj('worker-retime-final-freeze.json');need(wf['base']==out['base'] and wf['failed_attempts']==[] and len(wf['files'])==8 and all(final['sources'][p]['sha256']==h for p,h in wf['files'].items()),'focused worker final owned freeze/no failed lane')
        plan_source=read(R/'crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs')
        need(b'fn fast() -> [Expected; 5]' in plan_source and b'fn slow() -> [Expected; 12]' in plan_source and plan_source.count(b'for point in [false, true]')==3 and b'for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep, ZoomCurve::Step]' in plan_source and b'for frame in 0..frames + 4' in plan_source and b'const OWNER_START: i64 = 2;' in plan_source,'static10 cases/78 owner/40 neighbor source basis')
        nh=obj('native-harness-freeze.json');need(nh['base_supplied_by_root']==out['base'] and nh['source_frozen'] is True and nh['final_output_frames']==120 and nh['canonical_48k_audio_samples']==192192,'native harness frozen contract')
        for p in nh['changed_paths']:pinread(p['source_copy'],{k:p[k] for k in ('bytes','sha256')});need(final['sources'][p['path']]=={k:p[k] for k in ('bytes','sha256')},'native helper final bytes')
        native=[]
        for n,scenario,count,warnings in [('native-retime-first','retime-follow-creep',129,3),('native-sequence-follow-creep-final','sequence-follow-creep',101,3),('native-repeat-follow-creep-final','repeat-follow-creep',129,3),('native-zoom-final','zoom',213,3)]:
            d=obj(n+'/report.json');md=d['metadata']
            need(d['failed'] is False and md['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and md['build_profile']=='release' and md['physical_display_measured'] is False and md['gpu_readback_in_timing_run'] is False,'native actual terminal/binary/limits')
            scenes={x['name']:x for x in d['scenarios']}
            need(set(scenes)=={scenario,'kestrel-shortcuts'} and len(scenes[scenario]['checks'])==count and all(c['passed'] is True for x in scenes.values() for c in x['checks']) and not any(f['severity']=='failure' for x in scenes.values() for f in x['findings']),'native raw exact checks')
            k=scenes['kestrel-shortcuts']['checks'];need(len(k)==1 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['conflicts']==[],'Kestrel actual production router compatibility')
            argv=runs[n]['command'];need(meta(argv[0])['sha256']==md['binary_sha256'] and meta(argv[argv.index('--kestrel-source')+1])['sha256']==k[0]['actual']['source_sha256']==k[0]['actual']['live_source_sha256'],'invoked native App/local Kestrel source')
            findings=scenes[scenario]['findings'];need(sum(f['severity']=='warning' for f in findings)==warnings,'actual warning count')
            native.append(dict(name=n,scenario=scenario,checks=count,kestrel_checks=1,findings=findings,skipped=scenes[scenario]['skipped']))
        mm=media(final)
        visual=obj('visual-review.json');need(len(visual['images'])==4,'Root four image scope')
        images=[png(p,m) for p,m in visual['images'].items()]
        review=obj('review-notes.json');need(review['base']==out['base'],'retained review basis')
        host=obj('host.json');need(host['hw.model']=='Mac17,7' and host['hw.memsize']=='137438953472' and host['hw.ncpu']=='18' and host['kern.hv_vmm_present']=='0','fresh host metadata')
        diagnostics={}
        for p in sorted(S.iterdir()):
            if p.suffix in ('.stdout','.stderr') and not p.name.startswith('audit-retime'):
                ls=[x for x in read(p).decode(errors='replace').splitlines() if x.startswith(('error:','error[','warning:','warning[','test result: FAILED'))]
                if ls:diagnostics[p.name]=ls
        out.update(status='no_findings',findings=[],final_source_count=17,final_sources=final['sources'],final_binaries=final['binaries'],candidate_equivalence=eq,source_review_freeze_sha256=meta(S/'source-review-freeze.json')['sha256'],tests={n:{k:v for k,v in t.items() if k!='passed_names'} for n,t in testresults.items()},focused_scopes=focused_results,plan_static_grid=dict(cases=10,owner_frames=6*5+4*12,neighbor_frames=10*4),app_unit_live_terminal=terminal,native_reports=native,media=mm,root_image_identities=images,root_visual_review=visual,host=host,retained_historical_diagnostics=diagnostics)
    except Exception as e:
        traceback.print_exc();out['status']='findings';out['findings'].append(dict(type=type(e).__name__,message=str(e)))
    finally:
        out['current_scoped_sources_unchanged']=before=={p:meta(R/p) for p in before}
        if not out['current_scoped_sources_unchanged']:out['status']='findings';out['findings'].append(dict(message='scoped source changed during audit'))
        out['limitations']=['Offline read-only audit only; no product/Cargo/test/worker/decoder/model/collector/Git commands executed.','Seventeen final paths and relevant focused scopes are witnesses, not compilation of every repository source.','Core focused own8/named9 remained stable; its global17 drift was only a concurrently edited Plan integration test. The false global guard is retained and not relabeled unchanged.','App unit binary was observed while the actual Cargo path was running and matched terminal bytes; test binary bytes are not archived. Focused Cargo executable pins are historical observations.','Native input and offscreen Metal do not qualify physical display/input, OS picker/IME, VoiceOver, listening or latency. Root four PNG identities are verified without independent visual judgement.','Emitted picture/audio comparison uses shared committed picture/limited-DSP semantics, not independent DSP/listening or encoded PCM bit equality. All picture-neighbor unobservability, warnings/skips and eleven existing App opt-in ignores remain explicit.','One release candidate with first/final byte equality; no duplicate Retime workflow run claimed. No separate clipped-media lane is inferred.']
        out['preserved_first_audit_attempt']=dict(path='audit-history/first-global-missing-schema-stop',cause='Auditor schema mismatch: actual missing-path fields are empty lists, not numeric zero.',product_failure=False,preservation_manifest=meta(S/'audit-history/first-global-missing-schema-stop/preservation.json'))
        out['input_file_pins']=PINS;out['verifier_sha256']=meta(Path(__file__))['sha256'];out['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (S/'audit-retime-evidence.json').open('x') as f:json.dump(out,f,indent=2,sort_keys=True);f.write('\n')
        print(json.dumps({k:out.get(k) for k in ('status','findings','final_source_count','current_scoped_sources_unchanged')},sort_keys=True))
    return 0 if out['status']=='no_findings' else 1
if __name__=='__main__':sys.exit(main())
