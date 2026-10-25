#!/usr/bin/env python3
"""Independent bounded, read-only verification of both scoped Follow packages."""
import binascii, collections, datetime, gzip, hashlib, io, json, os, re, sqlite3, stat, struct, sys, tarfile, tempfile, traceback
from fractions import Fraction as F
from pathlib import Path, PurePosixPath
S=Path('/tmp/deadpan-scoped-follow-creep-2026-10-24')
R=Path('/Users/michael/Code/deadpan')
E=R/'tools/media-qualification/evidence'
LIMIT=268435456
PINS={'primary':('2026-10-24-scoped-follow-creep','b7858060082699ba66d5cf68e954414c4fb6a018ab6eb309b6dbf8f96bc10a6b',218,15245224),
      'regressions':('2026-10-24-scoped-follow-creep-regressions','0a980f41b3e92c3115c2699d4fcf33606cd89f30de69bd3c7fac8bd7ebcdf5d9',56,6111277)}
AUDIT='19a50d4712bb0d7c68e8be218cacd019c3678cf250e301723adaf391f3fc3ec8'
def need(v,m):
    if not v:raise AssertionError(m)
def sha(b):return hashlib.sha256(b).hexdigest()
def bm(b):return dict(bytes=len(b),sha256=sha(b))
def meta(p):
    p=Path(p);need(stat.S_ISREG(p.lstat().st_mode),'regular file required:'+str(p));h=hashlib.sha256();n=0
    with p.open('rb') as f:
        for b in iter(lambda:f.read(1048576),b''):h.update(b);n+=len(b)
    return dict(bytes=n,sha256=h.hexdigest())
def safe(n):
    p=PurePosixPath(n);need(n and str(p)==n and not p.is_absolute() and all(x not in ('','.','..') for x in p.parts),'safe relative path');need(not n.endswith('.rs'),'snapshots must be text data')
def tree(root):
    need(stat.S_ISDIR(Path(root).lstat().st_mode),'regular root directory');out={}
    for d,ds,fs in os.walk(root,followlinks=False):
        for n in ds:need(stat.S_ISDIR((Path(d)/n).lstat().st_mode),'no directory links')
        for n in fs:out[(Path(d)/n).relative_to(root).as_posix()]=meta(Path(d)/n)
    return dict(sorted(out.items()))
def gunzip(b):
    need(b[:4]==b'\x1f\x8b\x08\x00' and b[4:8]==b'\0'*4,'canonical gzip header')
    with gzip.GzipFile(fileobj=io.BytesIO(b)) as f:
        raw=f.read(LIMIT+1);need(len(raw)<=LIMIT and not f.read(1),'bounded complete gzip')
    return raw
def texttests(raw):
    t=raw.decode();ss=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',t,re.M)
    totals=tuple(sum(int(x[i]) for x in ss) for i in range(3))
    need(ss and len(re.findall(r'^test .+ \.\.\. ok$',t,re.M))==totals[0] and len(re.findall(r'^test .+ \.\.\. ignored(?:,.*)?$',t,re.M))==totals[2],'actual named tests and ignored')
    return totals
def sqlite_rows(b):
    # Only bounded archived checkpoint bytes enter a fresh private scratch file.
    # No package database/sidecar is opened writable and no WAL is replayed.
    need(len(b)<=LIMIT,'bounded SQLite bytes')
    with tempfile.TemporaryDirectory(prefix='package-sqlite-read-',dir=S) as temp:
        p=Path(temp)/'checkpoint.sqlite'
        with p.open('xb') as f:f.write(b)
        need(meta(p)==bm(b) and not Path(str(p)+'-wal').exists(),'exact isolated checkpoint/no WAL')
        with sqlite3.connect(p.as_uri()+'?mode=ro&immutable=1',uri=True) as c:
            need(c.execute('pragma quick_check').fetchall()==[('ok',)],'archived SQLite integrity')
            tables={t:sorted(c.execute('select * from "'+t+'"').fetchall(),key=repr) for t in ('revisions','revision_patches','history','history_receipt','redo','state','transaction_steps')}
            kinds=dict(c.execute('select kind,count(*) from revisions group by kind').fetchall())
        need(meta(p)==bm(b) and sorted(x.name for x in Path(temp).iterdir())==['checkpoint.sqlite'],'immutable scratch DB unchanged/no sidecars')
    need(not Path(temp).exists(),'temporary SQLite copies removed')
    return tables,kinds
def even(v):
    a,r=divmod(v.numerator,v.denominator);return a+int(2*r>v.denominator or (2*r==v.denominator and a%2!=0))
def ratio(v):return F(int(v['numerator']),int(v['denominator']))
def q(v):return F(even(v*(1<<32)),1<<32)
def scale(j):
    a,b=even(F(11,10)*(1<<32)),even(F(11,5)*(1<<32));pr=even(F(2*j+1,72)*(1<<32))
    return F(a+even(F((b-a)*pr,1<<32)),1<<32)
old_names=['audit-scoped-evidence.py','audit-scoped-evidence.json','audit-scoped-evidence.stdout','audit-scoped-evidence.stderr']
old_before={n:meta(S/n) for n in old_names};history_before=tree(S/'audit-history')
before={k:tree(E/v[0]) for k,v in PINS.items()};source_before={}
result=dict(schema_version=1,status='running',findings=[],started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
all_sources={};bundles={}
try:
    for key,(name,ih,np,nb) in PINS.items():
        p=E/name;inv=json.loads((p/'files.json').read_bytes());need(before[key]['files.json']['sha256']==ih,'frozen inventory '+key)
        need(set(before[key])==set(inv)|{'files.json'} and len(inv)==np and sum(x['bytes'] for x in inv.values())==nb,'inventory exact membership and payload totals')
        for n,m in inv.items():safe(n);need(before[key][n]==m,'every payload byte/hash')
        ledger=json.loads((p/'encoding-ledger.json').read_bytes());need(isinstance(ledger,list),'actual encoding ledger list')
        data={};rows={};counts=collections.Counter();regular=0;decoded=0
        for row in ledger:
            n=row['path'];safe(n);need(n in inv and n not in rows,'unique inventoried encoding')
            b=(p/n).read_bytes();need(bm(b)==dict(bytes=row['stored_bytes'],sha256=row['stored_sha256']),'stored ledger bytes')
            need(row['encoding'] in ('identity','gzip','tar+gzip'),'actual known encoding')
            raw=b if row['encoding']=='identity' else gunzip(b)
            need(bm(raw)==dict(bytes=row['decoded_bytes'],sha256=row['decoded_sha256']),'every decoded hash')
            data[n]=raw;rows[n]=row;counts[row['encoding']]+=1;decoded+=len(raw)
            if row['encoding']!='tar+gzip':
                need(meta(row['source'])==bm(raw),'decoded actual approved source')
                all_sources.setdefault(str(Path(row['source']).resolve()),[]).append(raw);regular+=len(raw)
                need(raw[:4] not in (b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\x7fELF'),'no executable payload')
        need(set(inv)==set(rows)|{'collection.json','archive-ledger.json','encoding-ledger.json'},'complete encoding ledger coverage')
        aa=json.loads((p/'archive-ledger.json').read_bytes());archives={};archive_raw=0;omissions=[];summ=[]
        for a in aa:
            need(a['path'] in rows and rows[a['path']]['encoding']=='tar+gzip','archive encoding match')
            expected={m['path']:{k:m[k] for k in ('bytes','sha256')} for m in a['members']};need(len(expected)==len(a['members']),'unique archive expected members')
            members={};order=[]
            with tarfile.open(fileobj=io.BytesIO(data[a['path']]),mode='r:') as tf:
                for m in tf:
                    safe(m.name);need(m.isfile() and not m.issparse() and m.name not in members and m.name in expected and 0<=m.size<=LIMIT,'bounded safe file-only TAR')
                    need((m.uid,m.gid,m.uname,m.gname,m.mode,m.mtime)==(0,0,'','',0o644,0),'deterministic TAR metadata')
                    need(set(m.pax_headers)<={'path'} and ('path' not in m.pax_headers or m.pax_headers['path']==m.name),'safe exact PAX path')
                    b=tf.extractfile(m).read(LIMIT+1);need(len(b)==m.size and bm(b)==expected[m.name],'every archived member bytes/hash')
                    need(b[:4] not in (b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\x7fELF'),'no executable archive data')
                    members[m.name]=b;order.append(m.name);all_sources.setdefault(str((Path(a['root'])/m.name).resolve()),[]).append(b)
            need(order==sorted(order) and set(members)==set(expected),'complete deterministic member order')
            original=tree(a['root']);omit=a['omitted_sidecars']
            need(set(original)==set(expected)|set(omit) and all(original[n]==m for n,m in expected.items()),'full original archive root retained or explicitly classified')
            for n,m in omit.items():
                need(n.endswith(('-shm','-wal')),'only SQLite metadata omission');base=n[:-4]
                need(base in expected and base+'-wal' in omit and base+'-shm' in omit,'paired omission with authored database retained')
                need(omit[base+'-wal']['bytes']==0 and omit[base+'-wal']['sha256']==sha(b'') and omit[base+'-shm']['bytes']==32768,'empty WAL and bounded SHM policy')
                need(original[n]=={k:m[k] for k in ('bytes','sha256')} and 'nonauthored' in m['reason'],'actual omitted hash and explicit metadata classification')
                omissions.append(dict(archive=a['path'],path=n,**m))
            need(len(original)==a['source_files'] and sum(v['bytes'] for v in original.values())==a['source_bytes'],'full raw archive charge')
            for n,b in members.items():
                if n.endswith('-wal'):need(not b,'no nonempty retained WAL')
                if n.endswith('-shm'):need(len(b)==32768 and n[:-4]+'-wal' in members and not members[n[:-4]+'-wal'],'retained SHM empty-WAL pair')
            archives[a['path']]=members;archive_raw+=a['source_bytes']
            summ.append(dict(path=a['path'],source_files=a['source_files'],retained_members=len(members),source_bytes=a['source_bytes'],retained_member_bytes=sum(map(len,members.values())),metadata_omissions=len(omit)))
        col=json.loads((p/'collection.json').read_bytes());plan=json.loads(data['packaging/collect-plan.json'])
        need(col['raw_payload_bytes']==regular+archive_raw<=plan['raw_byte_limit']==LIMIT,'each package cumulative raw budget')
        need(sha(data['packaging/collect-plan.json'])==col['plan_sha256'] and col['script_sha256']==sha(data['packaging/collect.py.txt'])=='320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e' and sha(data['packaging/bundle-helpers.py.txt'])=='2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13','actual plan/collector/helper')
        for n,m in plan['inputs'].items():need(meta(R/n)==m,'actual retained input')
        bundles[key]=dict(root=p,inv=inv,data=data,rows=rows,archives=archives,plan=plan)
        result[key]=dict(files=len(inv)+1,payloads=len(inv),payload_bytes=nb,inventory_sha256=ih,encodings=dict(counts),encoding_rows=len(rows),gzip_streams=counts['gzip']+counts['tar+gzip'],encoded_stored_bytes=sum(r['stored_bytes'] for r in ledger),decoded_ledger_bytes=decoded,raw_regular_bytes=regular,archive_source_bytes=archive_raw,raw_payload_bytes=col['raw_payload_bytes'],raw_byte_limit=LIMIT,archives=summ,tar_members=sum(len(x) for x in archives.values()),retained_archive_bytes=sum(len(v) for a in archives.values() for v in a.values()),metadata_omissions=omissions,authored_omissions=0,plan_sha256=col['plan_sha256'])
    def scratch(n):
        hits=all_sources.get(str((S/n).resolve()),[]);need(hits and all(b==hits[0] for b in hits),'consistent retained scratch counterpart:'+n);return hits[0]
    def obj(n):return json.loads(scratch(n))
    prior=obj('audit-scoped-evidence.json');need(sha(scratch('audit-scoped-evidence.json'))==AUDIT==old_before['audit-scoped-evidence.json']['sha256'] and prior['status']=='no_findings' and not prior['findings'],'exact frozen raw audit')
    retained_audit_inputs=0
    for n,m in prior['input_file_pins'].items():
        hits=all_sources.get(str(Path(n).resolve()),[])
        if hits:need(all(bm(b)==m for b in hits),'all retained raw-audited bytes');retained_audit_inputs+=1
    binding=obj('regressions-binding.json');need(sha(scratch('regressions-binding.json'))=='b84eab4006ae3a88ff4959988a7ecfd974c12c57d54232147c7322e6003ad240','frozen cross-package binding')
    need(binding['regressions_package']=='tools/media-qualification/evidence/'+PINS['regressions'][0] and binding['inventory']==before['regressions']['files.json'] and binding['payloads']==56 and binding['stored_payload_bytes']==6111277,'exact regression inventory link')
    reg=bundles['regressions']
    for n,m in binding['original_report_and_run_stream_pins'].items():
        hits=[k for k in reg['data'] if k==n or k==n+'.gz'];need(len(hits)==1 and bm(reg['data'][hits[0]])==m,'complete regression report pair and receipt pins')
    for n,m in binding['shared_witnesses'].items():need(bm(scratch(n))==m,'shared source/binary identity record')
    final,first=obj('final-identities.json'),obj('first-identities.json');need(final['sources']==first['sources']==prior['final_sources'] and len(final['sources'])==22,'one source22 candidate')
    source_before={n:meta(R/n) for n in final['sources']};need(source_before==final['sources'],'current22 exact')
    for n,m in final['sources'].items():
        need(bm(scratch('source-final/'+n+'.txt'))==m if str((S/'source-final'/ (n+'.txt')).resolve()) in all_sources else any(bm(b)==m for b in all_sources.get(str((R/n).resolve()),[])),'final source snapshot exact')
    a=bundles['primary']['archives'];need({n[:-4]:bm(b) for n,b in a['archives/source-first.tar.gz'].items()}==first['sources'],'all first22 snapshot bytes')
    for ids in (first,final):
        for n,m in ids['binaries'].items():need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'retained executed private sibling binaries')
    eq=obj('candidate-equivalence.json');need(eq['source_paths']==22 and eq['source_bytes_equal'] is True and eq['binary_bytes_equal'] is True and final['binaries']==prior['final_binaries'],'one release candidate first/final actual identity')
    for archive,rows in a.items():
        if archive.endswith('-source.tar.gz') and 'worker-' in archive:
            label=archive.split('/')[-1][:-len('-source.tar.gz')];d=obj(label+'.json')
            need({n[:-4]:sha(b) for n,b in rows.items()}==d['owned_before'],'complete historical worker12 source bytes')
        if archive in ('archives/plan-focused-1-source.tar.gz','archives/plan-focused-2-source.tar.gz'):
            label=archive.split('/')[-1][:-len('-source.tar.gz')];d=obj(label+'.json')
            union={**d['source_before'],**d['core_plan_basis_before'],**d['manifest_before']['present']}
            need(len(union)==304 and {n[:-4]:bm(b) for n,b in rows.items()}==union,'Plan actual304 snapshot union')
    hist=a['archives/audit-history.tar.gz'];need(len(hist)==15,'all three audit attempts retained')
    for prefix in ('first-historical-worker-scope-stop','second-q32-stage-oracle-stop','third-passing-before-final-integrity-guard'):
        pr=json.loads(hist[prefix+'/preservation.json'])
        for n,m in pr['files'].items():need(bm(hist[prefix+'/'+n])==m,'exact preserved audit attempt')
        status=json.loads(hist[prefix+'/audit-scoped-evidence.json'])['status'];need(status==('no_findings' if prefix.startswith('third-') else 'findings'),'honest stopped/passing audit history')
    expected={'core-plan-final':(2059,0,0),'app-units-final':(1304,0,11),'worker-core-1':(3,0,0),'worker-core-scoped-1':(59,0,0),'worker-cli-1':(11,5,0),'worker-cli-2':(15,1,0),'worker-cli-3':(16,0,0),'worker-cli-unit-1':(1,0,0),'plan-focused-1':(2,0,0),'plan-focused-2':(2,0,0)}
    actualtests={}
    for n,counts in expected.items():need(texttests(scratch(n+'.stdout'))==counts,'actual raw test summary:'+n);actualtests[n]=counts
    allruns=[n[:-5] for n in prior['input_file_pins'] if n.startswith(str(S)+'/') and n.endswith('.json') and Path(n).parent==S]
    standard=['format-final','app-check-first','format-after-lint','app-check-after-lint','clippy-after-lint','core-plan-final','app-units-final','release-first','native-scoped-first','native-repeat-follow-creep-final','native-retime-follow-creep-final','native-zoom-final','native-sequence-follow-creep-final','native-hold-follow-creep-final','media-third','clippy-first','commands-write','media-first','media-second']
    for n in standard:
        d=obj(n+'.json');need(d['source_before']==d['source_after'] and d['sources_unchanged'] is True and d['binary_before']==d['binary_after'] and d['binaries_unchanged'] is True,'actual terminal unchanged run guards')
        need(d['exit_code']==({'clippy-first':101,'commands-write':101,'media-first':1,'media-second':1}.get(n,0)) and 'finished_at' in d,'actual preserved success/failure terminal')
        for path,m in d['streams'].items():need(bm(scratch(Path(path).name))==m,'exact terminal streams')
    live,terminal=obj('app-unit-executable-live.json'),obj('app-unit-executable-terminal.json')
    need(live['binary']==terminal['binary'] and {k:live[k] for k in ('bytes','sha256')}=={k:terminal[k] for k in ('bytes','sha256')}==meta(live['binary']),'actual unit live/terminal bytes')
    run=obj('app-units-final.json');need(run['started_at']<live['observed_at']<run['finished_at'] and Path(live['binary']).name in scratch('app-units-final.stderr').decode(),'live observed actual unit path')
    natives=[]
    for old in prior['native_reports']:
        n=old['label'];d=obj(n+'/report.json');sc={x['name']:x for x in d['scenarios']};one=sc[old['scenario']]
        need(d['failed'] is False and d['metadata']['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and d['metadata']['physical_display_measured'] is False and set(sc)=={old['scenario'],'kestrel-shortcuts'},'actual complete native reports')
        need(len(one['checks'])==old['checks'] and all(c['passed'] is True for v in sc.values() for c in v['checks']) and one['findings']==old['findings'] and one['skipped']==old['skipped'],'all native checks/warnings/exclusions retained')
        need(bm(scratch(n+'/report.json'))==old['report'] and bm(scratch(n+'/report.html'))==old['html'],'untruncated native JSON and HTML')
        k=sc['kestrel-shortcuts']['checks'];need(len(k)==1 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['conflicts']==[],'actual Kestrel compatibility')
        natives.append(dict(label=n,checks=len(one['checks']),kestrel=1,warnings=old['warnings'],skipped=old['skipped']))
    visual=obj('visual-review.json');pngs=[]
    for image in visual['images']:
        b=scratch(Path(image['path']).relative_to(S).as_posix());need(bm(b)=={k:image[k] for k in ('bytes','sha256')} and b[:8]==b'\x89PNG\r\n\x1a\n','four exact selected PNG byte identities')
        i=8;dim=None;tag=None
        while i<len(b):
            need(i+12<=len(b),'PNG header');n=int.from_bytes(b[i:i+4],'big');tag=b[i+4:i+8];end=i+n+12
            need(end<=len(b) and binascii.crc32(b[i+4:i+n+8])&0xffffffff==int.from_bytes(b[i+n+8:end],'big'),'PNG chunk CRC/bounds')
            if tag==b'IHDR':dim=struct.unpack('>II',b[i+8:i+16])
            i=end
            if tag==b'IEND':need(n==0 and i==len(b),'PNG exact terminal');break
        need(tag==b'IEND' and dim and all(0<x<=8192 for x in dim),'bounded complete PNG');pngs.append(dict(path=image['path'],**bm(b),dimensions=dim))
    need(len(pngs)==4,'four Root pictures')
    m=a['archives/media-third.tar.gz'];original=a['archives/actual-native-scoped-project.tar.gz'];get=lambda n:json.loads(m[n])
    cmds=get('commands.json');need(len(cmds)==9 and all(x['exit_code']==0 for x in cmds) and [x['name'] for x in cmds]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'],'all nine actual CLI commands')
    for x in cmds:need(meta(x['argv'][0])['sha256']==final['binaries']['deadpan-cli']['sha256'],'actual media CLI identity')
    doc=get('dump.stdout');need(doc==get('portable-dump.stdout') and get('validate.stdout')['valid'] is True and get('portable-validate.stdout')['valid'] is True,'exact portable authored doc/current Store validation')
    v=get('verification.json');need(v==get('verify.stdout') and v['passed'] is True and v['failures']==[] and len(v['pictures'])==240 and v['range']==[0,240] and v['revision_id']==doc['revision_id'],'full retained verification')
    oracle=get('independent-native-oracle.json');need(len(oracle)==240,'complete scoped oracle')
    target=doc['targets']['target-1'];obs=collections.Counter()
    for frame,(p,o) in enumerate(zip(v['pictures'],oracle)):
        outer,local=divmod(frame,124);play,j=divmod(local,40);gap=local>=116 or j>=36
        ticks=F(23023) if gap else F((2*j+1)*1001,3);ordinal=int(ticks//1001)
        need(o['Edit']==frame and o['gap']==gap and o['ticks']==str(ticks) and o['Original_ordinal']==ordinal and o['selected']==(164<=frame<200),'all independent timing rows')
        if not gap:need([ratio(o['owner_pose'][k]) for k in ('center_x','center_y','scale')]==[q(F(58 if ordinal<12 else 68,100)),F(1,2),scale(j) if 164<=frame<200 else q(F(11,10))],'all independent quantized scoped poses')
        need(p['passed'] is True and p['flags']==[] and p['project_frame']==p['output_frame']==frame and p['provenance']==dict(kind='original',asset=target['asset'],source_frame=ordinal,source_pts=ordinal*1001),'all exported frame provenance')
        for k in ('previous_index','next_index'):
            if p[k] is not None:obs[p[k]['status']]+=1
    need(dict(obs)==dict(observable=262,unobservable=216),'neighbor observability exact')
    au=v['audio'];need(len(au)==1 and au[0]['window']==au[0]['project_samples']==[0,384384] and au[0]['passed'] is True and au[0]['flags']==[] and au[0]['uncovered_samples']==0 and au[0]['offset_status']=='verified_zero' and au[0]['measured_offset_samples']==0 and all(s['status']=='verified_zero' and s['peak_lag']==0 for s in au[0]['segments']),'full PCM zero offset')
    movie=bm(m['scoped-follow-creep.mp4']);need(movie==prior['media']['movie'] and v['movie']==prior['media']['movie_metadata'],'actual full movie metadata/hash')
    need(v['movie']['audio']==dict(first_pts=-2048,physical_samples=387072,presented_end=384384,gaps=[]) and v['movie']['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=384384),'actual AAC edit metadata')
    hn,kn=sqlite_rows(original['project.sqlite']);hp,kp=sqlite_rows(m['portable.deadpan/project.sqlite'])
    need(hn==hp and kn==kp==dict(initial=1,edit=15,undo=7,redo=2),'complete immutable archived history equality')
    counts={k:len(x) for k,x in hn.items()};need(counts==prior['media']['history_counts'],'all seven SQL table counts')
    cr=get('copy.stdout')['portable_copy']['verified_history'];need(cr==prior['media']['portable_history_receipt'] and cr['revisions']==25 and cr['replayed']+cr['verified_by_receipt']==25,'all revision receipt authority')
    need(len(a['archives/media-first.tar.gz'])==1 and 'commands.json' not in a['archives/media-first.tar.gz'] and len(json.loads(a['archives/media-second.tar.gz']['commands.json']))==1,'both stopped media scopes unmodified')
    need(get('result.json')==json.loads(scratch('media-third.stdout')) and get('result.json')['history_before_after_and_portable']['sha256']==prior['media']['history_digest'],'actual successful driver terminal digest')
    result.update(status='no_findings',findings=[],retained_raw_audit_sha256=AUDIT,retained_audited_inputs=retained_audit_inputs,final_source_count=22,source_bytes=final['sources'],candidate_equivalence=eq,cross_package_binding_sha256=sha(scratch('regressions-binding.json')),raw_test_counts=actualtests,native_reports=natives,root_png_identities=pngs,media=dict(commands=9,pictures=240,audio_samples=384384,neighbor_observability=dict(obs),movie=movie,history_counts=counts,revision_kinds=kn,complete_archived_native_portable_sql_equal=True,portable_history_receipt=cr),combined_payloads=274,combined_files=276,combined_stored_payload_bytes=21356501)
except Exception as e:
    traceback.print_exc();result.update(status='findings',findings=[dict(type=type(e).__name__,message=str(e))])
finally:
    result['package_trees_unchanged']={k:before[k]==tree(E/v[0]) for k,v in PINS.items()}
    result['prior_audit_files_unchanged']=old_before=={n:meta(S/n) for n in old_names}
    result['prior_audit_history_unchanged']=history_before==tree(S/'audit-history')
    result['current_sources_unchanged']=source_before=={n:meta(R/n) for n in source_before}
    if not all(result['package_trees_unchanged'].values()) or not result['prior_audit_files_unchanged'] or not result['prior_audit_history_unchanged'] or not result['current_sources_unchanged']:
        result['status']='findings';result['findings'].append(dict(message='Evidence/source changed during verifier'))
    result['limitations']=['Read-only independent package verification only; no collector/payload script/product/Cargo/test/worker/decoder/model/Git command run.','Two separate complete inventories and individual256MiB raw budgets; no claim that their combined raw charge fits one package.','Current22 source paths and named historical subsets are not whole-repository compilation. One first/final byte-identical candidate.','Prior focused CLI/test-only lint and media-driver assertion failures remain failed; all three raw-auditor attempts are retained honestly.','Native physical/picker exclusions, layout warnings,11 existing App ignores and262observable/216unobservable neighbors retained.','Selected PNG integrity binds Root visual review without independent visual/physical judgment.','Shared committed picture and limited-DSP references do not prove independent DSP/listening, encoded PCM bit equality, latency or requirement completion.','Archived SQLite bytes read through exclusive temporary scratch copies with mode=ro&immutable=1, verified unchanged and removed; no package database or sidecar opened writable.']
    result['preserved_first_verifier_attempt']={'path':'package-history/first-in-memory-sqlite-reader-stop','preservation_manifest':meta(S/'package-history/first-in-memory-sqlite-reader-stop/preservation.json'),'first_execution_passed':False,'classification':'Auditor SQLite deserialization adapter; not product evidence failure'}
    result['verifier_sha256']=meta(Path(__file__))['sha256'];result['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S/'package-verification.json').open('x') as f:json.dump(result,f,indent=2,sort_keys=True);f.write('\n')
    print(json.dumps({k:result.get(k) for k in ('status','findings','combined_files','combined_payloads','combined_stored_payload_bytes','package_trees_unchanged','prior_audit_files_unchanged','current_sources_unchanged')},sort_keys=True))
sys.exit(0 if result['status']=='no_findings' else 1)
