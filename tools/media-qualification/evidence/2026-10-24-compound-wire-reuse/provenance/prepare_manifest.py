"""Generate a proposed explicit package manifest after all producer terminals exist."""
from pathlib import Path
from pathlib import PurePosixPath
import hashlib
import json
import stat
import unicodedata

S = Path('/tmp/deadpan-compound-wire-reuse-production-2026-10-24')
DRAFT = S / 'packaging-draft'
OUT = DRAFT / 'approved-package-manifest.json'
PLAN = DRAFT / 'static-packaging-plan.md'
PACKAGE = DRAFT / 'package.py'
DEST = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-24-compound-wire-reuse')
STAGES = ['rustc','cargo','format','core-unit-build','core-unit-all','core-integration','store-preparation','release-app','strict']
MEDIA_TREE = ('media-one-leaf-fixture.json','media-evidence-inventory.json','media-terminal.json')
TOP_SUFFIXES = {'.json','.py','.md','.patch','.stdout','.stderr'}
TOP_EXCLUDE = {'approved-package-manifest.json','package-manifest.json','inventory.json'}
EXTRA_HISTORY = [
    (S/'analyze-before-root-review.py.txt','history/analyze-before-root-review.py.txt'),
    (S/'analyze-before-schema-fix.py.txt','history/analyze-before-schema-fix.py.txt'),
    (DRAFT/'package-before-review-fix.py.txt','history/package-before-review-fix.py.txt'),
    (DRAFT/'prepare_manifest-before-review-fix.py.txt','history/prepare_manifest-before-review-fix.py.txt'),
    (DRAFT/'package-before-grouped-review-fixes.py.txt','history/package-before-grouped-review-fixes.py.txt'),
    (DRAFT/'prepare_manifest-before-grouped-review-fixes.py.txt','history/prepare_manifest-before-grouped-review-fixes.py.txt'),
    (DRAFT/'packaging-draft-history.json','history/packaging-draft-history.json'),
    (DRAFT/'draft-review.md','history/draft-review.md'),
]
MAX_PAYLOAD_BYTES=25_000_000
MAX_TOTAL_BYTES=150_000_000
EXPECTED_FIXTURES={f'rotation-{i}-{kind}' for i in range(1,6) for kind in ('control','candidate')}
HELPER_MANIFEST=Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json')
REPORTS = {'measurement-report'}


def sha_bytes(data): return hashlib.sha256(data).hexdigest()
def sha(path): return sha_bytes(Path(path).read_bytes())
def read(path): return json.loads(Path(path).read_text())
def tree_map(root):
    root=Path(root); result={}
    assert not root.is_symlink() and root.is_dir()
    for p in sorted(root.rglob('*')):
        assert not p.is_symlink()
        mode=p.lstat().st_mode
        if stat.S_ISDIR(mode): continue
        assert stat.S_ISREG(mode)
        result[p.relative_to(root).as_posix()]={'bytes':p.stat().st_size,'sha256':sha(p)}
    return result
def payload(source, destination, compress=True):
    q=PurePosixPath(destination)
    assert destination and not q.is_absolute() and '..' not in q.parts and '.' not in q.parts and '\\' not in destination
    assert q.as_posix()==destination
    assert unicodedata.normalize('NFC',destination)==destination
    p=Path(source); assert p.is_file() and not p.is_symlink()
    assert p.stat().st_size<=MAX_PAYLOAD_BYTES, (p,p.stat().st_size)
    raw=p.read_bytes()
    return {'source':str(p),'destination':destination,'bytes':len(raw),
            'sha256':sha_bytes(raw),'encoding':'gzip' if compress and len(raw)>100_000 else 'identity'}


def main():
    assert not OUT.exists(), 'refuse overwrite of proposed approved manifest'
    # Fail closed until the build, timing, and media producers have completed.
    build=read(S/'build-terminal.json'); timing=read(S/'measurement-terminal.json'); media=read(S/'media-terminal.json')
    assert build['status']=='complete' and build['steps']==9 and build['stage_names']==STAGES
    assert timing['status']=='complete' and timing['trials']==10 and timing['commands']==20
    assert media['status']=='complete' and media['commands']==4 and media['tests']==1
    analysis_review=S/'analysis-review.json'
    assert (S/'analysis.json').is_file() and analysis_review.is_file()
    assert read(analysis_review)['status']=='no_findings' and read(analysis_review)['findings']==[]
    assert not (S/'memory-terminal.json').exists(), 'this package has no memory lane'
    binaries=read(S/'binaries.json'); baseline=read(S/'baseline.json'); pins=read(S/'media-static-pins.json')
    candidate_app=binaries['deadpan-app']; control=baseline['control']; helper_manifest=read(HELPER_MANIFEST)
    assert candidate_app['sha256']==build['app_sha256'] and sha(candidate_app['executed'])==candidate_app['sha256']
    assert control['sha256']=='970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2'
    assert sha(control['path'])==control['sha256']
    assert sha(HELPER_MANIFEST)==baseline['qualified_helper_manifest_sha256']==pins['qualified_helper_manifest_sha256']
    for name,want in pins['qualified_helper_sha256'].items():
        entry=binaries[name]; qualified=helper_manifest[name]
        assert entry['qualified_helper_reuse'] and not entry['freshly_built']
        assert entry['sha256']==qualified['sha256']==want
        assert sha(entry['executed'])==want and sha(qualified['executed'])==want
    source_map=read(S/'frozen-sources.json'); base_map=read(S/'baseline-sources.json')
    assert len(source_map)==len(base_map)==2053
    assert sha(S/'frozen-sources.json')=='2b0997f1ad22ce7dd869beba699909d22972efba434279cee350478dc63cb728'
    assert sha(S/'baseline-sources.json')=='f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53'
    assert {p for p in source_map if source_map[p]!=base_map[p]} == {
        'crates/deadpan-core/src/compound.rs','crates/deadpan-core/src/one_leaf_replay.rs'}
    assert sha(PLAN) and PLAN.is_file() and PACKAGE.is_file()
    source_review=read(S/'source-review.json')
    assert source_review['status']=='no_findings' and source_review['findings']==[]
    assert source_review['source_map_sha256']==sha(S/'frozen-sources.json')
    assert source_review['prototype_map_sha256']
    assert len(read(S/'baseline-inputs.json'))==6
    rows=[]
    # Preserve bounded top-level scripts, receipts, maps, logs, notes, and analysis records.
    for p in sorted(S.iterdir()):
        if p.is_file() and not p.is_symlink() and p.suffix in TOP_SUFFIXES and p.name not in TOP_EXCLUDE:
            rows.append(payload(p,'production/'+p.name))
    # Raw JSON/HTML reports are separate directories, not top-level files.
    report_dirs=sorted(p for p in S.iterdir() if p.is_dir() and p.name.endswith('-report'))
    assert len(report_dirs)==10, len(report_dirs)
    for report_dir in report_dirs:
        for report_file in sorted(report_dir.iterdir()):
            if report_file.is_file() and not report_file.is_symlink() and report_file.suffix in {'.json','.html','.stdout','.stderr'}:
                rows.append(payload(report_file,'reports/'+report_dir.name+'/'+report_file.name))
    # The static plan is carried once in the approval envelope, bound by its digest.
    rows.extend([payload(Path(__file__),'provenance/prepare_manifest.py',False),
                 payload(PACKAGE,'provenance/package.py',False)])
    for src,dest in EXTRA_HISTORY:
        rows.append(payload(src,dest,False))
    # Six exact source/build input files are separately retained as bytes.
    inputs=read(S/'baseline-inputs.json')
    assert inputs==read(Path('/tmp/deadpan-single-leaf-net-production-2026-10-24/baseline-inputs.json'))
    for rel,digest in sorted(inputs.items()):
        p=S/'source'/rel
        assert sha(p)==digest
        rows.append(payload(p,'inputs/'+rel))
    qualified_root=Path('/tmp/deadpan-single-leaf-net-production-2026-10-24/source')
    for rel,digest in inputs.items(): assert sha(qualified_root/rel)==digest
    # Keep the exact before/after Rust for the two changed paths as plain text snapshots.
    changes=[]
    for rel in sorted(p for p in source_map if source_map[p]!=base_map[p]):
        candidate=S/'source'/rel; qualified=Path('/tmp/deadpan-single-leaf-net-production-2026-10-24/source')/rel
        assert sha(candidate)==source_map[rel] and sha(qualified)==base_map[rel]
        before='source/qualified-main/'+rel+'.txt'; after='source/candidate/'+rel+'.txt'
        rows.extend([payload(qualified,before,False),payload(candidate,after,False)])
        changes.append({'path':rel,'before_sha256':base_map[rel],'before_destination':before,
                        'after_sha256':source_map[rel],'after_destination':after})
    # Preserve the complete actual media evidence tree listed by its verified inventory.
    inv=read(S/'media-evidence-inventory.json'); root=Path(inv['root']).resolve(strict=True)
    assert tree_map(root)==inv['files'], 'media inventory must name the complete regular-file tree'
    media_files=[]
    for rel,meta in sorted(inv['files'].items()):
        p=root/rel
        assert p.is_file() and not p.is_symlink() and p.stat().st_size==meta['bytes'] and sha(p)==meta['sha256']
        dest='media-evidence/'+rel
        rows.append(payload(p,dest,False))
        media_files.append({'relative_path':rel,'destination':dest,'bytes':meta['bytes'],'sha256':meta['sha256']})
    assert len(media_files)==35
    assert {x['relative_path'] for x in media_files}==set(inv['files'])
    media_terminal=read(S/'media-terminal.json')
    exports=media_terminal['evidence']['exports']
    assert {x['leg'] for x in exports}=={'cached','uncached'}
    for export in exports:
        movie=Path(export['movie']).resolve(strict=True)
        rel=movie.relative_to(root).as_posix()
        meta=inv['files'][rel]
        assert sha(movie)==export['movie_sha256']==meta['sha256']
        assert meta['bytes']==movie.stat().st_size
    # Fixture maps are payloads; archives exclude exactly the two verified SQLite runtime sidecars.
    fixture_rows=[]
    roots=sorted(S.glob('rotation-*.deadpan'))
    assert len(roots)==10, len(roots)
    destinations={r['destination']:r for r in rows}
    for root in roots:
        name=root.name[:-len('.deadpan')]
        assert name in EXPECTED_FIXTURES and root == S/(name+'.deadpan') and not root.is_symlink()
        before=S/(name+'-input-before.json'); after=S/(name+'-input-after.json')
        assert before.is_file() and after.is_file()
        b,a=read(before),read(after)
        assert tree_map(root)==a
        before_auth={k:v for k,v in b.items() if k not in {'project.sqlite-shm','project.sqlite-wal'}}
        after_auth={k:v for k,v in a.items() if k not in {'project.sqlite-shm','project.sqlite-wal'}}
        assert before_auth==after_auth
        sidecars={}
        for side in ('project.sqlite-shm','project.sqlite-wal'):
            if side in a:
                sidecars[side]={'after':a[side]}
                if side in b: sidecars[side]['before']=b[side]
        assert sidecars.get('project.sqlite-shm',{}).get('after',{}).get('bytes')==32768
        assert sidecars.get('project.sqlite-wal',{}).get('after',{}).get('bytes')==0
        assert set(sidecars)=={'project.sqlite-shm','project.sqlite-wal'}
        bd='production/'+before.name; ad='production/'+after.name
        assert bd in destinations and ad in destinations
        fixture_rows.append({'name':name,'root':str(root),
          'before_map':{'source':str(before),'destination':bd,'bytes':before.stat().st_size,'sha256':sha(before)},
          'after_map':{'source':str(after),'destination':ad,'bytes':after.stat().st_size,'sha256':sha(after)},
          'allowed_sidecars':sidecars,'archive_destination':'fixtures/'+name+'.tar.gz'})
    dests=[r['destination'] for r in rows]
    assert len(dests)==len(set(dests)), 'duplicate payload destination'
    assert len({x['name'] for x in fixture_rows})==len(fixture_rows)==10
    assert {x['name'] for x in fixture_rows}==EXPECTED_FIXTURES
    assert len({x['archive_destination'] for x in fixture_rows})==10
    assert len({x['root'] for x in fixture_rows})==10
    reserved={'approval/approved-package-manifest.json','approval/packager-final-review.json',
      'approval/PACKAGING-GO.json','approval/static-packaging-plan.md','approval/package.py',
      'package-manifest.json','inventory.json'}
    assert not ({r['destination'] for r in rows}|{x['archive_destination'] for x in fixture_rows}) & reserved
    all_destinations={r['destination'] for r in rows}|{x['archive_destination'] for x in fixture_rows}|reserved
    components=[PurePosixPath(x).parts for x in all_destinations]
    assert len(all_destinations)==len(rows)+len(fixture_rows)+len(reserved)
    folded=[tuple(unicodedata.normalize('NFC',part).casefold() for part in parts) for parts in components]
    assert len(set(folded))==len(folded), 'normalized or case-folded destination alias'
    assert all(not (a!=b and len(a)<len(b) and b[:len(a)]==a) for a in folded for b in folded), 'file/directory path collision'
    for row in rows:
        p=Path(row['source']); assert not p.is_symlink() and stat.S_ISREG(p.lstat().st_mode)
        assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256']
    fixture_bytes=sum(meta['bytes'] for fixture in fixture_rows
                      for rel,meta in read(fixture['after_map']['source']).items()
                      if rel not in {'project.sqlite-shm','project.sqlite-wal'})
    assert fixture_bytes<=10*MAX_PAYLOAD_BYTES
    assert all(sum(meta['bytes'] for rel,meta in read(f['after_map']['source']).items()
                   if rel not in {'project.sqlite-shm','project.sqlite-wal'})<=MAX_PAYLOAD_BYTES
               for f in fixture_rows)
    assert sum(row['bytes'] for row in rows)+fixture_bytes<=MAX_TOTAL_BYTES
    manifest={'schema':'deadpan-compound-wire-reuse-production-evidence-1',
      'status':'proposed_for_root_review','candidate_source_map_sha256':sha(S/'frozen-sources.json'),
      'qualified_base_source_map_sha256':sha(S/'baseline-sources.json'),
      'input_map_sha256':sha(S/'baseline-inputs.json'),'candidate_app_sha256':build['app_sha256'],
      'control_app_sha256':read(S/'baseline.json')['control']['sha256'],
      'source_review_sha256':sha(S/'source-review.json'),'analysis_review_sha256':sha(analysis_review),
      'binary_manifest_sha256':sha(S/'binaries.json'),
      'qualified_helper_manifest_sha256':sha(Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json')),
      'analysis_sha256':sha(S/'analysis.json'),'media_inventory_sha256':sha(S/'media-evidence-inventory.json'),
      'static_packaging_plan_sha256':sha(PLAN),'terminal_sha256':{
        'build':sha(S/'build-terminal.json'),'measurement':sha(S/'measurement-terminal.json'),
        'media':sha(S/'media-terminal.json')},'source_changes':changes,'payloads':rows,
      'fixtures':fixture_rows,'media_files':media_files}
    OUT.write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'manifest':str(OUT),'payloads':len(rows),'fixtures':len(fixture_rows),
      'media_files':len(media_files),'source_changes':len(changes),'manifest_sha256':sha(OUT)},sort_keys=True))

if __name__=='__main__': main()
