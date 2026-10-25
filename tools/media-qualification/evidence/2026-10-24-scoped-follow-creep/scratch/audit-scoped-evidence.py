#!/usr/bin/env python3
"""Independent bounded offline audit of scoped Follow creep qualification receipts."""
import collections, copy, datetime, difflib, hashlib, json, re, sqlite3, stat, struct, sys, traceback, zlib
from fractions import Fraction as F
from pathlib import Path
S=Path('/tmp/deadpan-scoped-follow-creep-2026-10-24')
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
BASE='fb38159bae82f2c08e7dfd96f112c24d6571b8e2'
# Preparation only. Native/media counts and immutable history row counts are
# populated from actual terminal receipts, never inferred from the design.
AUDIT_CONTRACT={
    'listed_final_sources':22,
    'worker_owned':12,
    'worker_named_dependency_basis':9,
    'plan_owned':2,
    'plan_core_plan_basis':287,
    'proposed_total_frames':240,
    'proposed_selected_range':[164,200],
    'proposed_full_48k_audio_samples':384384,
    'scope':'outer Play2 plus inner Play2; selected complete36-frame Retime',
    'historical_expected_tests':{
        'worker-core-1':[3,0,0],
        'worker-core-scoped-1':[59,0,0],
        'worker-cli-1':[11,5,0],
        'worker-cli-2':[15,1,0],
        'worker-cli-3':[16,0,0],
        'worker-cli-unit-1':[1,0,0],
        'plan-focused-1':[2,0,0],
        'plan-focused-2':[2,0,0],
    },
    'historical_expected_exits':{
        'worker-cli-1':101,'worker-cli-2':101,'clippy-first':101,
        'commands-write':101,
    },
    'proof_limits':[
        '12owned/9named/22listed maps are not a whole-product source snapshot.',
        'Three Core filter hits overlap59 scoped hits: count61 unique cases, not62.',
        'Original and rerun Plan each passed2; do not pool as4 distinct tests.',
        'Physical copy-on-write aliases must be checked against actual isolation/override records, not forbidden or stripped as arbitrary identity noise.',
        'All240 pictures, full384384 samples,240-frame native oracle and actual SQL history remain pending until terminal evidence exists.',
        'Source and scope maps, binary identities, movie sizes, AAC clocks, observable neighbors, warnings/skips and SQL counts must come from actual outputs.',
    ],
}
def source_pin_map():
    paths=obj('source-files.json')
    need(len(paths)==len(set(paths))==22,'actual source22 manifest')
    return {p:meta(R/p) for p in paths}
def focused_workers(final):
    result={}
    for n,expected in AUDIT_CONTRACT['historical_expected_tests'].items():
        if n.startswith('plan-'):continue
        d=obj(n+'.json');exit_code=AUDIT_CONTRACT['historical_expected_exits'].get(n,0)
        need(d['exit_code']==exit_code and 'finished_at' in d and d['cwd']==str(R),'worker terminal:'+n)
        need(d['owned_before']==d['owned_after'] and d['owned_unchanged'] is True and len(d['owned_before'])==12,'actual stable owned12:'+n)
        need(d['basis_before']==d['basis_after'] and d['basis_unchanged'] is True and len(d['basis_before'])==9,'actual stable named9:'+n)
        need(d['global_source_before']==d['global_source_after'] and d['global_source_unchanged'] is True and len(d['global_source_before'])==22,'actual stable listed22 during run:'+n)
        need(d['global_complete_before'] is True and d['global_complete_after'] is True and d['global_missing_before']==d['global_missing_after']==[],'all listed22 existed:'+n)
        for p,h in d['owned_before'].items():
            snap=Path(d['source_copies'])/(p+'.txt')
            need(meta(snap)['sha256']==h,'exact historical attempted-source snapshot:'+n)
            read(snap)
        for suffix,h in d['stream_pins'].items():need(meta(S/(n+suffix))['sha256']==h,'exact worker raw stream:'+n)
        t=tests(n);need([t[k] for k in ('passed','failed','ignored')]==expected,'historical test counts:'+n)
        result[n]={
            'actual_tests':t,'terminal_exit':exit_code,
            'owned_source_paths':12,'named_dependency_paths':9,'global_listed_paths':22,
            'owned_current_differences':[p for p,h in d['owned_before'].items() if final[p]['sha256']!=h],
            'global_current_differences':[p for p,h in d['global_source_before'].items() if final[p]['sha256']!=h],
            'executed_binary_observations':d['executed_binary_pins'],
        }
    a=set(result['worker-core-1']['actual_tests']['passed_names'])
    b=set(result['worker-core-scoped-1']['actual_tests']['passed_names'])
    need(len(a|b)==61 and len(a&b)==1,'actual unique Core cases and overlap')
    # Later concrete assertions bind each recorded difference to exact retained
    # test-only source bytes after final Root source freeze is supplied.
    return result
def focused_plan(final):
    records={}
    for n in ('plan-focused-1','plan-focused-2'):
        d=obj(n+'.json')
        need(d['exit_code']==0 and 'finished_at' in d and d['cwd']==str(R),'Plan terminal')
        need(d['source_before']==d['source_after'] and d['owned_sources_unchanged'] is True and len(d['source_before'])==2,'Plan stable own2')
        need(d['core_plan_basis_before']==d['core_plan_basis_after'] and d['core_plan_basis_unchanged'] is True and len(d['core_plan_basis_before'])==287,'Plan stable Core/Plan287')
        need(d['manifest_before']==d['manifest_after'] and d['manifest_unchanged'] is True,'Plan actual listed manifest')
        observed=d['manifest_before']
        need(observed['missing']==[] and len(observed['present'])==22,'Plan listed22 present, no missing')
        for p,m in d['streams'].items():pinread(p,m)
        need(d['runner_sha256']==meta(S/'plan-run.py')['sha256'],'Plan runner identity')
        for p,m in {**d['source_before'],**d['core_plan_basis_before'],**observed['present']}.items():
            pinread(S/(n+'-source')/(p+'.txt'),m)
        t=tests(n);need([t[k] for k in ('passed','failed','ignored')]==[2,0,0] and t['filtered']==[136],'actual Plan2 scoped cases')
        records[n]={'actual_tests':t,'owned_paths':2,'core_plan_basis_paths':287,'global_listed_paths':22,'current_owned_differences':[p for p,m in d['source_before'].items() if final[p]!=m],'executed_binary_observation':d['executed_binary']}
    return records

def ratio(v):return F(int(v['numerator']),int(v['denominator']))
def even(v):
    a,r=divmod(v.numerator,v.denominator)
    return a+int(2*r>v.denominator or (2*r==v.denominator and a%2!=0))
def q32(v):return F(even(v*(1<<32)),1<<32)
def scale32(j):
    first=even(F(11,10)*(1<<32));last=even(F(11,5)*(1<<32))
    progress=even(F(2*j+1,72)*(1<<32))
    return F(first+even(F((last-first)*progress,1<<32)),1<<32)
def canonical(v):return json.dumps(v,sort_keys=True,separators=(',',':'))
def media(final,scoped):
    folder='media-third';r=obj(folder+'/result.json');cmd=obj(folder+'/commands.json')
    need(len(cmd)==9 and [x['name'] for x in cmd]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'],'actual nine CLI sequence')
    for x in cmd:
        need(x['exit_code']==0 and x['seconds']>=0 and meta(x['argv'][0])['sha256']==final['binaries']['deadpan-cli']['sha256'],'actual CLI command terminal/binary')
        for ext in ('stdout','stderr'):read(folder+'/'+x['name']+'.'+ext)
    need(obj(folder+'/native-report.json')==scoped,'media retained complete native report')
    doc=obj(folder+'/dump.stdout');need(doc==obj(folder+'/portable-dump.stdout'),'exact portable authored document')
    need(doc['revision_id']==r['revision'],'media immutable revision')
    nodes=doc['nodes'];root=nodes[doc['root']]['kind'];need(root['type']=='sequence' and len(root['children'])==1,'one outer root')
    outer=root['children'][0];ok=nodes[outer]['kind'];inner=ok['child'];ik=nodes[inner]['kind'];template=ik['child']
    need(ok['type']==ik['type']=='repeat','actual nested Repeat')
    rt=nodes[template]['kind'];need(rt['type']=='retime' and rt['duration']==36 and rt['mapping']=={'start':0,'end':24} and rt['pitch']=='follow_speed','ordinary36/24 Retime')
    def it(k):
        return [dict(allocation=x['allocation'],ordinal=j) for x in k['iterations']['runs'] for j in range(x['first'],x['first']+x['count'])]
    oi,ii=it(ok),it(ik);need(len(oi)==2 and len(ii)==3,'stable2x3 iterations')
    ov=doc['overrides'];need(len(ov)==2 and len(ov[outer])==1 and ov[outer][0]['iteration']==oi[1],'outer second branch only')
    ci=ov[outer][0]['root'];need(set(ov)=={outer,ci} and len(ov[ci])==1 and ov[ci][0]['iteration']==ii[1],'inner second branch only')
    selected=ov[ci][0]['root']
    def children(n):
        k=nodes[n]['kind']
        return k['children'] if k['type']=='sequence' else [k['child']] if k['type'] in ('retime','repeat') else []
    aliases={}
    def pair(a,b):
        need(a!=b and a not in (outer,inner,template,doc['root']),'fresh actual clone')
        if a in aliases:need(aliases[a]==b,'consistent alias');return
        aliases[a]=b
        need(nodes[a]['kind']['type']==nodes[b]['kind']['type'] and len(children(a))==len(children(b)),'clone same structure')
        for aa,bb in zip(children(a),children(b)):pair(aa,bb)
    pair(ci,inner);pair(selected,template)
    need(len(aliases)==7 and r['clone_aliases']==aliases,'all seven concrete clone bindings')
    def norm(v):
        if isinstance(v,str):return aliases.get(v,v)
        if isinstance(v,list):return [norm(x) for x in v]
        if isinstance(v,dict):return {k:norm(x) for k,x in v.items()}
        return v
    fixed=nodes[template]['framing'];f=fixed['value']
    need('clock' not in fixed and f['type']=='follow' and f['target']=='target-1' and ratio(f['scale'])==F(11,10) and 'scale_transition' not in f,'fixed live Follow retained')
    need([ratio(f['fallback'][k]) for k in ('center_x','center_y','scale')]==[F(1,2),F(1,2),F(11,10)],'independent centered fallback')
    animated=copy.deepcopy(fixed);animated['value']['scale_transition']={'factor':{'numerator':'2','denominator':'1'},'curve':'linear'}
    need(nodes[selected]['framing']==animated,'selected owner transition')
    for a,b in aliases.items():
        expected=copy.deepcopy(nodes[b])
        if a==selected:expected['framing']=animated
        need(norm(nodes[a])==expected,'entire clone fields except explicitly selected framing')
    need(sum(bool(n.get('framing',{}).get('value',{}).get('scale_transition')) for n in nodes.values())==1,'one animated concrete owner')
    target=doc['targets']['target-1'];clock={'numerator':1,'denominator':30000}
    need(target['region']=={'center':[580000,500000]} and target['corrections']==[{'at':12012,'region':{'center':[680000,500000]}}] and not target.get('samples'),'actual saved point/correction')
    def duration(n):
        k=nodes[n]['kind']
        if k['type']=='source':return k['source']['duration']
        if k['type']=='retime':return k['duration']
        if k['type']=='sequence':return sum(map(duration,k['children']))
        if k['type']=='repeat':return len(it(k))*duration(k['child'])+(len(it(k))-1)*k.get('gap',{}).get('duration',0)
        raise AssertionError('unexpected node')
    need(duration(inner)==116 and duration(doc['root'])==240,'actual nested total duration')
    for k,length in ((ok,8),(ik,4),(nodes[ci]['kind'],4)):
        g=k['gap'];need(g['duration']==length and g['audio']=={'type':'silence'} and g['video']=={'type':'freeze','asset':target['asset'],'timestamp':{'ticks':23023,'time_base':clock}},'retained captured gap')
        need(g['picture_context']['canvases'],'captured gap canvas')
        for c in g['picture_context']['canvases']:
            for pose in c['layers']:
                if pose is not None:need([ratio(pose[z]) for z in ('center_x','center_y','scale')]==[q32(F(68,100)),F(1,2),q32(F(11,10))],'captured gap pose')
    def source(n,t):
        k=nodes[n]['kind']
        if k['type']=='retime':
            need(k.get('purpose')=='partition' and k['pitch']=='preserve' and k['mapping']['end']-k['mapping']['start']==k['duration'],'partition provider')
            return source(k['child'],t+k['mapping']['start'])
        if k['type']=='sequence':
            for child in k['children']:
                if t<duration(child):return source(child,t)
                t-=duration(child)
            raise AssertionError('provider out of range')
        need(k['type']=='source','leaf provider')
        v=k['source'];video=v['video'];m=v['video_mapping'];need(video['type']=='stream' and video['asset']==target['asset'] and video['span']['start']['time_base']==video['span']['end']['time_base']==clock,'source measured clock')
        placement=ratio(m['start']) if 'start' in m else F(0);frames=F(v['duration']) if m['type']=='fit_beat' else ratio(m['frames'])
        return n,video['span']['start']['ticks']+(t-placement)*F(video['span']['end']['ticks']-video['span']['start']['ticks'],frames)
    checks=scoped['scenarios'][1]['checks'] if scoped['scenarios'][1]['name']=='scoped-follow-creep' else next(x for x in scoped['scenarios'] if x['name']=='scoped-follow-creep')['checks']
    def check(name):return next(c['actual'] for c in reversed(checks) if c['name']==name)
    proof=check('The final export has exactly one animated concrete branch')
    need(proof['aliases']==aliases and proof['altered_frames']==36 and all(proof[k] is True for k in ('override_paths_exact','originals_unchanged','clones_exact','frames_exact','audio_exact')),'native full scope/isolation/audio checks')
    native=check('All 240 frames retain exact providers and captured gaps while only the selected 36-frame owner creeps')
    need(native['exact'] is True and len(native['frames'])==240 and native['fractional']==144,'native complete oracle rows')
    oracle=obj(folder+'/independent-native-oracle.json');need(len(oracle)==240,'retained driver oracle rows')
    v=obj(folder+'/verification.json');need(v==obj(folder+'/verify.stdout') and v['passed'] is True and v['failures']==[] and v['revision_id']==doc['revision_id'] and v['range']==[0,240] and v['frame_rate']==[30000,1001] and v['canvas']==v['raster']==[320,180],'actual all-frame verification')
    need(len(v['pictures'])==240,'240 decoded picture comparisons')
    gaps=fractional=selected_count=0
    for frame,(row,p,oo) in enumerate(zip(native['frames'],v['pictures'],oracle)):
        outerplay,local=divmod(frame,124);innerplay,j=divmod(local,40);outergap=local>=116;gap=outergap or j>=36
        ticks=F(23023) if gap else F((2*j+1)*1001,3);ordinal=int(ticks//1001);sel=164<=frame<200
        gaps+=gap;fractional+=not gap and ticks.denominator!=1;selected_count+=sel
        actualinner=ci if outerplay else inner;owner=selected if sel else nodes[actualinner]['kind']['child']
        repeats=[{'node':outer,'iteration':oi[outerplay]}]
        if gap:
            provider=outer if outergap else actualinner
            if outergap:repeats=[]
            need(row['owner_pose'] is None and row['gap_after']==(oi[outerplay] if outergap else ii[innerplay]),'gap metadata exact')
        else:
            repeats.append({'node':actualinner,'iteration':ii[innerplay]})
            provider,actualticks=source(nodes[owner]['kind']['child'],F(2*j+1,3));need(actualticks==ticks,'exact provider fraction')
            scale=scale32(j) if sel else q32(F(11,10))
            need([ratio(row['owner_pose'][k]) for k in ('center_x','center_y','scale')]==[q32(F(58 if ordinal<12 else 68,100)),F(1,2),scale],'all native live owner centers/scales')
        need(row['Edit']==frame and row['gap']==gap and row['Original_ordinal']==ordinal and ratio(row['expected_point']['ticks'])==ticks and row['expected_point']['time_base']==clock and row['provider']=={'node':provider,'repeats':repeats},'all240 exact native provider/Play/gap rows')
        need(oo==dict(Edit=frame,gap=gap,Original_ordinal=ordinal,ticks=str(ticks),provider=provider,selected=sel,owner_pose=row['owner_pose']),'driver oracle independently recomputed')
        need(p['passed'] is True and p['flags']==[] and p['output_frame']==p['project_frame']==frame and p['provenance']==dict(kind='original',asset=target['asset'],source_frame=ordinal,source_pts=ordinal*1001),'actual all240 exported provenance')
    need((gaps,fractional,selected_count)==(24,144,36),'actual gap/fractional/scoped counts')
    obs=collections.Counter(p[x]['status'] for p in v['pictures'] for x in ('previous_index','next_index') if p[x] is not None)
    need(dict(obs)=={'observable':262,'unobservable':216} and v['summary']['index_observable_comparisons']==262 and v['summary']['index_unobservable_comparisons']==216,'all neighbor observability')
    a=v['audio'];need(len(a)==1 and a[0]['window']==a[0]['project_samples']==[0,384384] and a[0]['passed'] is True and a[0]['flags']==[] and a[0]['uncovered_samples']==0 and a[0]['offset_status']=='verified_zero' and a[0]['measured_offset_samples']==0 and all(x['status']=='verified_zero' and x['peak_lag']==0 for x in a[0]['segments']),'complete signal audio/zero offset')
    movie=meta(S/folder/'scoped-follow-creep.mp4');need(movie==dict(bytes=164670,sha256='a3cce65faf3ab682ab5184fa20d0deb4f148ca5589f3b293810fe0f9eabe315d'),'actual movie identity');read(folder+'/scoped-follow-creep.mp4')
    m=v['movie'];need(m['bytes']==movie['bytes'] and m['video_frames']==m['expected_video_frames']==240 and m['picture_anomalies']==[] and m['audio']==dict(first_pts=-2048,physical_samples=387072,presented_end=384384,gaps=[]) and m['expected_audio_samples']==384384 and m['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=384384) and m['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=240240),'actual AAC/video timing')
    pubs=[x for line in read(folder+'/render.stdout').splitlines() for x in dictionaries(json.loads(line)) if 'movie_sha256' in x]
    need(pubs and all(x==pubs[0] for x in pubs) and pubs[0]['movie_sha256']==movie['sha256'],'render publication actual bytes')
    pinread(S/folder/Path(pubs[0]['report']).name,dict(bytes=pubs[0]['report_bytes'],sha256=pubs[0]['report_sha256']))
    nativepath=Path(cmd[0]['argv'][3]);portable=S/folder/'portable.deadpan';h=rows(nativepath);hp=rows(portable)
    need(h==hp,'complete immutable seven history tables exact')
    need(len(h['revisions'])==25 and len(h['history'])==15,'actual25/15 history')
    cr=obj(folder+'/copy.stdout')['portable_copy']['verified_history'];need(cr['revisions']==25 and cr['replayed']+cr['verified_by_receipt']==25,'portable full revision verification receipt')
    with sqlite3.connect((nativepath/'project.sqlite').as_uri()+'?mode=ro&immutable=1',uri=True) as db:
        kinds=dict(db.execute('select kind,count(*) from revisions group by kind').fetchall())
        vals={k:db.execute(sql).fetchall() for k,sql in {'revisions':'select id,parent_id,kind from revisions order by id','history':'select id,parent_id,revision_id,request,edit from history order by id','redo':'select position,history_id from redo order by position','state':'select head_revision,cursor,workflow from state order by singleton'}.items()}
    digest=hashlib.sha256(canonical(vals).encode()).hexdigest()
    need(kinds==dict(initial=1,edit=15,undo=7,redo=2) and digest==r['history_before_after_and_portable']['sha256']=='4dae063c100a608771634cfebcf9c9da1da1ae78a471ae3465c8ab0451087945','complete retained history digest and actual Undo/Redo')
    need(r==json.loads(read('media-third.stdout')) and r['commands']==9 and r['frames']==240 and r['audio_samples']==384384 and r['portable_document_equal'] is True and r['derived_fixture'] is False and r['movie_sha256']==movie['sha256'],'actual driver terminal summary')
    return dict(commands=9,frames=240,live_frames=216,gap_frames=24,selected_range=[164,200],fractional_provider_frames=144,clone_aliases=aliases,neighbor_observability=dict(obs),audio_samples=384384,audio_offset=0,movie=movie,movie_metadata=m,history_counts={k:len(v) for k,v in h.items()},revision_kinds=kinds,history_digest=digest,portable_history_receipt=cr,complete_immutable_history_equal=True)

def main():
    out=dict(schema_version=1,status='running',findings=[],base=BASE,started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());before={}
    try:
        final,first=obj('final-identities.json'),obj('first-identities.json');before=source_pin_map()
        need(before==final['sources']==first['sources'],'all22 current/first/final exact')
        for phase,ids in [('first',first),('final',final)]:
            for p,m in ids['sources'].items():pinread(S/('source-'+phase)/(p+'.txt'),m)
            need(len(ids['binaries'])==5,'five candidate sibling binaries')
            for n,m in ids['binaries'].items():need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'actual retained executable')
        need({n:{k:m[k] for k in ('bytes','sha256')} for n,m in first['binaries'].items()}=={n:{k:m[k] for k in ('bytes','sha256')} for n,m in final['binaries'].items()},'all five first/final identical')
        eq=obj('candidate-equivalence.json');need(eq['source_paths']==22 and eq['source_bytes_equal'] is True and eq['binary_bytes_equal'] is True,'one actual release candidate')
        runs={}
        labels=['format-final','app-check-first','format-after-lint','app-check-after-lint','clippy-after-lint','core-plan-final','app-units-final','release-first','native-scoped-first','native-repeat-follow-creep-final','native-retime-follow-creep-final','native-zoom-final','native-sequence-follow-creep-final','native-hold-follow-creep-final','media-third']
        for n in labels:runs[n]=receipt(n)
        for n,code in [('clippy-first',101),('commands-write',101),('media-first',1),('media-second',1)]:runs[n]=receipt(n,code)
        ab={str(R/p):m for p,m in before.items()}
        for n,d in runs.items():
            delta=[p for p,m in d['source_before'].items() if ab.get(p)!=m]
            need(delta==([] if n not in ('format-final','app-check-first','clippy-first') else [str(R/'crates/deadpan-plan/tests/picture_plan/scoped_follow_creep.rs')]),'exact actual historical source scope:'+n)
        need('collapsible_if' in read('clippy-first.stderr').decode() and '-D' in runs['clippy-after-lint']['command'] and '--all-targets' in runs['clippy-after-lint']['command'],'retained lint failure/current strict')
        broad=tests('core-plan-final');app=tests('app-units-final')
        need(broad['failed']==broad['ignored']==0 and broad['passed']>2056 and (app['passed'],app['failed'],app['ignored'])==(1304,0,11),'actual final broad/App totals')
        workers=focused_workers(before);plans=focused_plan(before)
        allowed='crates/deadpan-cli/tests/project_commands/macros/framing.rs'
        for n,d in workers.items():need(d['owned_current_differences']==([allowed] if n in ('worker-core-1','worker-core-scoped-1','worker-cli-1','worker-cli-2') else []),'exact failed helper difference')
        need(plans['plan-focused-1']['current_owned_differences']==['crates/deadpan-plan/tests/picture_plan/scoped_follow_creep.rs'] and plans['plan-focused-2']['current_owned_differences']==[],'historical Plan style-only rerun')
        old=read('plan-pre-clippy-fix-scoped_follow_creep.rs.txt').decode();new=read(R/'crates/deadpan-plan/tests/picture_plan/scoped_follow_creep.rs').decode()
        oldblock='''        if has_play(address, "inner", 1) {
            if let Some(next) = self.second.get(&mapped) {
                mapped = next.clone();
            }
        }'''
        # Record the complete textual diff; final broad/Plan receipts bind current bytes.
        diff=list(difflib.unified_diff(old.splitlines(),new.splitlines(),lineterm=''))
        need(len(diff)<30 and 'if let Some(next)' in old and '&& let Some(next)' in new,'bounded test-only Plan let-chain correction')
        wf=obj('worker-scoped-final-freeze.json');need(wf['authority_unchanged'] is True and meta(R/'crates/deadpan-core/src/semantic/planner/framing.rs')['sha256']==before['crates/deadpan-core/src/semantic/planner/framing.rs']['sha256'],'retained authority freeze')
        for p,h in wf['files'].items():need(before[p]['sha256']==h,'worker final source bytes')
        authority=read(wf['authority_original_copy'])
        need(hashlib.sha256(authority).hexdigest()==wf['authority_sha256'] and authority in read(R/'crates/deadpan-core/src/semantic/planner/framing.rs'),'exact retained unchanged authority function')
        need(wf['base']==BASE and wf['runner_sha256']==meta(S/'worker-run.py')['sha256'],'worker base/runner')
        live,terminal=obj('app-unit-executable-live.json'),obj('app-unit-executable-terminal.json')
        need(live['binary']==terminal['binary'] and all(live[k]==terminal[k] for k in ('bytes','sha256')) and meta(live['binary'])=={k:live[k] for k in ('bytes','sha256')},'actual App unit live/terminal match')
        need(runs['app-units-final']['started_at']<live['observed_at']<runs['app-units-final']['finished_at'] and Path(live['binary']).name in read('app-units-final.stderr').decode(),'live actual Cargo test executable')
        nr=[];scoped=None
        for n,scenario,count,warnings in [('native-scoped-first','scoped-follow-creep',155,3),('native-repeat-follow-creep-final','repeat-follow-creep',134,3),('native-retime-follow-creep-final','retime-follow-creep',134,3),('native-zoom-final','zoom',213,3),('native-sequence-follow-creep-final','sequence-follow-creep',101,3),('native-hold-follow-creep-final','hold-follow-creep',67,2)]:
            d=obj(n+'/report.json');md=d['metadata'];ss={x['name']:x for x in d['scenarios']};one=ss[scenario]
            need(d['failed'] is False and set(ss)=={scenario,'kestrel-shortcuts'} and md['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and md['build_profile']=='release' and md['physical_display_measured'] is False and md['gpu_readback_in_timing_run'] is False,'actual native candidate/limits')
            need(len(one['checks'])==count and all(c['passed'] is True for sc in ss.values() for c in sc['checks']) and not any(x['severity']=='failure' for sc in ss.values() for x in sc['findings']),'actual native complete checks')
            need(sum(x['severity']=='warning' for x in one['findings'])==warnings,'actual layout warnings')
            k=ss['kestrel-shortcuts']['checks'];need(len(k)==1 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['conflicts']==[],'local Kestrel compatibility')
            argv=runs[n]['command'];need(meta(argv[0])['sha256']==md['binary_sha256'] and meta(argv[argv.index('--kestrel-source')+1])['sha256']==k[0]['actual']['source_sha256']==k[0]['actual']['live_source_sha256'],'actual App and local router source')
            nr.append(dict(label=n,scenario=scenario,checks=count,kestrel=1,warnings=warnings,findings=one['findings'],skipped=one['skipped'],report=meta(S/n/'report.json'),html=meta(S/n/'report.html')))
            if scenario=='scoped-follow-creep':
                scoped=d;need(one['skipped']==['OS picker behavior: only its selected path is scripted','Physical display presentation, VoiceOver, OS IME delivery and audio device output'],'exact two declared exclusions')
        need(not (S/'media-first/commands.json').exists() and len(obj('media-second/commands.json'))==1 and obj('media-second/commands.json')[0]['exit_code']==0,'first before CLI; second dump-only')
        need('skipped' in read('media-first.stderr').decode() and 'fallback' in read('media-second.stderr').decode(),'actual earlier driver assertion errors')
        prep=obj('media-preparation-3.json')
        for p,h in prep['pins'].items():need(meta(S/p)['sha256']==h,'preserved media attempt/preflight bytes')
        need(meta(S/'media.py')['sha256']==prep['pins']['media.py'],'actual successful driver identity')
        mm=media(final,scoped)
        vis=obj('visual-review.json');images=[png(x['path'],{k:x[k] for k in ('bytes','sha256')}) for x in vis['images']]
        need(len(images)==4,'Root four PNG identities')
        host=obj('host.json');read('review-notes.json')
        for name in ('review-general.md','review-authority.md','review-test.md','design.md','worker-scoped-final-handoff.md','plan-handoff-2.md','native-harness-handoff.md'):read(name)
        out.update(status='no_findings',final_source_count=22,final_sources=before,final_binaries=final['binaries'],candidate_equivalence=eq,tests={n:{k:v for k,v in t.items() if k!='passed_names'} for n,t in [('core-plan-final',broad),('app-units-final',app)]},focused_workers=workers,focused_plan=plans,plan_style_only_diff=diff,app_unit_live_terminal=terminal,native_reports=nr,media=mm,root_image_identities=images,host=host,retained_failures={'worker-cli-1':dict(exit=101,passed=11,failed=5,classification='test helper revision-header comparison'),'worker-cli-2':dict(exit=101,passed=15,failed=1,classification='test helper ancestor reprojection oracle'),'clippy-first':dict(exit=101,classification='test-only collapsible_if'),'media-first':dict(exit=1,cli_commands=0,classification='driver incorrectly rejected declared exclusions'),'media-second':dict(exit=1,cli_commands=1,classification='driver expected live center instead of preserved independent fallback'),'commands-write':dict(exit=101,classification='intentional generated command reference update')})
    except Exception as e:
        traceback.print_exc();out.update(status='findings',findings=[dict(type=type(e).__name__,message=str(e))])
    finally:
        out['current_scoped_sources_unchanged']=before=={p:meta(R/p) for p in before}
        if not out['current_scoped_sources_unchanged']:out.update(status='findings');out['findings'].append(dict(message='source changed during audit'))
        out['input_files_unchanged']=all(meta(p)==m for p,m in PINS.items())
        if not out['input_files_unchanged']:out['status']='findings';out['findings'].append(dict(message='pinned evidence input changed during audit'))
        out['limitations']=['Offline read-only audit; no product/Cargo/decoder/worker/model/Git or collector execution.','Twenty-two listed final paths and actual named focused scopes are not a whole repository source archive.','Focused historic failures and initial test-only lint failure remain failed; two media driver assertions remain failed. Final broad/current focused and third media runs are independent recorded successes.','One byte-identical first/final release candidate. App unit binary observed live and at terminal; Cargo executable bytes are observations rather than archived build binaries.','All native warning/skipped records and eleven existing App ignores retained. No physical display/input/IME/picker/VoiceOver/listening/latency claims.','Movie comparison uses shared picture/limited-DSP semantics, not independent DSP or encoded audio bit equality. Neighbor unobservability retained.','Root PNG byte integrity only, no independent visual judgment. Separate primary and regression packages require separate inventories and shared identity binding in later verification.']
        out['preserved_pre_final_passing_attempt']={'path':'audit-history/third-passing-before-final-integrity-guard','preservation_manifest':meta(S/'audit-history/third-passing-before-final-integrity-guard/preservation.json')};out['preserved_first_audit_attempt']={'path':'audit-history/first-historical-worker-scope-stop','classification':'Auditor over-narrow historical worker scope assumption; only shared CLI test helper drift, not production drift','preservation_manifest':meta(S/'audit-history/first-historical-worker-scope-stop/preservation.json')};out['preserved_second_audit_attempt']={'path':'audit-history/second-q32-stage-oracle-stop','classification':'Auditor fixed-point interpolation staging mismatch','preservation_manifest':meta(S/'audit-history/second-q32-stage-oracle-stop/preservation.json')};out['input_file_pins']=PINS;out['verifier_sha256']=meta(Path(__file__))['sha256'];out['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (S/'audit-scoped-evidence.json').open('x') as f:json.dump(out,f,indent=2,sort_keys=True);f.write('\n')
        print(json.dumps({k:out.get(k) for k in ('status','findings','final_source_count','current_scoped_sources_unchanged')},sort_keys=True))
    return int(out['status']!='no_findings')
if __name__=='__main__':sys.exit(main())
