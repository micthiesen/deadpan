#!/usr/bin/env python3
"""Independent bounded offline reader of the published Repeat qualification pack."""
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

S = Path("/tmp/deadpan-repeat-follow-creep-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
P = R / "tools/media-qualification/evidence/2026-10-24-repeat-follow-creep"
INV = "dcee080990be595f9826917c3d55dceee8a402600432f55b9804085710db049d"
AUDIT = "7b01c81a5c2a3efcdc70b05931c8752d621dd2c7adc5739214b3e4057a7515b8"
LIMIT = 268435456
result = {"schema_version": 1, "status": "running", "findings": [],
          "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "limitations": [
              "No collector, payload script, product, Cargo, test, worker, decoder, model or Git command executed.",
              "Twenty-one scoped sources and relevant historical subsets are checked, not compilation of every repository file.",
              "Root's image review is bound by bytes; PNG integrity is not independent visual or physical-display qualification.",
              "Shared picture/limited-audio references do not establish independent DSP, listening, encoded bit equality, tracking accuracy or latency/requirement completion.",
              "The old live-clipping helper failure and pre-spawn wrapper failure remain explicit. Derived clipping wrapper exited1 after181 successful product commands; separate offline reconciliation permits only newly created empty WAL. App unit hash was terminal-only.",
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


old_names=['audit-repeat-evidence.py','audit-repeat-evidence.json','audit-repeat-evidence.stdout','audit-repeat-evidence.stderr','clipped-media-reconciliation.py','clipped-media-reconciliation.json']
old_before={n:meta(S/n) for n in old_names}
before=tree(P);source_before={}
try:
    need(before['files.json']['sha256']==INV,'exact inventory pin')
    inv=json.loads((P/'files.json').read_bytes())
    need(len(inv)==165 and set(before)==set(inv)|{'files.json'},'strict inventory membership')
    need(sum(m['bytes'] for m in inv.values())==11068281,'stored payload bytes')
    for n,m in inv.items():safe(n);need(before[n]==m,'inventoried exact bytes')
    ledger=json.loads((P/'encoding-ledger.json').read_bytes())
    need(isinstance(ledger,list) and len(ledger)==162,'actual encoding ledger schema')
    rows,data,counts={}, {}, collections.Counter();raw_regular=0
    for row in ledger:
        n=row['path'];safe(n);need(n not in rows and n in inv,'unique inventoried encoding')
        b=(P/n).read_bytes()
        need(len(b)==row['stored_bytes'] and sha(b)==row['stored_sha256'],'stored encoding identity')
        need(row['encoding'] in ('identity','gzip','tar+gzip'),'known encoding')
        raw=b if row['encoding']=='identity' else gunzip(b)
        need(len(raw)==row['decoded_bytes'] and sha(raw)==row['decoded_sha256'],'all decoded ledger identities')
        rows[n],data[n]=row,raw;counts[row['encoding']]+=1
        if row['encoding']!='tar+gzip':
            need(meta(row['source'])==dict(bytes=len(raw),sha256=sha(raw)),'actual decoded source bytes')
            need(raw[:4] not in {b'\xfe\xed\xfa\xce',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xcf\xfa\xed\xfe',b'\xca\xfe\xba\xbe',b'\x7fELF'},'no binary payload bytes')
            raw_regular+=len(raw)
    need(counts==dict(identity=128,gzip=26,**{'tar+gzip':8}),'encoding totals')
    need(set(inv)==set(rows)|{'collection.json','encoding-ledger.json','archive-ledger.json'},'complete encoding coverage')
    def scratch(n):
        hits=[p for p,r in rows.items() if Path(r['source']).resolve()==(S/n).resolve()]
        need(len(hits)==1,f'one retained scratch counterpart:{n}')
        return data[hits[0]]
    def obj(n):return json.loads(scratch(n))
    aa=json.loads((P/'archive-ledger.json').read_bytes())
    need(isinstance(aa,list) and len(aa)==8,'archive ledger eight')
    archives,summaries={},[];archive_source_bytes=0;archive_member_bytes=0;all_omissions={}
    expected_omissions={
        'archives/actual-native-repeat-project.tar.gz':{'project.sqlite-wal','project.sqlite-shm'},
        'archives/clipped-media-final.tar.gz':{'cli-derived-clipped.deadpan/project.sqlite-wal','cli-derived-clipped.deadpan/project.sqlite-shm'}}
    for a in aa:
        expected={m['path']:dict(bytes=m['bytes'],sha256=m['sha256']) for m in a['members']}
        omitted=a['omitted_sidecars']
        need(len(expected)==len(a['members']) and set(omitted)==expected_omissions.get(a['path'],set()),'exact member/omission path sets')
        native_worker=a['path']=='archives/native-worker-source.tar.gz'
        members,order={},[]
        with tarfile.open(fileobj=io.BytesIO(data[a['path']]),mode='r:') as tf:
            for m in tf:
                safe(m.name,allow_rust=native_worker)
                need(m.isfile() and not m.issparse() and 0<=m.size<=LIMIT and m.name not in members and m.name in expected,'safe bounded unique regular TAR')
                need((m.uid,m.gid,m.uname,m.gname,m.mode,m.mtime)==(0,0,'','',0o644,0),'deterministic TAR metadata')
                need(set(m.pax_headers)<={'path'} and ('path' not in m.pax_headers or m.pax_headers['path']==m.name),'bounded safe PAX')
                b=tf.extractfile(m).read(LIMIT+1)
                need(len(b)==m.size and dict(bytes=len(b),sha256=sha(b))==expected[m.name],'every TAR member hash/bytes')
                need(b[:4] not in {b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\x7fELF'},'no executable TAR member')
                members[m.name]=b;order.append(m.name)
        need(order==sorted(order) and set(members)==set(expected),'complete sorted TAR path set')
        original=tree(a['root'])
        need(set(original)==set(expected)|set(omitted),'complete original tree with only exact metadata omissions')
        for n,m in expected.items():need(original[n]==m,'retained original TAR source bytes')
        for n,m in omitted.items():
            pin={k:m[k] for k in ('bytes','sha256')}
            need(original[n]==pin and 'paired SQLite WAL observed empty' in m['reason'],'omitted sidecar exact byte/hash/reason')
            base=n[:-4]
            need(base in expected and base+'-wal' in omitted and base+'-shm' in omitted,'omissions paired with retained actual database')
            need(omitted[base+'-wal']['bytes']==0 and omitted[base+'-wal']['sha256']==sha(b'') and omitted[base+'-shm']['bytes']==32768,'only empty WAL/bounded SHM omitted')
        need(len(original)==a['source_files'] and sum(m['bytes'] for m in original.values())==a['source_bytes'],'full archive source raw charge')
        retained_sidecars=[]
        for n in members:
            if n.endswith(('-wal','-shm')):
                base=n[:-4];need(base in members and base+'-wal' in members and base+'-shm' in members,'paired retained checkpoint sidecars')
                need(len(members[base+'-wal'])==0 and len(members[base+'-shm'])==32768,'retained checkpoint sidecar bounds')
                retained_sidecars.append(n)
        archives[a['path']]=members;archive_source_bytes+=a['source_bytes'];archive_member_bytes+=sum(m['bytes'] for m in expected.values())
        all_omissions[a['path']]=omitted
        summaries.append(dict(path=a['path'],members=len(members),retained_member_bytes=sum(m['bytes'] for m in expected.values()),source_files=a['source_files'],source_bytes=a['source_bytes'],metadata_omissions=omitted,retained_sidecars=retained_sidecars,archived_rust_named_snapshot_members=[n for n in members if n.endswith('.rs')]))
    need([len(archives[a['path']]) for a in aa]==[21,7,7,5,5,30,13,385] and sum(len(x) for x in archives.values())==473 and archive_member_bytes==8852173,'archive totals')
    collection=json.loads((P/'collection.json').read_bytes());plan=json.loads(data['packaging/collect-plan.json'])
    need(raw_regular+archive_source_bytes==collection['raw_payload_bytes']==156028443 and collection['raw_payload_bytes']<=plan['raw_byte_limit']==LIMIT,'cumulative raw charge includes omitted metadata')
    need(sha(data['packaging/collect-plan.json'])==collection['plan_sha256']=='29092583575de1907fcf3987169497a3d551989b53023139e538969ed06afc83','frozen plan bytes')
    need(sha(data['packaging/collect.py.txt'])==collection['script_sha256']=='320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e','collector bytes')
    need(sha(data['packaging/bundle-helpers.py.txt'])=='2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13','helper bytes')
    prior=obj('audit-repeat-evidence.json')
    need(sha(scratch('audit-repeat-evidence.json'))==AUDIT==old_before['audit-repeat-evidence.json']['sha256'] and prior['status']=='no_findings' and prior['findings']==[],'embedded frozen raw audit')
    # Every old audited file retained either directly or inside an archive is still exact.
    retained_by_source={}
    for n,r in rows.items():
        if r['encoding']!='tar+gzip':retained_by_source.setdefault(str(Path(r['source']).resolve()),[]).append(dict(bytes=len(data[n]),sha256=sha(data[n])))
    for a in aa:
        for n,b in archives[a['path']].items():retained_by_source.setdefault(str((Path(a['root'])/n).resolve()),[]).append(dict(bytes=len(b),sha256=sha(b)))
    for p,m in prior['input_file_pins'].items():
        need(all(x==m for x in retained_by_source.get(str(Path(p).resolve()),[])),'raw audit covered retained byte identity')
    final,first=obj('final-identities.json'),obj('first-identities.json')
    need(final['sources']==first['sources']==prior['final_sources'] and len(final['sources'])==21,'one candidate final21 scope')
    source_before={p:meta(R/p) for p in final['sources']};need(source_before==final['sources'],'current final21')
    for p,m in final['sources'].items():
        hits=[n for n,r in rows.items() if r['source']==str(R/p) and n.startswith('source/')]
        need(len(hits)==1 and dict(bytes=len(data[hits[0]]),sha256=sha(data[hits[0]]))==m and meta(S/'source-final'/(p+'.txt'))==m,'final source snapshot identity')
    need({n[:-4]:dict(bytes=len(b),sha256=sha(b)) for n,b in archives['archives/first-source.tar.gz'].items()}==first['sources'],'archived first21 map')
    worker=obj('worker-repeat-final-freeze.json')
    for archive in ('archives/worker-core-source.tar.gz','archives/worker-cli-source.tar.gz'):
        need({n[:-4]:sha(b) for n,b in archives[archive].items()}==worker['files'],'all seven focused worker snapshots')
    native_worker=obj('native-harness-freeze.json')
    need({x['path']:dict(bytes=x['bytes'],sha256=x['sha256']) for x in native_worker['changed_paths']}=={n:dict(bytes=len(b),sha256=sha(b)) for n,b in archives['archives/native-worker-source.tar.gz'].items()},'five archived native worker snapshots remain data only')
    for ids in (first,final):
        for n,m in ids['binaries'].items():need(meta(m['executed'])=={k:m[k] for k in ('bytes','sha256')},'retained executed sibling bytes')
    need(all({k:first['binaries'][n][k] for k in ('bytes','sha256')}=={k:final['binaries'][n][k] for k in ('bytes','sha256')} for n in final['binaries']),'all five binary identities equal')
    eq=obj('candidate-equivalence.json');need(eq['source_paths']==21 and eq['all_source_bytes_identical'] is True and eq['all_five_binary_bytes_identical'] is True,'one candidate provenance')
    for p,m in plan['inputs'].items():need(meta(R/p)==m,'collected build/document/fixture input')
    unit=obj('app-unit-executable-terminal.json');need(unit==prior['unit_terminal_observation'] and meta(unit['path'])=={k:unit[k] for k in ('bytes','sha256')} and 'No live hash' in unit['scope'],'actual terminal-only App unit proof')
    raw_tests={}
    for n,expected in [('core-plan-final',(2053,0,0)),('app-units-final',(1304,0,11)),('worker-core-1',(2,0,0)),('worker-cli-1',(14,0,0)),('plan-focused-1',(3,0,0)),('live-clip-before',(0,1,0)),('live-clip-fixed',(3,0,0))]:
        t=scratch(n+'.stdout').decode();ss=re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',t,re.M)
        total=tuple(sum(int(x[i]) for x in ss) for i in range(3));need(total==expected,'raw named test summary totals')
        need(len(re.findall(r'^test .+ \.\.\. ok$',t,re.M))==total[0] and len(re.findall(r'^test .+ \.\.\. ignored(?:,.*)?$',t,re.M))==total[2],'actual named passed/ignored counts')
        need(obj(n+'.json')['exit_code']==(101 if n=='live-clip-before' else 0),'actual terminal result code')
        raw_tests[n]=list(total)
    full_names=re.findall(r'^test (.+) \.\.\. ok$',scratch('core-plan-final.stdout').decode(),re.M)
    for n in re.findall(r'^test (.+) \.\.\. ok$',scratch('plan-focused-1.stdout').decode(),re.M)+re.findall(r'^test (.+) \.\.\. ok$',scratch('worker-core-1.stdout').decode(),re.M):need(full_names.count(n)==1,'exact focused tests in final broad suite')
    note=obj('camera-help-source-note.json');camera='crates/deadpan-app/src/preview/camera.rs'
    old=scratch('camera-before-live-clip-help.rs.txt');current=(R/camera).read_bytes()
    need(old.count(b'when clipped by a captured view')==2 and old.replace(b'when clipped by a captured view',b'when an earlier crop hides it')==current,'early Camera two-string-only scope')
    for n in ('worker-core-1','worker-cli-1'):
        d=obj(n+'.json');need(d['owned_unchanged'] is True and d['basis_unchanged'] is True and d['owned_before']==d['owned_after'] and d['basis_before']==d['basis_after'],'focused own/basis guards')
        need(all(meta(R/p)['sha256']==h for p,h in d['basis_before'].items()),'focused current production basis')
        need({p:h for p,h in d['global_source_before'].items() if final['sources'][p]['sha256']!=h}=={camera:note['before']['sha256']},'early focused global scope only hint changes')
        for suffix,h in d['stream_pins'].items():need(sha(scratch(n+suffix))==h,'focused stream pins')
    pd=obj('plan-focused-1.json');need(pd['main_manifest_before']==pd['main_manifest_after']==final['sources'] and pd['source_before']==pd['source_after'] and pd['basis_before']==pd['basis_after'] and all(meta(R/p)==m for p,m in pd['basis_before'].items()),'Plan actual final source basis')
    for n in ('core-plan-final','app-units-final','release-final','clippy-first','format-final','app-check-first','native-repeat-run','native-regressions','native-zoom-final','native-hold-follow-creep-final','media-final'):
        r=obj(n+'.json');need(r['exit_code']==0 and r['source_before']==r['source_after']=={str(R/p):m for p,m in final['sources'].items()} and r['sources_unchanged'] is True and r['binary_before']==r['binary_after'] and r['binaries_unchanged'] is True,'final21 terminal source/binary guards')
        for p,m in r['streams'].items():need(dict(bytes=len(scratch(Path(p).name)),sha256=sha(scratch(Path(p).name)))==m,'terminal raw stream counterpart')
    need(obj('commands-write.json')['exit_code']==101,'intentional generated reference writer exit')
    launch=obj('native-repeat-first.json');need('pid' not in launch and 'exit_code' not in launch and launch['command'][0]=='--binary' and scratch('native-repeat-first.stdout')==scratch('native-repeat-first.stderr')==b'','preserved pre-spawn wrapper failure')
    need(obj('native-regressions-scope-note.json')['actual_scope']==['sequence-follow-creep','kestrel-shortcuts'],'actual regression scenario only')
    before_members=archives['archives/before-live-clip-fix.tar.gz'];need(len(before_members)==5 and sha(before_members['crates/deadpan-plan/src/plan.rs.txt'])=='65757b3999ce03d8760960af1ee1f8d1740932ef4dcc5656885e56886211579d','actual pre-fix helper source retained')
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
    for n,scenario,count,wc in [('native-repeat-run','repeat-follow-creep',129,3),('native-regressions','sequence-follow-creep',96,3),('native-zoom-final','zoom',213,3),('native-hold-follow-creep-final','hold-follow-creep',67,2)]:
        report=obj(n+'/report.json');scenes={x['name']:x for x in report['scenarios']}
        need(report['failed'] is False and report['metadata']['build_profile']=='release' and report['metadata']['physical_display_measured'] is False and report['metadata']['binary_sha256']==final['binaries']['deadpan-app']['sha256'] and set(scenes)=={scenario,'kestrel-shortcuts'} and len(scenes[scenario]['checks'])==count,'native pass/current App/scenario count')
        need(all(c['passed'] is True for x in scenes.values() for c in x['checks']) and not any(f['severity']=='failure' for x in scenes.values() for f in x['findings']),'raw native successes')
        k=scenes['kestrel-shortcuts']['checks'];need(len(k)==1 and k[0]['actual']['reserved_bindings']==62 and k[0]['actual']['routing_cases']==21885504 and k[0]['actual']['conflicts']==[],'actual Kestrel scope')
        prev=next(x for x in prior['native_reports'] if x['name']==n)
        need(scenes[scenario]['findings']==prev['findings'] and scenes[scenario]['skipped']==prev['skipped'] and sum(f['severity']=='warning' for f in prev['findings'])==wc,'all warnings/skips preserved')
        natives.append(dict(name=n,checks=count,kestrel=1,warnings=wc,skips=prev['skipped']))
    # Media and authoritative databases are verified from actual archived bytes.
    native=archives['archives/final-native-repeat-project-and-export.tar.gz']
    clipped=archives['archives/clipped-media-final.tar.gz']
    original=archives['archives/actual-native-repeat-project.tar.gz']
    def get(m,n):return json.loads(m[n])
    def media_check(m,movie_name,expected_movie,observations,command_key='command'):
        v=get(m,'verification.json');need(v==get(m,'verify.stdout') and v['passed'] is True and v['failures']==[] and v['range']==[0,80] and v['frame_rate']==[30000,1001],'actual verified complete emitted file')
        need(len(v['pictures'])==80 and all(p['passed'] is True and p['flags']==[] for p in v['pictures']) and [p['output_frame'] for p in v['pictures']]==list(range(80)),'all80 encoded picture rows')
        observed=collections.Counter(p[k]['status'] for p in v['pictures'] for k in ('previous_index','next_index') if p[k] is not None)
        need(observed==observations and v['summary']['index_observable_comparisons']==observations['observable'] and v['summary']['index_unobservable_comparisons']==observations['unobservable'],'raw observability limits')
        a=v['audio'];need(len(a)==1 and a[0]['window']==a[0]['project_samples']==[0,128128] and a[0]['passed'] is True and a[0]['flags']==[] and a[0]['uncovered_samples']==0 and a[0]['offset_status']=='verified_zero' and a[0]['measured_offset_samples']==0 and all(x['peak_lag']==0 and x['status']=='verified_zero' for x in a[0]['segments']),'full128128 zero-offset audio')
        movie=dict(bytes=len(m[movie_name]),sha256=sha(m[movie_name]));need(movie==expected_movie,'actual archived MP4')
        mm=v['movie'];need(mm['bytes']==movie['bytes'] and mm['video_frames']==mm['expected_video_frames']==80 and mm['picture_anomalies']==[] and mm['audio']['gaps']==[] and mm['audio']['presented_end']==mm['expected_audio_samples']==128128 and mm['audio']['first_pts']==-2048 and mm['audio']['physical_samples']==131072 and mm['audio_edits']==dict(edits=1,empty_edits=0,media_time=2048,presented_media_ticks=128128) and mm['video_edits']==dict(edits=1,empty_edits=0,media_time=0,presented_media_ticks=80080),'actual AAC/video edit metadata')
        pubs=[d for line in m['render.stdout'].splitlines() for d in dicts(json.loads(line)) if 'movie_sha256' in d]
        need(pubs and all(x==pubs[0] for x in pubs) and pubs[0]['movie_sha256']==movie['sha256'] and pubs[0]['movie_bytes']==movie['bytes'],'actual render publication')
        need(dict(bytes=len(m[Path(pubs[0]['report']).name]),sha256=sha(m[Path(pubs[0]['report']).name]))==dict(bytes=pubs[0]['report_bytes'],sha256=pubs[0]['report_sha256']),'raw emitted render report')
        return dict(movie=movie,pictures=80,audio_window=[0,128128],audio_offset=0,observability=dict(observed),movie_metadata=mm)
    native_media=media_check(native,'repeat-follow-creep.mp4',prior['media_native']['movie'],dict(observable=114,unobservable=44))
    cmds=get(native,'commands.json');need(len(cmds)==9 and [c['name'] for c in cmds]==['dump','validate','render','verify','copy','portable-validate','portable-dump','checkpoint','portable-checkpoint'] and all(c['exit_code']==0 for c in cmds),'native nine real CLI commands')
    for c in cmds:need(meta(c['command'][0])=={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')},'native actual CLI identity')
    need(get(native,'dump.stdout')==get(native,'portable-dump.stdout') and get(native,'validate.stdout')==get(native,'portable-validate.stdout') and get(native,'validate.stdout')['valid'] is True,'native portable document and Store validation')
    need(get(native,'copy.stdout')['portable_copy']['verified_history']==dict(replayed=0,revisions=20,verified_by_receipt=20),'native complete history receipt20')
    derived_media=media_check(clipped,'cli-derived-clipped-repeat.mp4',prior['clipped_media']['movie'],dict(observable=104,unobservable=54))
    ccmds=get(clipped,'commands.json');need(len(ccmds)==181 and len({c['name'] for c in ccmds})==181 and all(c['exit_code']==0 for c in ccmds),'181 exact product commands allzero')
    for c in ccmds:
        need(meta(c['argv'][0])=={k:final['binaries']['deadpan-cli'][k] for k in ('bytes','sha256')},'derived actual CLI')
        for suffix in ('stdout','stderr'):need(dict(bytes=len(clipped[c['name']+'.'+suffix]),sha256=sha(clipped[c['name']+'.'+suffix]))==c[suffix],'all362 clipped raw streams pinned')
    failed=obj('clipped-media-final.json');term=get(clipped,'terminal.json');inputs=get(clipped,'inputs.json')
    need(failed['exit_code']==1 and failed['source_before']==failed['source_after']=={str(R/p):m for p,m in final['sources'].items()} and failed['binaries_unchanged'] is True and term['status']=='failed' and term['commands']==181 and 'result.json' not in clipped,'actual original guard exit1 unchanged')
    for p,m in failed['streams'].items():need(dict(bytes=len(scratch(Path(p).name)),sha256=sha(scratch(Path(p).name)))==m,'original failed wrapper streams unchanged')
    need(get(clipped,'failure.json')['error']=="AssertionError('persistent native package bytes changed')",'actual late guard error')
    b,e=inputs['native_authoritative_before'],term['native_authoritative_after'];empty=dict(bytes=0,sha256=sha(b''))
    need(set(e)-set(b)=={'project.sqlite-wal'} and e['project.sqlite-wal']==empty and all(e[n]==m for n,m in b.items()),'only allowed absent-to-empty WAL delta')
    recon=obj('clipped-media-reconciliation.json')
    need(sha(scratch('clipped-media-reconciliation.json'))=='fbb489090c84c6445dc06e35d5eb35be0a3105bb7649324f632da5776d729d8e' and recon['reconciliation_status']=='passed_with_original_guard_failure_preserved' and recon['original_run']['exit_code']==1 and recon['original_run']['failure_verbatim']==clipped['failure.json'].decode() and recon['original_run']['outer_stderr_verbatim']==scratch('clipped-media-final.stderr').decode(),'separate frozen offline reconciliation')
    need(dict(bytes=len(scratch('clipped-media-reconciliation.py')),sha256=sha(scratch('clipped-media-reconciliation.py')))==recon['reconciler'],'reconciliation script bytes')
    need(get(clipped,'native-document-before.stdout')==get(clipped,'native-document-after.stdout')==get(native,'dump.stdout')==get(clipped,'copy-document-before.stdout'),'native document unchanged through derived copy')
    from copy import deepcopy
    old=get(clipped,'copy-document-before.stdout');new=get(clipped,'copy-document-after.stdout');expected=deepcopy(old);expected['revision_id']=new['revision_id'];expected['targets']['target-1']['corrections']=[dict(at=12012,region=dict(center=[980000,500000]))]
    need(new==expected==get(clipped,'portable-dump.stdout'),'only target correction+revision changed')
    def sans_revision(x):
        x=deepcopy(x);x.pop('revision_id',None);return x
    need(sans_revision(get(clipped,'portable-undone-dump.stdout'))==sans_revision(old) and sans_revision(get(clipped,'portable-redone-dump.stdout'))==sans_revision(new),'derived portable Undo/Redo actual complete document')
    need(get(clipped,'portable-validate.stdout')['valid'] is True and get(clipped,'portable-validate.stdout')['revision_id']==get(clipped,'portable-redone-dump.stdout')['revision_id'],'derived real final Store validation')
    need(get(clipped,'copy-derived.stdout')['portable_copy']['verified_history']==dict(replayed=0,revisions=21,verified_by_receipt=21),'derived copied history21')
    # Recompute every retained per-frame clipping result without running a producer.
    from fractions import Fraction as F
    def ratio(x):return F(int(x['numerator']),int(x['denominator']))
    Q=1<<32
    def grid(x):return F(round(x*Q),Q)
    owner=next(n for n,v in old['nodes'].items() if v['kind']['type']=='repeat')
    fallback=old['nodes'][owner]['framing']['value']['fallback']
    observations=get(clipped,'independent-plan-observations.json');need(len(observations)==80,'all80 pose observations')
    clipped_frames=[]
    for j in range(80):
        before_sample=get(clipped,f'frame-before-{j:03}.stdout')['sample'];sample=get(clipped,f'frame-after-{j:03}.stdout')['sample']
        play,local=divmod(j,28);gap=local>=24;ordinal=23 if gap else local;percent=58 if ordinal<12 else 98
        projected=F(1,2)+(F(percent,100)-F(1,2))*F(8+play,8);visible=0<=projected<=1
        if not visible:clipped_frames.append(j)
        layer=[x for x in sample['framing'] if x['instance']['node']==owner and not x['escalation']]
        need(len(layer)==1,'one exact outer layer');layer=layer[0]
        center=grid(projected) if visible else ratio(fallback['center_x'])
        start,end=grid(F(11,10)),grid(F(11,5));scale=grid(start+(end-start)*grid(F(2*j+1,160)))
        need(ratio(layer['pose']['center_x'])==center and ratio(layer['pose']['center_y'])==F(1,2) and ratio(layer['pose']['scale'])==scale and ratio(layer['local_position'])==F(2*j+1,2) and ratio(layer['effective_duration'])==80,'independent Q32 center/clock/creep')
        obs=observations[j];need(obs['Edit']==j and obs['Original']==ordinal and obs['gap']==gap and obs['visible']==visible and obs['actual_pose']==layer['pose'] and ratio(obs['expected_center'])==center,'raw observation counterpart')
        x,y=sans_revision(before_sample),sans_revision(sample)
        for value in (x,y):
            for v in value['framing']:
                if v['instance']['node']==owner and not v['escalation']:v.pop('pose')
        need(x==y,'all nonouter sample fields identical')
    need(clipped_frames==list(range(40,56))+list(range(68,80))==recon['recovered_result']['clipped_frames'],'exact28 clipped frames')
    need(sans_revision(get(clipped,'audio-plan-before.stdout')['audio'])==sans_revision(get(clipped,'audio-plan-after.stdout')['audio']),'complete before/after audio plan')
    # Archived database bytes equal existing originals. Open only immutable read-only.
    roots={a['path']:Path(a['root']) for a in aa}
    def sql(archive,key):
        members=archives[archive];db=roots[archive]/key;wal=db.with_name(db.name+'-wal')
        need(not wal.exists() or meta(wal)['bytes']==0,'empty WAL before immutable read')
        m=meta(db);need(m==dict(bytes=len(members[key]),sha256=sha(members[key])),'actual SQLite retained byte-for-byte')
        with sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True) as c:
            need(c.execute('pragma quick_check').fetchall()==[('ok',)],'immutable SQLite integrity')
            tables={t:sorted(c.execute('SELECT * FROM "'+t+'"').fetchall(),key=repr) for t in ('revisions','revision_patches','history','history_receipt','redo','state','transaction_steps')}
        need(meta(db)==m,'SQLite unchanged')
        return tables
    hnative=sql('archives/actual-native-repeat-project.tar.gz','project.sqlite')
    hcopy=sql('archives/final-native-repeat-project-and-export.tar.gz','portable.deadpan/project.sqlite')
    need(hnative==hcopy and len(hnative['revisions'])==20 and len(hnative['history'])==12,'complete native/copied history20/12')
    hd=sql('archives/clipped-media-final.tar.gz','cli-derived-clipped.deadpan/project.sqlite')
    hp=sql('archives/clipped-media-final.tar.gz','cli-derived-history-portable.deadpan/project.sqlite')
    need(len(hd['revisions'])==21 and len(hp['revisions'])==23 and len(hd['history'])==len(hp['history'])==13,'derived21/13 and postUndoRedo23/13')
    for t in ('revisions','revision_patches'):
        need(set(hd[t])<=set(hp[t]) and len(set(hp[t])-set(hd[t]))==2,'all copied revision/patch rows plus exactly Undo/Redo')
    for t in ('history','redo','transaction_steps'):need(hd[t]==hp[t],'complete copied history rows retained')
    need(hd['state'][0][2:]==hp['state'][0][2:] and hd['state'][0][1]==new['revision_id'] and hp['state'][0][1]==get(clipped,'portable-redone-dump.stdout')['revision_id'],'stable cursor and distinct final redone head')
    need(hd['history_receipt'][0][2]==21 and hp['history_receipt'][0][2]==23 and hd['history_receipt'][0][1]==hp['history_receipt'][0][1] and hd['history_receipt'][0][5:]==hp['history_receipt'][0][5:],'durable validation receipt advance')
    stored_total=sum(x['bytes'] for x in inv.values())
    result.update(status='no_findings',findings=[],inventory_sha256=INV,primary_files=len(before),payloads=len(inv),payload_bytes=stored_total,primary_bytes_including_inventory=sum(x['bytes'] for x in before.values()),encoding_records=len(rows),encodings=dict(counts),gzip_streams=counts['gzip']+counts['tar+gzip'],encoded_ledger_stored_bytes=sum(r['stored_bytes'] for r in rows.values()),decoded_ledger_bytes=sum(r['decoded_bytes'] for r in rows.values()),raw_regular_bytes=raw_regular,raw_archive_source_bytes=archive_source_bytes,raw_payload_bytes=raw_regular+archive_source_bytes,budget_bytes=LIMIT,archives=summaries,tar_members=sum(len(x) for x in archives.values()),tar_member_bytes=archive_member_bytes,authored_omissions=0,metadata_omissions=sum(len(x) for x in all_omissions.values()),omitted_metadata_bytes=sum(x['bytes'] for m in all_omissions.values() for x in m.values()),final_source_paths=21,embedded_raw_audit_sha256=AUDIT,selected_pngs=pngs,native=natives,raw_tests=raw_tests,native_media=native_media,derived_media=derived_media,native_history_counts={k:len(v) for k,v in hnative.items()},native_portable_history_all_tables_equal=True,derived_history_counts={k:len(v) for k,v in hd.items()},derived_portable_after_undo_redo_counts={k:len(v) for k,v in hp.items()},copied_history_and_all_revision_rows_preserved=True,actual_clipped_wrapper_exit=1,derived_product_commands=181,clipped_frames=clipped_frames,clipped_guard_reconciliation_sha256=sha(scratch('clipped-media-reconciliation.json')),unit_binary_terminal_only=True,collector_sha256=collection['script_sha256'],plan_sha256=collection['plan_sha256'])
except Exception as exc:
    traceback.print_exc();result['status']='findings';result['findings'].append(dict(type=type(exc).__name__,message=str(exc)))
finally:
    after=tree(P);old_after={n:meta(S/n) for n in old_names}
    result['primary_tree_unchanged']=before==after
    result['inventory_unchanged']=before['files.json']==after['files.json']
    result['earlier_audit_files_unchanged']=old_before==old_after
    result['current_scoped_sources_unchanged']=source_before=={p:meta(R/p) for p in source_before}
    if not all(result[k] for k in ('primary_tree_unchanged','inventory_unchanged','earlier_audit_files_unchanged','current_scoped_sources_unchanged')):
        result['status']='findings';result['findings'].append(dict(message='Primary, earlier audit/reconciliation or scoped source changed'))
    result['prior_audit_file_pins']=old_before;result['verifier_sha256']=meta(Path(__file__))['sha256']
    result['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S/'package-verification.json').open('x') as f:json.dump(result,f,indent=2,sort_keys=True);f.write('\n')
    print(json.dumps({k:result.get(k) for k in ('status','findings','primary_files','payloads','payload_bytes','inventory_sha256','raw_payload_bytes','tar_members','metadata_omissions','authored_omissions','primary_tree_unchanged','earlier_audit_files_unchanged')},sort_keys=True))
sys.exit(0 if result['status']=='no_findings' else 1)
