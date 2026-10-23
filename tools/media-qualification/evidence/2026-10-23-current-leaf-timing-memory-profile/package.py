#!/usr/bin/env python3
"""Draft bounded evidence packager. Root runs only after terminal receipts and GO."""
from __future__ import annotations
import gzip, hashlib, importlib.util, json, shutil, statistics, difflib, re
from pathlib import Path

ROOT = Path('/tmp/deadpan-current-leaf-timing-memory-profile-2026-10-23')
REPO = Path('/Users/michael/Code/deadpan')
DEST = REPO / 'tools/media-qualification/evidence/2026-10-23-current-leaf-timing-memory-profile'
STAGE = DEST.with_name(DEST.name + '.staging')
Q = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
PERF = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/candidate-perf')
CONTROL_SHA = 'a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f'
PRIVATE_APP_SHA = '14ef183594c33011c7557c8affd0bf516d24a55a9994d42a8d0648deb01e6ede'
OBSERVER_SHA = 'd1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2'
PERF_SHA = 'fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166'
BASE_COMMIT = '3a32c923358c0cb7ca53f821bfb6db3e01ebb466'
CHANGED = [
 'crates/deadpan-app/src/project/service.rs',
 'crates/deadpan-app/src/project/service/macros.rs',
 'crates/deadpan-core/src/compound.rs',
 'crates/deadpan-core/src/leaf_profile.rs',
 'crates/deadpan-core/src/lib.rs',
 'crates/deadpan-core/src/semantic/planner/pause.rs',
]
INPUTS = ['Cargo.lock','Cargo.toml','rust-toolchain.toml','.cargo/config.toml',
          'crates/deadpan-app/src/navigation/registry/baseline-routing.txt','docs/COMMANDS.md']
KINDS = ('control','off','on')
SIDECARS = {'project.sqlite-shm','project.sqlite-wal'}
LIMIT = 100_000
SAFE_SUFFIXES = {'.json','.py','.md','.txt','.log','.stdout','.stderr','.html','.patch'}


def require(ok, why):
    if not ok: raise RuntimeError(why)

def read(path): return json.loads(Path(path).read_text())

def sha(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''): h.update(block)
    return h.hexdigest()

def copy_payload(src, rel):
    src=Path(src); require(src.is_file(),f'missing required payload: {src}')
    dst=STAGE/rel; dst.parent.mkdir(parents=True,exist_ok=True)
    data=src.read_bytes()
    if len(data)>LIMIT and src.suffix.lower() in {'.json','.txt','.md','.log','.stdout','.stderr','.html'}:
        dst=Path(str(dst)+'.gz')
        with dst.open('wb') as raw:
            with gzip.GzipFile(filename='',mode='wb',compresslevel=9,fileobj=raw,mtime=0) as stream:
                stream.write(data)
        with gzip.open(dst,'rb') as stream: require(stream.read()==data,f'gzip verify failed: {src}')
    else: dst.write_bytes(data)
    return dst

def copy_tree(src, rel, allowed=SAFE_SUFFIXES):
    src=Path(src); require(src.is_dir(),f'missing evidence directory: {src}')
    copied=0
    for p in sorted(src.rglob('*')):
        if not p.is_file() or p.suffix.lower() not in allowed: continue
        copy_payload(p,Path(rel)/p.relative_to(src));copied+=1
    require(copied>0,f'empty bounded evidence tree: {src}')
    return copied

def canonical(value,path):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')

def verify_binaries():
    bins=read(ROOT/'binaries.json'); observer=read(ROOT/'observer-binary.json')
    control=read(Q/'release-native-binaries.json')['deadpan-app']
    require(bins['deadpan-app']['sha256']==PRIVATE_APP_SHA,'private app hash changed')
    require(observer['sha256']==OBSERVER_SHA,'observer hash changed')
    require(control['sha256']==CONTROL_SHA,'qualified control app hash changed')
    for name,row in bins.items():
        p=Path(row['executed']);require(p.is_file() and sha(p)==row['sha256'],f'executed binary mismatch: {name}')
    for name,row in read(Q/'release-native-binaries.json').items():
        p=Path(row['executed']);require(p.is_file() and sha(p)==row['sha256'],f'qualified helper mismatch: {name}')
    observer_path=Path(observer['path']);require(observer_path.is_file() and sha(observer_path)==OBSERVER_SHA,'observer executable mismatch')
    observer_source=Path(observer['source'])
    require(observer_source.is_file() and sha(observer_source)==observer['source_sha256'],
            'observer source identity mismatch')
    reviewed=ROOT/'memory_observer.reviewed.rs.txt'
    require(reviewed.is_file() and sha(reviewed)==observer['reviewed_source_sha256'],
            'reviewed observer source identity mismatch')
    adapter=ROOT/'source/native/deadpan-process/src/lib.rs'
    rlib=Path(observer['adapter_rlib'])
    require(adapter.is_file() and sha(adapter)==observer['private_native_process_source_sha256'],
            'private process adapter source identity mismatch')
    require(rlib.is_file() and sha(rlib)==observer['adapter_rlib_sha256'],
            'observer linked adapter rlib identity mismatch')
    require(sha(PERF)==PERF_SHA,'candidate-perf helper hash mismatch')
    return bins,observer,control

def verify_sources():
    before=read(ROOT/'original-source-identities.json')
    after=read(ROOT/'instrumented-source-identities.json')
    audit=read(ROOT/'formatted-overlay-audit.json')
    basis=read(ROOT/'source-basis.json')
    require(basis.get('source_commit')==BASE_COMMIT and basis.get('rust_files')==2051,
            'source baseline identity mismatch')
    require(len(before)==2051 and before==read(Q/'candidate-all-core-source-identities.json'),
            'baseline map differs from qualified 2,051-file production map')
    require(len(after)==2052,'instrumented candidate must contain 2,052 Rust paths')
    require(set(audit.get('changed',[]))==set(CHANGED),'overlay changed-file set mismatch')
    require(audit.get('inputs_unchanged') is True and audit.get('new_rust_count')==2052,
            'overlay source/input guard changed')
    require({p for p,v in after.items() if before.get(p)!=v}==set(CHANGED),
            'source map differs outside the six-file overlay')
    current={str(p.relative_to(ROOT/'source')):sha(p) for p in (ROOT/'source').rglob('*.rs')}
    require(current==after,'live frozen source tree differs from final source map')
    inputs=read(ROOT/'input-identities.json')
    require(set(inputs)==set(INPUTS) and inputs==read(Q/'candidate-all-core-inputs.json'),
            'six input map differs from qualified production baseline')
    require(all(sha(ROOT/'source'/p)==v for p,v in inputs.items()),'input changed in private source tree')
    return before,after,inputs,basis

def phase_profile(stderr_path, expect_off):
    spec=importlib.util.spec_from_file_location('current_leaf_profile_parser',ROOT/'check_profile.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    with Path(stderr_path).open('rb') as f:
        result=module.parse(module.bounded_file_lines(f),expect_off=expect_off)
    if expect_off:
        require(result=={'profile_absent':True},f'gate-off profile appeared: {stderr_path}')
    else:
        expected={'app_service_total':1,'semantic_leaf_apply_call':1,
                  'replay_leaf_apply_call':1,'app_refresh_succeeded':1}
        require(result.get('counts')==expected and result.get('nanoseconds',{}).get('app_refresh_succeeded')==0,
                f'profile row counts/zero marker mismatch: {stderr_path}')
    return result

def report_and_inputs(key, row, app_hash):
    before=read(ROOT/f'{key}-input-before.json');after=read(ROOT/f'{key}-input-after.json')
    filtered=lambda d:{k:v for k,v in d.items() if k not in SIDECARS}
    require(filtered(before)==filtered(after),f'{key}: authored paths or bytes changed')
    require(before.get('project.sqlite',{}).get('sha256')==after.get('project.sqlite',{}).get('sha256'),
            f'{key}: authored database changed')
    require(after.get('project.sqlite-wal',{}).get('bytes',0)==0,f'{key}: WAL is not empty')
    report=read(ROOT/f'{key}-report/report.json')
    require(report.get('failed') is False,f'{key}: report failed')
    meta=report.get('metadata',{})
    require(meta.get('build_profile')=='release' and meta.get('binary_sha256')==app_hash,
            f'{key}: report app identity/profile mismatch')
    require(meta.get('fixture_sha256')==before['project.sqlite']['sha256'],f'{key}: report fixture mismatch')
    require(meta.get('cargo_lock_sha256_at_run')==read(ROOT/'input-identities.json')['Cargo.lock'],
            f'{key}: report Cargo.lock identity mismatch')
    require(meta.get('physical_display_measured') is False,
            f'{key}: unexpected physical-display measurement claim')
    pause=[s for s in report.get('scenarios',[]) if s.get('name')=='large-first-pause']
    kestrel=[s for s in report.get('scenarios',[]) if s.get('name')=='kestrel-shortcuts']
    require(len(pause)==len(kestrel)==1,'pause or Kestrel scenario missing/repeated')
    require(len(pause[0].get('checks',[]))==7 and all(c.get('passed') is True for c in pause[0]['checks']),
            f'{key}: seven pause checks failed')
    require(len(kestrel[0].get('checks',[]))==1 and kestrel[0]['checks'][0].get('passed') is True,
            f'{key}: Kestrel check failed')
    metrics={t['name']:t['summary']['completed']['p50_ms'] for t in pause[0].get('timings',[])
             if t.get('name','').startswith('large_first_pause')}
    require(set(metrics)=={'large_first_pause_input_to_commit_ms',
                           'large_first_pause_input_to_offscreen_background_composition_ms'},
            f'{key}: expected timing metric pair missing')
    return before,after,report,metrics

def recompute_sample_summary(receipt,samples_path):
    rows=read(samples_path)
    require(isinstance(rows,list) and rows and len(rows)<=60000,'sample array absent or exceeds bound')
    require(receipt.get('terminal') is True and receipt.get('valid_memory_trial') is True and
            receipt.get('reason')=='exited','observer receipt is not a valid terminal memory trial')
    require(receipt.get('requested_interval_ms')==5 and receipt.get('maximum_runtime_seconds')==180 and
            receipt.get('maximum_samples')==60000,'observer bound fields differ')
    require(receipt.get('group_teardown_confirmed') is True and receipt.get('reap_attempted') is True and
            receipt.get('reaped') is True and receipt.get('app_exit_code')==0 and receipt.get('errors')==[],
            'owned group/reap/app-exit receipt invalid')
    require(receipt.get('samples_written') is True and receipt.get('sampled_lower_bound_only') is True and
            receipt.get('whole_app_lifetime_only') is True and receipt.get('per_leaf_memory') is False and
            receipt.get('memory_ceiling_qualified') is False,'memory scope flags mismatch')
    names={'elapsed_ns','query_ns','phys_footprint_bytes','resident_size_bytes'}
    last=-1;gaps=[]
    for r in rows:
        require(set(r)==names and all(type(r[k]) is int and r[k]>=0 for k in names),'invalid sample shape/value')
        require(r['elapsed_ns']>last,'sample elapsed clock is not strictly increasing')
        if last>=0:gaps.append(r['elapsed_ns']-last)
        last=r['elapsed_ns']
    require(receipt.get('sample_count')==len(rows),'sample count does not match receipt')
    footprint=max(r['phys_footprint_bytes'] for r in rows)
    resident=max(r['resident_size_bytes'] for r in rows)
    first,last=rows[0]['elapsed_ns'],rows[-1]['elapsed_ns']
    max_gap=max(gaps,default=0);query_sum=sum(r['query_ns'] for r in rows)
    require(receipt.get('observed_max_phys_footprint_bytes')==footprint and
            receipt.get('observed_max_resident_size_bytes')==resident,'receipt maxima do not recompute')
    require(receipt.get('first_sample_start_elapsed_ns')==first and
            receipt.get('last_sample_start_elapsed_ns')==last and
            receipt.get('maximum_sample_start_gap_ns')==max_gap and
            receipt.get('sample_query_sum_ns')==query_sum,'receipt clock/gap/query aggregate mismatch')
    require(type(receipt.get('sampling_window_elapsed_ns')) is int and
            receipt['sampling_window_elapsed_ns']>=last+rows[-1]['query_ns'],
            'receipt sampling window does not contain the final query')
    return {'sample_count':len(rows),'observed_max_phys_footprint_bytes':footprint,
            'observed_max_resident_size_bytes':resident,'first_sample_start_elapsed_ns':first,
            'last_sample_start_elapsed_ns':last,'maximum_sample_start_gap_ns':max_gap,
            'sample_query_sum_ns':query_sum,'sampling_window_elapsed_ns':receipt['sampling_window_elapsed_ns']}

def verify_trials():
    timing=read(ROOT/'timing-trials.json');memory=read(ROOT/'memory-trials.json')
    require(len(timing)==len(memory)==9,'need nine timing and nine separate memory trials')
    rotation=[['control','off','on'],['off','on','control'],['on','control','off']]
    expected=[(r,k) for r,order in enumerate(rotation,1) for k in order]
    require([(x['round'],x['kind']) for x in timing]==expected,'timing triplet order mismatch')
    require([(x['round'],x['kind']) for x in memory]==expected,'memory triplet order mismatch')
    app_hash={'control':CONTROL_SHA,'off':PRIVATE_APP_SHA,'on':PRIVATE_APP_SHA}
    timing_profiles=[];memory_profiles=[];timing_metrics={};memory_memory={}
    all_trials=[]
    for lane,rows,profiles in [('timing',timing,timing_profiles),('memory',memory,memory_profiles)]:
        for row in rows:
            r,k=row['round'],row['kind'];key=f'{lane}-{r}-{k}'
            expected_lane='timing' if lane=='timing' else 'owned-observer-memory'
            require(row.get('lane')==expected_lane and row.get('checks')==7 and row.get('kestrel_checks')==1,
                    f'{key}: lane/check counters mismatch')
            require(row.get('load_average',[999])[0]<=3.0,f'{key}: load threshold exceeded')
            if lane=='timing':
                require(row.get('source_binary_input_guards') is True and
                        row.get('authored_paths_bytes_unchanged') is True and row.get('wal_empty') is True,
                        f'{key}: source/binary/input guard missing')
                stderr=ROOT/f'{key}.stderr'
                parsed=phase_profile(stderr,expect_off=(k!='on'))
                parser_output=read(ROOT/f'{key}-parse.stdout')
                require(parser_output==parsed and row.get('profile')==parsed,f'{key}: profile parse mismatch')
                if k=='on':profiles.append(parsed)
                _,_,_,metrics=report_and_inputs(key,row,app_hash[k])
                require(row.get('binary_sha256')==app_hash[k] and row.get('metrics')==metrics,
                        f'{key}: trial/binary/report values disagree')
                timing_metrics[(r,k)]=metrics
            else:
                require(row.get('source_binary_input_observer_guards') is True and
                        row.get('authored_paths_bytes_unchanged') is True and row.get('wal_empty') is True,
                        f'{key}: memory lane source/binary/input guard missing')
                receipt=read(ROOT/f'{key}-observer-receipt.json')
                samples=recompute_sample_summary(receipt,ROOT/f'{key}-memory-samples.json')
                _,_,_,metrics=report_and_inputs(key,row,app_hash[k])
                require(row.get('binary_sha256')==app_hash[k] and row.get('observer_sha256')==OBSERVER_SHA,
                        f'{key}: memory-lane binary identity mismatch')
                row_memory_fields={'sample_count','observed_max_phys_footprint_bytes','observed_max_resident_size_bytes',
                                   'first_sample_start_elapsed_ns','last_sample_start_elapsed_ns',
                                   'maximum_sample_start_gap_ns','sampling_window_elapsed_ns'}
                require(set(row.get('memory',{}))==row_memory_fields and
                        all(row['memory'][field]==samples[field] for field in row_memory_fields) and
                        row.get('timing_metrics_observer_contaminated_do_not_pool')==metrics,
                        f'{key}: observer receipt/sample/report disagreement')
                parsed=phase_profile(ROOT/f'{key}-app.stderr',expect_off=(k!='on'))
                parser_output=read(ROOT/f'{key}-parse.stdout')
                require(parsed==parser_output and row.get('profile')==parsed,f'{key}: memory app profile mismatch')
                if k=='on':profiles.append(parsed)
                memory_memory[(r,k)]=samples
            pre=read(ROOT/f'{key}-preflight.json')
            require(pre.get('ready') is True and pre.get('samples') and
                    pre['samples'][-1].get('load_average',[999])[0]<=3.0 and
                    pre['samples'][-1].get('conflicting_processes')==[],f'{key}: preflight was not quiet')
            all_trials.append(key)
    require(len(timing_profiles)==len(memory_profiles)==3,'expected six enabled profile blocks')
    for rows_,lane,label in [(timing,'timing','metrics'),(memory,'memory','memory')]:
        summary=read(ROOT/f'{lane}-summary.json')
        require(summary.get('diagnostic_only') is True and summary.get('no_percentile_or_target_pass') is True,
                f'{lane}: diagnostic scope flags missing')
        require(summary.get('n_per_kind')==3,f'{lane}: triplet size mismatch')
        if lane=='timing':
            require(summary.get('instrumentation_overhead_qualified') is False and
                    summary.get('queue_wait_measured') is False,'timing summary overclaims')
            for kind in KINDS:
                subset=[x['metrics'] for x in rows_ if x['kind']==kind]
                names=set(subset[0]);require(all(set(x)==names for x in subset),f'{kind} metric set differs')
                require(set(summary['kinds'][kind])==names,f'{kind} summary metric set differs')
                for metric in names:
                    vals=[x[metric] for x in subset]
                    expected_stat={'min':min(vals),'median':statistics.median(vals),'max':max(vals)}
                    for stat,value in expected_stat.items():
                        require(abs(float(summary['kinds'][kind][metric][stat])-value)<1e-9,
                                f'{kind}/{metric}/{stat} summary mismatch')
            profiles=[x['profile']['nanoseconds'] for x in rows_ if x['kind']=='on']
            residuals=[x['profile']['other_service_cleanup_and_overhead_ns'] for x in rows_ if x['kind']=='on']
            phase_names=set(profiles[0])
            require(all(set(x)==phase_names for x in profiles),'enabled timing profile phase set changed')
            require(set(summary.get('on_profile_phase_ms',{}))==phase_names,
                    'timing summary profile phase set differs')
            for phase in phase_names:
                vals=[x[phase]/1e6 for x in profiles]
                expected_stat={'min':min(vals),'median':statistics.median(vals),'max':max(vals)}
                for stat,value in expected_stat.items():
                    require(abs(float(summary['on_profile_phase_ms'][phase][stat])-value)<1e-9,
                            f'profile phase summary {phase}/{stat} differs from raw block')
            expected_residual={'min':min(residuals)/1e6,'median':statistics.median(residuals)/1e6,
                               'max':max(residuals)/1e6}
            for stat,value in expected_residual.items():
                require(abs(float(summary['on_other_service_cleanup_and_overhead_ms'][stat])-value)<1e-9,
                        f'outer-minus-leaf diagnostic summary/{stat} differs from raw blocks')
        else:
            # Timing metrics from the observer lane remain per-trial diagnostic
            # rows only. They are never pooled into the unobserved timing lane.
            for kind in KINDS:
                subset=[x['memory'] for x in rows_ if x['kind']==kind]
                for summary_key,field in [('sampled_max_phys_footprint_bytes','observed_max_phys_footprint_bytes'),
                                          ('sampled_max_resident_size_bytes','observed_max_resident_size_bytes')]:
                    vals=[x[field] for x in subset]
                    expected_stat={'min':min(vals),'median':statistics.median(vals),'max':max(vals)}
                    for stat,value in expected_stat.items():
                        require(abs(float(summary['kinds'][kind][summary_key][stat])-value)<1e-9,
                                f'memory {kind}/{summary_key}/{stat} mismatch')
    commands=read(ROOT/'measurement-commands.json')
    require(len(commands)==54 and all(x.get('exit')==0 for x in commands),
            'measurement receipt must contain 54 successful make/run/parse commands')
    expected_commands=[]
    for lane,lane_rotation,runner in [('timing',rotation,'run'),('memory',rotation,'observer')]:
        for round_no,order in enumerate(lane_rotation,1):
            for kind in order:
                stem=f'{lane}-{round_no}-{kind}'
                expected_commands.extend([stem+'-make',stem if lane=='timing' else stem+'-'+runner,stem+'-parse'])
    require([c.get('name') for c in commands]==expected_commands,
            'measurement command list does not match the exact two-lane triplet plan')
    return {'timing_trials':len(timing),'memory_trials':len(memory),'profile_blocks':6,
            'timing_summary':read(ROOT/'timing-summary.json'),'memory_summary':read(ROOT/'memory-summary.json')}

def verify_supporting_gates():
    go=read(ROOT/'FINAL-PACKAGING-GO.json');require(go.get('authorized') is True,'Root GO absent/unauthorized')
    build=read(ROOT/'build-receipt.json')
    require([r.get('name') for r in build]==['format-check','build'] and all(r.get('exit')==0 for r in build),
            'profile source format/release build receipt incomplete')
    ob=read(ROOT/'observer-build-receipt.json');res=read(ROOT/'observer-build-resumed-receipt.json')
    require(isinstance(ob,list) and len(ob)==2 and ob[0].get('exit')==0 and ob[1].get('exit')!=0 and
            res.get('exit')==0 and res.get('source_unchanged') is True,
            'observer initial link-failure/resumed-build history differs')
    parser=read(ROOT/'parser-tests.json')
    require(parser.get('exit')==0 and parser.get('source_unchanged') is True,
            'synthetic profile parser tests did not pass')
    test_source=(ROOT/'test_profile_parser.py').read_text()
    test_log=(ROOT/'parser-tests.stderr').read_text()
    require(test_source.count('    def test_')==10 and 'Ran 10 tests' in test_log and
            'OK' in test_log,'synthetic parser must retain ten passing cases')
    driver=read(ROOT/'measurement-driver-preflight-tests.json')
    require(driver.get('guarded_manifests_pass') is True and driver.get('negative_cases')==8 and
            driver.get('measurement_not_started') is True,'measurement driver guard negatives incomplete')
    smoke=read(ROOT/'observer-smoke-resumed/summary.json')
    require(smoke.get('cases_passed')==5 and len(smoke.get('cases',[]))==5 and
            all(c.get('passed') is True for c in smoke['cases']),'resumed observer smoke did not pass all five cases')
    smoke_commands=read(ROOT/'observer-smoke-resumed/commands.json')
    smoke_command_exits={x['name']:x['exit'] for x in smoke_commands}
    for case in smoke['cases']:
        receipt=read(case['receipt'])
        require(receipt.get('reason')==case['expected_reason'] and
                receipt.get('valid_memory_trial')==case['expected_valid'] and
                receipt.get('terminal') is True and
                smoke_command_exits.get(case['case'])==case['expected_exit'],
                f"observer smoke receipt mismatch: {case['case']}")
    variants=smoke.get('runtime_fault_variants_are_separate_binaries',{})
    require(set(variants)=={'timeout','sample-refusal'},'observer smoke fault-variant set changed')
    for name,v in variants.items():
        source=Path(v['source'])
        require(source.is_file() and sha(source)==v['sha256'],f'{name} fault source hash mismatch')
        require(Path(v['binary']).is_file() and sha(v['binary'])==v['binary_sha256'],
                f'{name} fault binary hash mismatch')
        require(set(v['only_exact_replacement'])=={'before','after'},f'{name} fault replacement metadata malformed')
        base=(ROOT/'memory_observer.rs').read_text()
        before_text,after_text=v['only_exact_replacement']['before'],v['only_exact_replacement']['after']
        require(base.count(before_text)==1 and source.read_text()==base.replace(before_text,after_text,1),
                f'{name} fault source is not the single recorded substitution')
    run_out=(ROOT/'measurement-driver.stdout').read_text().splitlines()
    run_result=json.loads(run_out[0]) if len(run_out)==1 else {}
    require(run_result.get('done') is True and run_result.get('timing_trials')==9 and
            run_result.get('memory_trials')==9 and (ROOT/'measurement-driver.stderr').read_bytes()==b'',
            'measurement driver terminal receipt is missing, incomplete, or failed')
    build_out=(ROOT/'build-driver.stdout').read_text().splitlines()
    build_rows=[json.loads(line) for line in build_out]
    require(len(build_rows)==3 and all(x.get('exit')==0 for x in build_rows[:2]) and
            build_rows[-1].get('done') is True and build_rows[-1].get('app_sha256')==PRIVATE_APP_SHA and
            (ROOT/'build-driver.stderr').read_bytes()==b'',
            'source build driver did not terminate cleanly after two successful steps')
    owner=read(ROOT/'measurement-owner.json')
    require(owner.get('frozen_runners') and
            all(Path(p).is_file() and sha(p)==h for p,h in owner['frozen_runners'].items()),
            'measured runner/manifest bytes differ from measurement entry freeze')
    review=read(ROOT/'independent-review.json')
    require(review.get('status')=='no_findings' and review.get('findings')==[],
            'independent source review has findings')
    return {'go':go,'build_steps':len(build),'initial_observer_build_exit':ob[1]['exit'],
            'resumed_observer_build_exit':res['exit'],'parser':parser,'driver_negative_cases':8,
            'observer_smoke_cases':5,'review_status':review['status']}

def include_top_file(p):
    if p.name in {'source.tar'} or p.suffix in {'.deadpan','.rlib','.a','.dylib','.so','.mp4'}:return False
    if re.match(r'^(timing|memory)-[1-3]-(control|off|on)(?:-|\.)',p.name):return False
    return p.suffix.lower() in SAFE_SUFFIXES

def main():
    require(not DEST.exists() and not STAGE.exists(),'destination or staging package already exists')
    support=verify_supporting_gates()
    before,after,inputs,basis=verify_sources()
    bins,observer,control=verify_binaries()
    measurement=verify_trials()
    STAGE.mkdir(parents=True)
    try:
        # Bounded top-level evidence; extension whitelist excludes executables,
        # source archives, SQLite packages, target/source trees and media files.
        for p in sorted(ROOT.iterdir()):
            if p.is_file() and include_top_file(p):copy_payload(p,Path('raw')/p.name)
        for folder in ('first-observer-link-failure','observer-smoke','observer-smoke-resumed'):
            copy_tree(ROOT/folder,Path('history')/folder)
        copy_tree(ROOT/'worker-notes','provenance/worker-notes')
        # Preserve report trees, logs and maps for every timing and observer trial.
        for lane in ('timing','memory'):
            for r in range(1,4):
                for kind in KINDS:
                    key=f'{lane}-{r}-{kind}'
                    for p in sorted(ROOT.iterdir()):
                        if p.is_file() and p.name.startswith(key) and p.suffix.lower() in SAFE_SUFFIXES:
                            copy_payload(p,Path('trials')/lane/key/p.name)
                    report=ROOT/f'{key}-report'
                    copy_tree(report,Path('trials')/lane/key/'report')
        # Exact baseline/final sources, with a truthful absent baseline for the new module.
        for rel,digest in after.items():
            if rel not in CHANGED:continue
            source=ROOT/'source'/rel
            require(sha(source)==digest,f'final source mismatch: {rel}')
            copy_payload(source,Path('sources/final')/(rel+'.rs.txt'))
            baseline=ROOT/'worker-notes/before'/f'{rel}.txt'
            if rel in before:
                require(baseline.is_file() and sha(baseline)==before[rel],f'baseline snapshot mismatch: {rel}')
                copy_payload(baseline,Path('sources/before')/(rel+'.rs.txt'))
            else:
                require(rel=='crates/deadpan-core/src/leaf_profile.rs',f'unexpected missing baseline source: {rel}')
                (STAGE/'sources/before'/f'{rel}.rs.txt.note').parent.mkdir(parents=True,exist_ok=True)
                (STAGE/'sources/before'/f'{rel}.rs.txt.note').write_text('Newly added overlay module; no baseline source existed.\n')
        # Regenerate a formatted diff against exact baseline snapshots, not the preformat patch.
        diff=[]
        for rel in CHANGED:
            base=ROOT/'worker-notes/before'/f'{rel}.txt'
            final=ROOT/'source'/rel
            a=base.read_text().splitlines(keepends=True) if base.is_file() else []
            b=final.read_text().splitlines(keepends=True)
            diff.extend(difflib.unified_diff(a,b,fromfile='a/'+rel,tofile='b/'+rel))
        (STAGE/'formatted-overlay.patch').write_text(''.join(diff))
        # Exact checked observer source as .rs.txt; do not retain generated binaries.
        copy_payload(ROOT/'memory_observer.rs',Path('sources/observer/memory_observer.rs.txt'))
        copy_payload(Path(__file__),'package.py')
        # Root report is a scratch draft at packaging time, retained as a draft.
        copy_payload(Path(__file__).parent/'report-draft.md','report-draft.md')
        verified={'source_commit':BASE_COMMIT,'baseline_rust_paths':len(before),'candidate_rust_paths':len(after),
                  'changed_paths':CHANGED,'input_paths':inputs,'private_app_sha256':bins['deadpan-app']['sha256'],
                  'control_app_sha256':control['sha256'],'memory_observer_sha256':observer['sha256'],
                  'candidate_perf_sha256':PERF_SHA,'supporting_gates':support,'measurement':measurement,
                  'schema_unchanged':{'core':58,'sqlite':87},
                  'scope':{'instrumentation_overhead_qualified':False,'queue_wait_measured':False,
                           'memory_is_sampled_whole_app_lifetime_lower_bound':True,
                           'exact_peak_per_leaf_memory_or_memory_ceiling_claim':False}}
        canonical(verified,STAGE/'verified-gates.json')
        files=[]
        for p in sorted(STAGE.rglob('*'),key=lambda x:x.as_posix()):
            if p.is_file() and p != STAGE/'inventory.json':
                if p.suffix=='.gz':
                    with gzip.open(p,'rb') as stream:stream.read()
                files.append({'path':p.relative_to(STAGE).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p)})
        inventory={'total_files':len(files),'total_bytes':sum(x['bytes'] for x in files),'files':files}
        canonical(inventory,STAGE/'inventory.json')
        require(read(STAGE/'inventory.json')==inventory,'inventory reread differs')
        for row in files:
            p=STAGE/row['path'];require(p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],
                                        f'payload reread mismatch: {row["path"]}')
        invsha=sha(STAGE/'inventory.json')
        require(not DEST.exists(),'destination appeared during pack')
        STAGE.rename(DEST)
        print(json.dumps({'destination':str(DEST),'total_files':len(files),'total_bytes':inventory['total_bytes'],
                          'inventory_sha256':invsha,'timing_trials':9,'memory_trials':9},sort_keys=True))
    except Exception:
        shutil.rmtree(STAGE,ignore_errors=True)
        raise

if __name__=='__main__':main()
