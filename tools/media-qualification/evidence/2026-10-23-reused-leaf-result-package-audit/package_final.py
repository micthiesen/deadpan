#!/usr/bin/env python3
"""Final package builder draft. Run only after root authorizes finalization.

Reads actual producer receipts and frozen overlay trees from the qualification
scratch. Requires all eight qualification stages and every post stage to finish
successfully, plus a root-written GO marker. It never copies binaries, nextest
archives, targets, generated project packages, SQLite databases, or media bytes.
"""
from __future__ import annotations
import argparse, gzip, hashlib, json, shutil
from pathlib import Path

S = Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
CHECKPOINT = REPO / 'tools/media-qualification/evidence/2026-10-23-reused-leaf-result-core-checkpoint'
GO = S / 'FINAL-PACKAGING-GO.json'
BASE = '1e67fdb27bbb3a59d282e5240c65b860b4090b76'
EXPECTED = {'candidate-all-core': (1514, 69), 'candidate-broad': (3505, 110), 'candidate-ui': (1296, 2)}
PROD = 'crates/deadpan-core/src/semantic/planner/pause.rs'
TEST = 'crates/deadpan-core/src/semantic/planner/tests/pause.rs'
TEST_NAME = 'interior_pause_and_audio_setter_match_legacy_leaf_patches_with_bound_audio_and_marks'

def sha(b: bytes) -> str: return hashlib.sha256(b).hexdigest()
def load(path: Path): return json.loads(path.read_text())
def need(path: Path):
    if not path.exists(): raise SystemExit(f'missing required terminal evidence: {path}')
    return path

def test_counts(phase: str):
    result=load(need(S/f'{phase}-result.json'))
    tests,bins=EXPECTED[phase]
    assert result == {'source_base': BASE, 'source_location': 'candidate', 'exit': 0, 'tests': tests, 'binaries': bins, 'sources_inputs_executables_unchanged': True}, (phase,result)
    log=(S/f'{phase}-run.log').read_text()
    expected_summary={
      'candidate-all-core': '1514 tests run: 1514 passed, 0 skipped',
      'candidate-broad': '3505 tests run: 3505 passed (1 leaky), 740 skipped',
      'candidate-ui': '1296 tests run: 1296 passed (1 leaky), 11 skipped',
    }[phase]
    assert expected_summary in log, (phase,expected_summary)
    listing=load(need(S/f'{phase}-list.json'))['rust-suites']
    matched=sum(1 for suite in listing.values() if any(case.get('filter-match',{}).get('status')=='matches' for case in suite.get('testcases',{}).values()))
    assert matched==bins, (phase,len(listing),matched,bins)
    binaries=load(need(S/f'{phase}-binaries.json'))
    assert len(binaries)==bins
    for row in binaries.values():
        path=Path(row['path']); assert path.is_file() and sha(path.read_bytes())==row['sha256']
    return result

def verify_gates():
    if not GO.is_file(): raise SystemExit(f'no finalization authorization marker: {GO}')
    go=load(GO)
    assert go.get('authorized') is True and isinstance(go.get('by'),str) and go['by'].strip()
    q=load(need(S/'qualification-receipt.json'))
    assert len(q)==8 and [r['step'] for r in q]==list(range(1,9)) and all(r['exit']==0 for r in q),q
    for phase in EXPECTED: test_counts(phase)
    assert TEST_NAME in (S/'candidate-all-core-run.log').read_text()
    # Strict check, formatting, and strict-UI Clippy/format have separate receipts.
    for name in ['candidate-strict-check-receipt.json','candidate-strict-format-receipt.json']:
        assert load(need(S/name))['exit']==0
    ui_lint=load(need(S/'ui-lint-receipt.json'))
    assert len(ui_lint)==2 and {r['name'] for r in ui_lint}=={'clippy','format'} and all(r['exit']==0 for r in ui_lint)
    # Exact phase outputs are produced by qualify_native_media.py.
    for phase,names in {
      'debug-media':['build','neutral'],
      'release-media':['build','neutral','aligned'],
      'release-native':['build','performance'],
    }.items():
      for stage in names: assert load(need(S/f'{phase}-{stage}-receipt.json'))['exit']==0
    media=load(need(S/'release-media-binaries.json'))
    assert {'deadpan-cli','deadpan-media-worker','deadpan-transcribe','deadpan-track'} <= set(media['helpers'])
    for item in media['helpers'].values():
      assert Path(item['path']).is_file() and sha(Path(item['path']).read_bytes())==item['sha256']
    native=load(need(S/'release-native-binaries.json'))
    assert set(native)=={'deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track'}
    for item in native.values():
      p=Path(item['executed']);assert p.is_file() and sha(p.read_bytes())==item['sha256']
    report=load(need(S/'release-native-performance/report.json'));assert report.get('failed') is False
    # Post driver emits one receipt per operation, one summary for five native scenarios,
    # and exactly five alternating pairs with a candidate/control row each.
    post=load(need(S/'post-receipt.json'))
    assert post and all(x.get('exit')==0 for x in post),post
    portable=load(need(S/'owner-portable-check/receipt.json'))
    assert portable['safe_relative_regular_files_only'] is True
    assert len(portable['cases'])==2
    for case in portable['cases']:
      assert len(case['commands'])==2 and all(c['exit']==0 and c['package_paths_and_bytes_unchanged'] for c in case['commands'])
    extra=load(need(S/'extra-native-summary.json'))
    assert len(extra)==5 and all(not x['failed'] and x['checks']>0 for x in extra)
    for i in range(1,6):
      for kind in ('control','candidate'):
        report=load(need(S/f'pair-{i}-{kind}-report/report.json'))
        assert report.get('failed') is False
        case=next(c for c in report['scenarios'] if c['name']=='large-first-pause')
        assert len(case['checks'])==7 and all(c['passed'] for c in case['checks'])
        before=load(need(S/f'pair-{i}-{kind}-input-before.json'))
        after=load(need(S/f'pair-{i}-{kind}-input-after.json'))
        excluded={'project.sqlite-shm','project.sqlite-wal'}
        assert {k:v for k,v in before.items() if k not in excluded}=={k:v for k,v in after.items() if k not in excluded}
        assert after.get('project.sqlite-wal',{}).get('bytes',0)==0
        assert report['metadata']['fixture_sha256']==before['project.sqlite']['sha256']==after['project.sqlite']['sha256']
        row=next(r for r in load(need(S/'pause-trials.json')) if r['pair']==i and r['kind']==kind)
        assert row['checks']==7 and row['source_binary_guards'] and row['authored_input_unchanged'] and row['wal_empty']
        assert row['load'][0] <= 3
    trials=load(need(S/'pause-trials.json'));summary=load(need(S/'pause-summary.json'))
    assert len(trials)==10 and summary['n_per_kind']==5 and summary['diagnostic_only'] and summary['no_percentile_or_target_pass']
    assert {x['kind'] for x in trials}=={'control','candidate'}
    isolated=load(need(S/'isolated-leak-reruns.json'))
    expected_isolated={('candidate-broad','serialized_record_does_not_grant_authority_over_current_inventory'),('candidate-ui','ntsc_picture_changes_at_allocated_sample_boundary_once')}
    assert {(r['phase'],r['test']) for r in isolated}==expected_isolated
    assert all(r['exit']==0 and r['tests']==r['passed']==1 and r['source_binary_guards'] for r in isolated)
    for r in isolated:
      log=(S/f"{r['phase']}-isolated-leak-rerun.log").read_text()
      assert '1 test run: 1 passed' in log
    initial=need(S/'isolated_leak_reruns_initial.py.txt').read_text()
    initial_err=need(S/'isolated-leak-reruns-initial-driver.stderr').read_text()
    assert 'guard.py' in initial and 'candidate-broad-lock.json' in initial_err
    return {'qualification_steps':len(q),'post_steps':len(post),'test_phases':{k:v[0] for k,v in EXPECTED.items()},'timing_trials':len(trials),'isolated_leak_reruns':len(isolated),'authorization':go}

def copy(src: Path, dst: Path, compress=False):
    data=src.read_bytes();dst.parent.mkdir(parents=True,exist_ok=True)
    if compress:
      dst=dst.with_name(dst.name+'.gz');dst.write_bytes(gzip.compress(data,compresslevel=9,mtime=0))
    else: dst.write_bytes(data)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--destination',type=Path,required=True)
    ap.add_argument('--finalize',action='store_true')
    args=ap.parse_args()
    if not args.finalize: raise SystemExit('draft mode; use --finalize only after root authorizes and gates are terminal')
    gates=verify_gates()
    d=args.destination.resolve()
    if d.exists(): raise SystemExit(f'destination already exists: {d}')
    d.mkdir(parents=True)
    # Keep exact receipts, manifests, run lists, fixture maps, and outputs. Compress all
    # logs/stdout/stderr and large JSON reports; exclude executables and generated payloads.
    for p in sorted(S.rglob('*')):
      if not p.is_file(): continue
      rel=p.relative_to(S); n=p.name
      if p==GO: continue
      if rel.parts[0] in {'next-size-study','final-evidence-draft'}: continue
      if any(part.endswith(('-extracted','-overlays','-source')) for part in rel.parts): continue
      if any(part in {'target'} for part in rel.parts): continue
      if any(part.endswith(('.deadpan','.tar','.tar.zst','.tar.gz','.mp4')) for part in rel.parts): continue
      if any(part in {'first-focused-failure'} for part in rel.parts):
        if not (rel.parts[0]=='first-focused-failure' and p.suffix in {'.json','.log','.stdout','.stderr'}): continue
      is_source_map=n.endswith('-source-identities.json')
      is_gate_file=(p.suffix=='.json' or p.suffix in {'.py','.txt'} or p.suffix in {'.stdout','.stderr','.log'})
      if not is_gate_file: continue
      if p.suffix in {'.stdout','.stderr','.log'} or (p.suffix in {'.json','.txt'} and p.stat().st_size>100_000):
        copy(p,d/'raw'/rel,True)
      else: copy(p,d/'raw'/rel)
    # Copy only the two candidate Core Rust files that changed, from their frozen
    # overlay snapshots, and verify each against the actual all-Core source identity map.
    manifest=load(need(S/'candidate-all-core-source-identities.json'))
    overlay=S/'candidate-all-core-overlays'
    audited={}
    for rel in [PROD,TEST]:
      frozen=overlay/(rel+'.txt');need(frozen)
      raw=frozen.read_bytes();assert sha(raw)==manifest[rel],(rel,sha(raw),manifest[rel])
      target=d/'sources'/f'{rel.replace("/","__")}.rs.txt';copy(frozen,target)
      audited[rel]={'manifest_sha256':manifest[rel],'retained_sha256':sha(target.read_bytes()),'bytes':len(raw)}
    (d/'source-copy-audit.json').write_text(json.dumps({'base':BASE,'all_core_source_manifest':manifest and sha((S/'candidate-all-core-source-identities.json').read_bytes()),'sources':audited},indent=2)+'\n')
    # Preserve the completed checkpoint verbatim, including the initial failure,
    # corrected focused fixture, exact sources, reviews, receipts, and their inventory.
    if not (CHECKPOINT/'inventory.json').is_file(): raise SystemExit(f'missing prior checkpoint: {CHECKPOINT}')
    inv=load(CHECKPOINT/'inventory.json')
    for row in inv['files']:
      src=CHECKPOINT/row['path']; data=src.read_bytes()
      if len(data)!=row['bytes'] or sha(data)!=row['sha256']: raise SystemExit(f'checkpoint changed: {row["path"]}')
      copy(src,d/'historical-core-checkpoint'/row['path'])
    copy(CHECKPOINT/'inventory.json',d/'historical-core-checkpoint/inventory.json')
    # Independent output inventory and read-after-write verification.
    rows=[]
    for p in sorted(d.rglob('*')):
      if p.is_file() and p != d/'inventory.json':
        b=p.read_bytes();rows.append({'path':p.relative_to(d).as_posix(),'bytes':len(b),'sha256':sha(b)})
    inventory={'files':rows,'total_files':len(rows),'total_bytes':sum(x['bytes'] for x in rows),'verified_gates':gates}
    (d/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
    for row in rows:
      b=(d/row['path']).read_bytes()
      assert len(b)==row['bytes'] and sha(b)==row['sha256'],row['path']
    print(json.dumps({'files':len(rows),'bytes':sum(x['bytes'] for x in rows),'inventory_sha256':sha((d/'inventory.json').read_bytes()),'all_payloads_verified':True},indent=2))
if __name__=='__main__': main()
