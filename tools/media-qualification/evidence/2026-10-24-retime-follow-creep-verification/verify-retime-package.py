#!/usr/bin/env python3
"""Independent bounded offline reader of the published Retime qualification pack."""
import binascii
import collections
import datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sqlite3
import stat
import struct
import sys
import tarfile
import traceback

S = Path("/tmp/deadpan-retime-follow-creep-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
P = R / "tools/media-qualification/evidence/2026-10-24-retime-follow-creep"
INV = "488348a4d58e81c307f947a968c8563a89d65740a18a619af4c9edb3002ce523"
AUDIT = "ba51734a1db42d9c156a22d2726d751dec2d1311b533c3fbf45388ce4499393e"
LIMIT = 268435456
result = {"schema_version": 1, "status": "running", "findings": [],
          "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "limitations": [
              "No collector, payload script, product, Cargo, test, worker, decoder, model or Git command executed.",
              "Seventeen scoped sources and relevant historical subsets are checked, not compilation of every repository file.",
              "Root's image review is bound by bytes; PNG integrity is not independent visual or physical-display qualification.",
              "Shared picture/limited-audio references do not establish independent DSP, listening, encoded bit equality, tracking accuracy or latency/requirement completion.",
              "The first offline audit schema stop and its passing retry are retained. Core focused global17 drift was only an unrelated Plan integration-test draft; own8/named9 remain current. App unit hash was observed live and at terminal. One release candidate only.",
          ]}


def need(ok, message):
    if not ok:
        raise AssertionError(message)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def meta(path):
    p = Path(path)
    need(stat.S_ISREG(p.lstat().st_mode), f"not regular: {p}")
    h = hashlib.sha256()
    size = 0
    with p.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
            size += len(block)
    return {"bytes": size, "sha256": h.hexdigest()}


def safe(name, allow_rust=False):
    p = PurePosixPath(name)
    need(name and not p.is_absolute() and str(p) == name
         and all(x not in ("", ".", "..") for x in p.parts), "unsafe relative path")
    need(allow_rust or not name.endswith(".rs"), "live Rust source instead of text snapshot")


def tree(root):
    root = Path(root)
    need(stat.S_ISDIR(root.lstat().st_mode), "not a directory")
    out = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            need(stat.S_ISDIR((Path(directory) / name).lstat().st_mode), "linked/special directory")
        for name in files:
            p = Path(directory) / name
            out[p.relative_to(root).as_posix()] = meta(p)
    return dict(sorted(out.items()))


def gunzip(b):
    need(b[:4] == b"\x1f\x8b\x08\x00" and b[4:8] == b"\0\0\0\0", "canonical gzip header")
    with gzip.GzipFile(fileobj=io.BytesIO(b)) as f:
        raw = f.read(LIMIT + 1)
        need(len(raw) <= LIMIT and not f.read(1), "oversized gzip")
    return raw


def dicts(o):
    if isinstance(o, dict):
        yield o
        for v in o.values():
            yield from dicts(v)
    elif isinstance(o, list):
        for v in o:
            yield from dicts(v)


old_names=['audit-retime-evidence.py','audit-retime-evidence.json','audit-retime-evidence.stdout','audit-retime-evidence.stderr']
old_before={n:meta(S/n) for n in old_names};history_before=tree(S/'audit-history')
before=tree(P);source_before={}
try:
    need(before['files.json']['sha256']==INV,'exact inventory pin')
    inv=json.loads((P/'files.json').read_bytes())
    need(len(inv)==141 and set(before)==set(inv)|{'files.json'} and sum(x['bytes'] for x in inv.values())==11811693,'strict inventory membership/bytes')
    for n,m in inv.items():safe(n);need(before[n]==m,'every actual payload identity')
    ledger=json.loads((P/'encoding-ledger.json').read_bytes());need(isinstance(ledger,list) and len(ledger)==138,'actual encoding ledger schema/count')
    rows,data,counts={}, {}, collections.Counter();raw_regular=0
    for r in ledger:
        n=r['path'];safe(n);need(n in inv and n not in rows,'unique inventoried ledger record')
        b=(P/n).read_bytes();need(len(b)==r['stored_bytes'] and sha(b)==r['stored_sha256'],'stored ledger bytes/hash')
        need(r['encoding'] in ('identity','gzip','tar+gzip'),'known encoding')
        raw=b if r['encoding']=='identity' else gunzip(b)
        need(len(raw)==r['decoded_bytes'] and sha(raw)==r['decoded_sha256'],'all decoded bytes/hash')
        rows[n],data[n]=r,raw;counts[r['encoding']]+=1
        if r['encoding']!='tar+gzip':
            need(meta(r['source'])==dict(bytes=len(raw),sha256=sha(raw)),'decoded source counterpart')
            need(raw[:4] not in {b'\xfe\xed\xfa\xce',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xcf\xfa\xed\xfe',b'\xca\xfe\xba\xbe',b'\x7fELF'},'no executable payload')
            raw_regular+=len(raw)
    need(counts==dict(identity=110,gzip=20,**{'tar+gzip':8}) and set(inv)==set(rows)|{'collection.json','archive-ledger.json','encoding-ledger.json'},'complete encoding aggregate/coverage')
    def scratch(n):
        hits=[p for p,r in rows.items() if Path(r['source']).resolve()==(S/n).resolve()]
        need(len(hits)==1,f'unique scratch counterpart:{n}');return data[hits[0]]
    def obj(n):return json.loads(scratch(n))
    aa=json.loads((P/'archive-ledger.json').read_bytes());need(isinstance(aa,list) and len(aa)==8,'eight archive records')
    archives,summaries={},[];archive_bytes=0
    for a in aa:
        expected={m['path']:dict(bytes=m['bytes'],sha256=m['sha256']) for m in a['members']}
        need(len(expected)==len(a['members']) and a['omitted_sidecars']=={},'no duplicate or omitted archive members')
        members,order={},[]
        with tarfile.open(fileobj=io.BytesIO(data[a['path']]),mode='r:') as tf:
            for m in tf:
                safe(m.name)
                need(m.isfile() and not m.issparse() and 0<=m.size<=LIMIT and m.name not in members and m.name in expected,'safe regular bounded unique TAR member')
                need((m.uid,m.gid,m.uname,m.gname,m.mode,m.mtime)==(0,0,'','',0o644,0),'deterministic TAR metadata')
                need(set(m.pax_headers)<={'path'} and ('path' not in m.pax_headers or m.pax_headers['path']==m.name),'safe PAX metadata')
                b=tf.extractfile(m).read(LIMIT+1);need(len(b)==m.size and dict(bytes=len(b),sha256=sha(b))==expected[m.name],'all actual TAR member bytes')
                need(b[:4] not in {b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\x7fELF'},'no executable archive data')
                members[m.name]=b;order.append(m.name)
        need(order==sorted(order) and set(members)==set(expected),'complete deterministic TAR membership/order')
        need(tree(a['root'])==dict(sorted(expected.items())),'entire original archive root retained without omissions')
        size=sum(m['bytes'] for m in expected.values());need(size==a['source_bytes'] and len(members)==a['source_files'],'archive raw totals')
        retained_sidecars=[]
        for n in members:
            if n.endswith(('-wal','-shm')):
                base=n[:-4];need(base in members and base+'-wal' in members and base+'-shm' in members,'paired retained checkpoint metadata')
                need(len(members[base+'-wal'])==0 and len(members[base+'-shm'])==32768,'retained empty WAL/bounded SHM')
                retained_sidecars.append(n)
        archives[a['path']]=members;archive_bytes+=size
        summaries.append(dict(path=a['path'],members=len(members),retained_member_bytes=size,source_bytes=a['source_bytes'],metadata_omissions=0,retained_sidecars=retained_sidecars))
    need([len(archives[a['path']]) for a in aa]==[17,8,8,5,300,30,13,5] and sum(len(x) for x in archives.values())==386 and archive_bytes==11241986,'archive aggregate count/bytes')
    collection=json.loads((P/'collection.json').read_bytes());plan=json.loads(data['packaging/collect-plan.json'])
    need(raw_regular+archive_bytes==collection['raw_payload_bytes']==164328488 and collection['raw_payload_bytes']<=plan['raw_byte_limit']==LIMIT,'cumulative raw-byte budget')
    need(sha(data['packaging/collect-plan.json'])==collection['plan_sha256']=='fb3e2b67909d721934ed604ce35e0a4d4bd832021f26c937683531873229a6bb','frozen plan')
    need(sha(data['packaging/collect.py.txt'])==collection['script_sha256']=='320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e','collector bytes')
    need(sha(data['packaging/bundle-helpers.py.txt'])=='2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13','helper bytes')
    prior=obj('audit-retime-evidence.json')
    need(sha(scratch('audit-retime-evidence.json'))==AUDIT==old_before['audit-retime-evidence.json']['sha256'] and prior['status']=='no_findings' and prior['findings']==[],'embedded frozen passing raw audit retry')
    retained_by_source={}
    for n,r in rows.items():
        if r['encoding']!='tar+gzip':retained_by_source.setdefault(str(Path(r['source']).resolve()),[]).append(dict(bytes=len(data[n]),sha256=sha(data[n])))
    for a in aa:
        for n,b in archives[a['path']].items():retained_by_source.setdefault(str((Path(a['root'])/n).resolve()),[]).append(dict(bytes=len(b),sha256=sha(b)))
    for p,m in prior['input_file_pins'].items():need(all(x==m for x in retained_by_source.get(str(Path(p).resolve()),[])),'previously audited retained bytes unchanged')
    final,first=obj('final-identities.json'),obj('first-identities.json');freeze=obj('source-review-freeze.json')
    need(final['sources']==first['sources']==freeze['sources']==prior['final_sources'] and len(final['sources'])==17,'source17 first/final/review binding')
    source_before={p:meta(R/p) for p in final['sources']};need(source_before==final['sources'],'current17 source bytes')
    for p,m in final['sources'].items():
        hits=[n for n,r in rows.items() if r['source']==str(R/p) and n.startswith('source/')]
        need(len(hits)==1 and dict(bytes=len(data[hits[0]]),sha256=sha(data[hits[0]]))==m and meta(S/'source-final'/(p+'.txt'))==m,'final snapshot exact bytes')
    need({n[:-4]:dict(bytes=len(b),sha256=sha(b)) for n,b in archives['archives/first-source.tar.gz'].items()}==first['sources'],'all first17 snapshots')
    worker=obj('worker-retime-final-freeze.json')
    for archive in ('archives/worker-core-source.tar.gz','archives/worker-cli-source.tar.gz'):need({n[:-4]:sha(b) for n,b in archives[archive].items()}==worker['files'],'eight focused worker snapshots')
    nh=obj('native-harness-freeze.json')
    need({x['path']:dict(bytes=x['bytes'],sha256=x['sha256']) for x in nh['changed_paths']}=={n[:-4]:dict(bytes=len(b),sha256=sha(b)) for n,b in archives['archives/native-worker-source.tar.gz'].items()},'five native helper snapshot pins')
    for ids in (first,final):
        for n,m in ids['binaries'].items():need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'retained actual executed siblings')
    need(len(final['binaries'])==5 and all({k:first['binaries'][n][k] for k in ('bytes','sha256')}=={k:final['binaries'][n][k] for k in ('bytes','sha256')} for n in final['binaries']),'five first/final binary identities')
    eq=obj('candidate-equivalence.json');need(eq['source_paths']==17 and eq['all_source_bytes_identical'] is True and eq['all_five_binary_bytes_identical'] is True,'one candidate provenance')
    for p,m in plan['inputs'].items():need(meta(R/p)==m,'actual build/document/fixture input bytes')
    # The first offline stop is preserved, not retrospectively reported passing.
    history=archives['archives/audit-history.tar.gz'];prefix='first-global-missing-schema-stop/'
    preservation=json.loads(history[prefix+'preservation.json']);need(len(history)==5,'entire first audit attempt retained')
    for n,m in preservation['files'].items():need(dict(bytes=len(history[prefix+n]),sha256=sha(history[prefix+n]))==m,'first-attempt exact byte preservation')
    stopped=json.loads(history[prefix+'audit-retime-evidence.json'])
    need(stopped['status']=='findings' and stopped['findings'][0]['message']=='all17 existed before and after' and stopped['current_scoped_sources_unchanged'] is True,'first schema assertion stop retained honestly')
    need(prior['preserved_first_audit_attempt']['product_failure'] is False and prior['preserved_first_audit_attempt']['preservation_manifest']==dict(bytes=len(history[prefix+'preservation.json']),sha256=sha(history[prefix+'preservation.json'])),'passing retry links actual first stop')
    raw_tests={}
    for n,count,ignored in [('core-plan-final',2056,0),('app-units-final',1304,11),('worker-core-1',2,0),('worker-cli-1',15,0),('plan-focused-1',3,0)]:
        t=scratch(n+'.stdout').decode();ss=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',t,re.M)
        totals=tuple(sum(int(x[i]) for x in ss) for i in range(3))
        need(totals==(count,0,ignored) and len(re.findall(r'^test .+ \.\.\. ok$',t,re.M))==count and len(re.findall(r'^test .+ \.\.\. ignored(?:,.*)?$',t,re.M))==ignored,'raw named test counts/ignored')
        need(obj(n+'.json')['exit_code']==0,'test terminal result')
        raw_tests[n]=list(totals)
    broad_names=re.findall(r'^test (.+) \.\.\. ok$',scratch('core-plan-final.stdout').decode(),re.M)
    for n in ('worker-core-1','plan-focused-1'):
        for name in re.findall(r'^test (.+) \.\.\. ok$',scratch(n+'.stdout').decode(),re.M):need(broad_names.count(name)==1,'exact focused cases in final broad')
    for n in ('worker-core-1','worker-cli-1'):
        d=obj(n+'.json')
        need(d['owned_before']==d['owned_after'] and d['owned_unchanged'] is True and d['basis_before']==d['basis_after'] and d['basis_unchanged'] is True and len(d['owned_before'])==8 and len(d['basis_before'])==9,'actual own8/named9 stable')
        need(d['global_complete_before'] is True and d['global_complete_after'] is True and d['global_missing_before']==d['global_missing_after']==[],'actual global missing-list schema')
        need(all(meta(R/p)['sha256']==h for p,h in d['owned_before'].items()) and all(meta(R/p)['sha256']==h for p,h in d['basis_before'].items()),'current focused owned/basis')
        delta={p:dict(before=h,after=d['global_source_after'][p]) for p,h in d['global_source_before'].items() if h!=d['global_source_after'][p]}
        expected={} if n=='worker-cli-1' else {'crates/deadpan-plan/tests/picture_plan/retime_follow_creep.rs':dict(before='3d7b380519fa7b311a815a2986acd5ab81c90d61b3006e36bd0cc4f5c31549aa',after='3fba182c5c07571e5dbfbda4f532ae2c259f2191c07e85508f982ad1f2c5880c')}
        need(delta==expected and d['global_source_unchanged']==(n=='worker-cli-1'),'exact concurrent Plan integration-test drift remains explicit')
        for suffix,h in d['stream_pins'].items():need(sha(scratch(n+suffix))==h,'focused raw stream pins')
    pd=obj('plan-focused-1.json')
    need(pd['source_before']==pd['source_after'] and pd['core_plan_basis_before']==pd['core_plan_basis_after'] and pd['global17_before']==pd['global17_after']==final['sources'] and pd['global17_unchanged'] is True and len(pd['core_plan_basis_before'])==287,'Plan own2/basis287/global17 guards')
    combined={**pd['source_before'],**pd['core_plan_basis_before'],**pd['global17_before']}
    need(len(combined)==300 and {n[:-4]:dict(bytes=len(b),sha256=sha(b)) for n,b in archives['archives/plan-focused-source.tar.gz'].items()}==combined,'entire actual Plan own+basis+global snapshot union300')
    for p,m in pd['streams'].items():need(dict(bytes=len(scratch(Path(p).name)),sha256=sha(scratch(Path(p).name)))==m,'Plan raw streams')
    for n in ('format-final','app-check-first','clippy-first','core-plan-final','app-units-final','release-final','native-retime-first','native-sequence-follow-creep-final','native-repeat-follow-creep-final','native-zoom-final','media-final'):
        r=obj(n+'.json');need(r['exit_code']==0 and r['source_before']==r['source_after']=={str(R/p):m for p,m in final['sources'].items()} and r['sources_unchanged'] is True and r['binary_before']==r['binary_after'] and r['binaries_unchanged'] is True,'terminal exact source17/runtime guards')
        for p,m in r['streams'].items():need(dict(bytes=len(scratch(Path(p).name)),sha256=sha(scratch(Path(p).name)))==m,'terminal output pin')
    need(obj('commands-write.json')['exit_code']==101,'intentional generated-reference writer preserved')
    live,terminal=obj('app-unit-executable-live.json'),obj('app-unit-executable-terminal.json')
    need(terminal['observed_live']==live and terminal['unchanged'] is True and terminal['terminal']=={k:live[k] for k in ('bytes','sha256')}==meta(live['path']) and live['sha256']=='ae505efaf475bdbc5b43c3c8616deb3b8400dc5157aef02fe4f4ec3a43fea29c','actual App unit live/terminal identity')
    unitrun=obj('app-units-final.json');need(datetime.datetime.fromisoformat(unitrun['started_at'])<datetime.datetime.fromisoformat(live['observed_at'])<datetime.datetime.fromisoformat(unitrun['finished_at']) and Path(live['path']).name in scratch('app-units-final.stderr').decode(),'live observation while actual Cargo path running')
    visual=obj('visual-review.json');pngs={}
    for n,row in rows.items():
        if not n.startswith('images/'):continue
        b=data[n];need(b[:8]==b'\x89PNG\r\n\x1a\n','PNG signature');i=8;dims=None
        while i<len(b):
            need(i+12<=len(b),'PNG chunk header')
            size=struct.unpack('>I',b[i:i+4])[0];tag=b[i+4:i+8];end=i+size+12
            need(end<=len(b) and size<=LIMIT and binascii.crc32(b[i+4:i+8+size])&0xffffffff==struct.unpack('>I',b[i+8+size:end])[0],'PNG chunk bounds/CRC')
            if tag==b'IHDR':dims=struct.unpack('>II',b[i+8:i+16])
            i=end
            if tag==b'IEND':need(size==0 and i==len(b),'PNG exact end');break
        need(tag==b'IEND' and dims and all(0<x<=8192 for x in dims),'complete bounded PNG')
        rel=Path(row['source']).resolve().relative_to(S.resolve()).as_posix();need(dict(bytes=len(b),sha256=sha(b))==visual['images'][rel],'Root selected image identity')
        pngs[n]=dict(bytes=len(b),sha256=sha(b),dimensions=dims)
    need(len(pngs)==4,'four selected Root visual witnesses')
    natives=[]
    for n,scenario,count,wc in [('native-retime-first','retime-follow-creep',129,3),('native-sequence-follow-creep-final','sequence-follow-creep',101,3),('native-repeat-follow-creep-final','repeat-follow-creep',129,3),('native-zoom-final','zoom',213,3)]:
        report=obj(n+'/report.json');scenes={x['name']:x for x in report['scenarios']}
        need(report['failed'] is False and report['metadata']['build_profile']=='release' and report['metadata']['physical_display_measured'] is False and report['metadata']['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and set(scenes)=={scenario,'kestrel-shortcuts'} and len(scenes[scenario]['checks'])==count,'native pass/current App/scenario count')
        need(all(c['passed'] is True for x in scenes.values() for c in x['checks']) and not any(f['severity']=='failure' for x in scenes.values() for f in x['findings']),'raw native successes')
        k=scenes['kestrel-shortcuts']['checks'];need(len(k)==1 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['conflicts']==[],'actual Kestrel scope')
        prev=next(x for x in prior['native_reports'] if x['name']==n)
        need(scenes[scenario]['findings']==prev['findings'] and scenes[scenario]['skipped']==prev['skipped'] and sum(f['severity']=='warning' for f in prev['findings'])==wc,'all warnings/skips preserved')
        natives.append(dict(name=n,checks=count,kestrel=1,warnings=wc,skips=prev['skipped']))
    m=archives['archives/final-native-retime-project-and-export.tar.gz']
    original=archives['archives/actual-native-retime-project.tar.gz']
    get=lambda n:json.loads(m[n])
    cmds=get('commands.json')
    need([x['name'] for x in cmds]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'] and all(x['exit_code']==0 for x in cmds),'actual nine media commands')
    for x in cmds:need(meta(x['command'][0])=={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')},'actual media CLI byte identity')
    doc=get('dump.stdout');need(doc==get('portable-dump.stdout') and get('validate.stdout')==get('portable-validate.stdout') and get('validate.stdout')['valid'] is True,'exact portable authored document/real Store validation')
    kind=doc['nodes'][doc['root']]['kind'];need(kind['type']=='sequence' and len(kind['children'])==1,'ordinary root')
    owner=kind['children'][0];node=doc['nodes'][owner];retime=node['kind']
    need(retime==dict(type='retime',child=retime['child'],duration=120,mapping=dict(start=0,end=80),pitch='follow_speed'),'actual nonunit2/3 Retime tape serde')
    child=doc['nodes'][retime['child']]['kind'];need(child['type']=='sequence' and len(child['children'])==2 and not any(x['kind']['type']=='repeat' for x in doc['nodes'].values()),'actual two input fragments')
    from fractions import Fraction as F
    def ratio(x):return F(int(x['numerator']),int(x['denominator']))
    def duration(i):
        x=doc['nodes'][i]['kind']
        if x['type']=='source':return x['source']['duration']
        if x['type']=='retime':return x['duration']
        need(x['type']=='sequence','ordinary input tree');return sum(duration(c) for c in x['children'])
    need([duration(i) for i in child['children']]==[40,40] and duration(retime['child'])==80 and duration(doc['root'])==120,'input80/output120')
    framing=node['framing'];f=framing['value']
    need('clock' not in framing and f['type']=='follow' and f['target']=='target-1' and f['scale']==f['fallback']['scale']==dict(numerator='11',denominator='10') and f['fallback']['center_x']==dict(numerator='311385129',denominator='536870912') and f['scale_transition']==dict(factor=dict(numerator='2',denominator='1'),curve='linear') and all(not n.get('framing') for i,n in doc['nodes'].items() if i!=owner),'single owner-output Follow saved scales and independent child framing')
    target=doc['targets']['target-1'];clock=dict(numerator=1,denominator=30000)
    need(target['region']==dict(center=[580000,500000]) and target['corrections']==[dict(at=80080,region=dict(center=[680000,500000]))] and not target.get('samples',[]),'actual manually corrected Original target')
    def bounds(i,start,end):
        x=doc['nodes'][i]['kind']
        if x['type']=='retime':
            need(x['purpose']=='partition' and x['pitch']=='preserve' and F(x['mapping']['end']-x['mapping']['start'],x['duration'])==1,'unity partition only')
            return bounds(x['child'],x['mapping']['start']+start,x['mapping']['start']+end)
        need(x['type']=='source','leaf Source');src=x['source'];v=src['video'];mapping=src['video_mapping']
        need(v['type']=='stream' and v['asset']==target['asset'] and v['span']['start']['time_base']==v['span']['end']['time_base']==clock,'actual source identity/timebase')
        if mapping['type']=='fit_beat':placement,frames=F(0),F(src['duration'])
        else:
            need(mapping['type'] in ('duration','placement','selected_placement'),'source mapping type')
            placement=ratio(mapping['start']) if 'start' in mapping else F(0);frames=ratio(mapping['frames'])
        origin=F(v['span']['start']['ticks'],1001);length=F(v['span']['end']['ticks']-v['span']['start']['ticks'],1001)
        return origin+(start-placement)*length/frames,origin+(end-placement)*length/frames
    actual_ranges=[bounds(i,F(0),F(40)) for i in child['children']]
    need(actual_ranges==[(F(0),F(40)),(F(80),F(120))],'actual retained Original ranges')
    v=get('verification.json')
    need(v==get('verify.stdout') and v['passed'] is True and v['failures']==[] and v['revision_id']==doc['revision_id'] and v['range']==[0,120] and v['frame_rate']==[30000,1001],'actual complete emitted verification')
    need(len(v['pictures'])==120 and [p['output_frame'] for p in v['pictures']]==list(range(120)) and all(p['passed'] is True and p['flags']==[] for p in v['pictures']),'all120 picture results')
    fractional=0;cut=[]
    for j,p in enumerate(v['pictures']):
        q=F(2*j+1,3);o=q+(40 if j>=60 else 0);ordinal=o.numerator//o.denominator
        fractional+=o.denominator!=1
        need(p['project_frame']==j and p['provenance']==dict(kind='original',asset=target['asset'],source_frame=ordinal,source_pts=ordinal*1001),'all120 independently mapped Original ordinal/PTS')
        if j in (59,60):cut.append(dict(edit=j,exact_original=str(o),decoded_ordinal=ordinal))
    need(fractional==80 and [x['decoded_ordinal'] for x in cut]==[39,80],'80 fractional providers and exact mapped cut')
    obs=collections.Counter(p[k]['status'] for p in v['pictures'] for k in ('previous_index','next_index') if p[k] is not None)
    need(obs==dict(observable=150,unobservable=88) and v['summary']['index_observable_comparisons']==150 and v['summary']['index_unobservable_comparisons']==88,'observability limits preserved')
    a=v['audio'];need(len(a)==1 and a[0]['passed'] is True and a[0]['flags']==[] and a[0]['window']==a[0]['project_samples']==[0,192192] and a[0]['uncovered_samples']==0 and a[0]['offset_status']=='verified_zero' and a[0]['measured_offset_samples']==0 and all(x['peak_lag']==0 and x['status']=='verified_zero' for x in a[0]['segments']),'full192192 audio zero offset/uncovered')
    movie=dict(bytes=len(m['retime-follow-creep.mp4']),sha256=sha(m['retime-follow-creep.mp4']))
    need(movie==prior['media']['movie']==dict(bytes=172802,sha256='6fac1365b2ce192eebe16dccf01883af4041e9d4c1d238d5dc09d689afb3f4f1'),'actual MP4 archived bytes')
    mm=v['movie'];need(mm==prior['media']['movie_metadata'] and mm['video_frames']==mm['expected_video_frames']==120 and mm['picture_anomalies']==[] and mm['audio']==dict(first_pts=-2048,physical_samples=194560,presented_end=192192,gaps=[]) and mm['expected_audio_samples']==192192 and mm['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=192192) and mm['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=120120),'actual AAC/video edit lists and complete clocks')
    pubs=[d for line in m['render.stdout'].splitlines() for d in dicts(json.loads(line)) if 'movie_sha256' in d]
    need(pubs and all(x==pubs[0] for x in pubs) and pubs[0]['movie_sha256']==movie['sha256'] and pubs[0]['movie_bytes']==movie['bytes'],'actual render publication')
    need(dict(bytes=len(m[Path(pubs[0]['report']).name]),sha256=sha(m[Path(pubs[0]['report']).name]))==dict(bytes=pubs[0]['report_bytes'],sha256=pubs[0]['report_sha256']),'actual retained render report identity')
    need(get('copy.stdout')['portable_copy']['verified_history']==dict(replayed=0,revisions=21,verified_by_receipt=21),'complete portable21 revision receipt')
    roots={a['path']:Path(a['root']) for a in aa}
    histories=[]
    for archive,key in [('archives/actual-native-retime-project.tar.gz','project.sqlite'),('archives/final-native-retime-project-and-export.tar.gz','portable.deadpan/project.sqlite')]:
        members=archives[archive];db=roots[archive]/key;wal=db.with_name(db.name+'-wal')
        need(not wal.exists() or meta(wal)['bytes']==0,'empty WAL before immutable SQLite')
        before_db=meta(db);need(before_db==dict(bytes=len(members[key]),sha256=sha(members[key])),'actual retained database bytes')
        with sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True) as c:
            need(c.execute('pragma quick_check').fetchall()==[('ok',)],'immutable SQLite integrity')
            h={t:sorted(c.execute('SELECT * FROM "'+t+'"').fetchall(),key=repr) for t in ('revisions','revision_patches','history','history_receipt','redo','state','transaction_steps')}
        need(meta(db)==before_db,'SQLite byte stability')
        histories.append(h)
    need(histories[0]==histories[1] and len(histories[0]['revisions'])==21 and len(histories[0]['history'])==13,'full native/portable selected history tables equal21/13')
    result.update(status='no_findings',findings=[],inventory_sha256=INV,primary_files=len(before),payloads=len(inv),payload_bytes=sum(x['bytes'] for x in inv.values()),primary_bytes_including_inventory=sum(x['bytes'] for x in before.values()),encoding_records=len(rows),encodings=dict(counts),gzip_streams=counts['gzip']+counts['tar+gzip'],encoded_ledger_stored_bytes=sum(x['stored_bytes'] for x in rows.values()),decoded_ledger_bytes=sum(x['decoded_bytes'] for x in rows.values()),raw_regular_bytes=raw_regular,raw_archive_source_bytes=archive_bytes,raw_payload_bytes=raw_regular+archive_bytes,budget_bytes=LIMIT,archives=summaries,tar_members=sum(len(x) for x in archives.values()),tar_member_bytes=archive_bytes,metadata_omissions=0,authored_omissions=0,final_source_paths=17,embedded_raw_audit_sha256=AUDIT,first_audit_schema_stop_preserved=True,passing_retry_preserved=True,selected_pngs=pngs,native=natives,raw_tests=raw_tests,actual_media_commands=9,movie=movie,picture_rows=120,picture_neighbor_observability=dict(obs),fractional_provider_coordinates=80,exact_mapped_cut=cut,audio_window=[0,192192],audio_offset=0,native_portable_history_counts={k:len(x) for k,x in histories[0].items()},native_portable_all_selected_history_tables_equal=True,app_unit_live_terminal_matched=True,collector_sha256=collection['script_sha256'],plan_sha256=collection['plan_sha256'])
except Exception as exc:
    traceback.print_exc();result['status']='findings';result['findings'].append(dict(type=type(exc).__name__,message=str(exc)))
finally:
    after=tree(P);old_after={n:meta(S/n) for n in old_names}
    result['primary_tree_unchanged']=before==after;result['inventory_unchanged']=before['files.json']==after['files.json']
    result['earlier_audit_files_unchanged']=old_before==old_after;result['first_audit_history_unchanged']=history_before==tree(S/'audit-history')
    result['current_scoped_sources_unchanged']=source_before=={p:meta(R/p) for p in source_before}
    if not all(result[k] for k in ('primary_tree_unchanged','inventory_unchanged','earlier_audit_files_unchanged','first_audit_history_unchanged','current_scoped_sources_unchanged')):
        result['status']='findings';result['findings'].append(dict(message='Primary, audit history or scoped source changed'))
    result['prior_audit_file_pins']=old_before;result['verifier_sha256']=meta(Path(__file__))['sha256'];result['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S/'package-verification.json').open('x') as f:json.dump(result,f,indent=2,sort_keys=True);f.write('\n')
    print(json.dumps({k:result.get(k) for k in ('status','findings','primary_files','payloads','payload_bytes','inventory_sha256','raw_payload_bytes','tar_members','authored_omissions','metadata_omissions','primary_tree_unchanged','earlier_audit_files_unchanged')},sort_keys=True))
sys.exit(0 if result['status']=='no_findings' else 1)
