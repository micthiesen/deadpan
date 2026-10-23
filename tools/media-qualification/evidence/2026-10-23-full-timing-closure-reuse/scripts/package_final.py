#!/usr/bin/env python3
"""Bounded final evidence packager; run only by root after all gates are terminal."""
from __future__ import annotations
import argparse, gzip, hashlib, json, re, shutil, subprocess, statistics
from pathlib import Path

S=Path("/tmp/deadpan-full-timing-closure-reuse-qualification-2026-10-23")
REPO=Path("/Users/michael/Code/deadpan")
DEST=REPO/"tools/media-qualification/evidence/2026-10-23-full-timing-closure-reuse"
GO=S/"FINAL-PACKAGING-GO.json"
BASE="ca40995b64fc4acdc0a5e046660a36a1faa7cee5"
CHANGED=["crates/deadpan-core/src/audio_reference.rs"]
NEW_TESTS=[
 "full_layout_reuse_leaf_closure_preserves_nested_wire_and_fields",
 "full_layout_reuse_partial_and_provisional_tables_still_rebuild",
 "full_layout_reuse_late_unknown_alias_precedes_identity_and_clock_fallbacks",
 "full_layout_reuse_retains_canonical_wire_limit_refusal",
 "full_layout_reuse_ten_thousand_hold_interior_pause_matches_reference",
]
LIMIT=100_000

def need(ok,msg):
    if not ok: raise SystemExit(msg)
def load(p):
    try: return json.loads(p.read_text())
    except Exception as e: raise SystemExit(f"missing/invalid JSON {p}: {e}")
def sha_bytes(b): return hashlib.sha256(b).hexdigest()
def sha(p):
    h=hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda:f.read(1024*1024),b""): h.update(b)
    return h.hexdigest()
def required(p):
    need(p.is_file(),f"missing terminal evidence: {p}")
    return p

def test_phase(name, expected_tests, expected_skips):
    result=load(required(S/f"{name}-result.json"))
    need(result.get("source_base")==BASE and result.get("source_location")=="candidate" and
         result.get("exit")==0 and result.get("tests")==expected_tests and
         result.get("sources_inputs_executables_unchanged") is True,
         f"{name}: phase result does not match expected successful run")
    listing=load(required(S/f"{name}-list.json"))["rust-suites"]
    matches=0; suite_matches=0
    for suite in listing.values():
        selected=[case for case in suite.get("testcases",{}).values()
                  if case.get("filter-match",{}).get("status")=="matches"]
        matches+=len(selected)
        suite_matches+=bool(selected)
    binaries=load(required(S/f"{name}-binaries.json"))
    need(len(binaries)==suite_matches and sum(len(r.get("tests",[])) for r in binaries.values())==matches,
         f"{name}: binary count does not equal matching suites/testcases")
    need(matches==expected_tests and result.get("binaries")==suite_matches,
         f"{name}: matching-suite/testcase counts mismatch")
    log=required(S/f"{name}-run.log").read_text()
    m=re.search(r"(\d+) tests run: (\d+) passed(?: \((\d+) leaky\))?, (\d+) skipped",log)
    need(m is not None,f"{name}: nextest summary absent")
    run,passed,leaky,skipped=map(lambda v: int(v) if v is not None else 0,m.groups())
    need(run==expected_tests and passed==expected_tests and skipped==expected_skips,
         f"{name}: passed/skipped counts mismatch")
    for row in binaries.values():
        p=Path(row["path"])
        need(p.is_file() and sha(p)==row["sha256"],f"{name}: retained executed test executable hash mismatch")
    return {"tests":run,"passed":passed,"skipped":skipped,"leaky":leaky,"matching_suites":suite_matches}

def verify_source_guards():
    manifest=load(required(S/"candidate-all-core-source-identities.json"))
    control=load(required(S/"qualified-control-source-identities.json"))
    adoption=load(required(S/"adoption-source-identities.json"))
    need(set(manifest)==set(control),"candidate/control Rust path sets differ")
    changed={path for path in manifest if manifest[path]!=control[path]}
    need(changed==set(CHANGED) and adoption.get("path") in changed and
         adoption.get("before")==control.get(adoption.get("path")) and
         adoption.get("tested_after")==manifest.get(adoption.get("path")),
         "candidate differs from qualified control outside the adopted one-file change")
    names=set(subprocess.check_output(["rg","--files","-g","*.rs"],cwd=REPO,text=True).splitlines())
    need(names==set(manifest) and len(manifest)==2050,"current Rust path set differs from frozen Core manifest")
    for path,expected in manifest.items():
        p=REPO/path
        need(p.is_file() and sha(p)==expected,f"current Rust source guard failed: {path}")
    inputs=load(required(S/"candidate-all-core-inputs.json"))
    need(len(inputs)==6,"expected six frozen input identities")
    for path,expected in inputs.items():
        p=REPO/path
        need(p.is_file() and sha(p)==expected,f"current input identity changed: {path}")
    for stage in ["candidate-broad","candidate-ui","candidate-strict","ui-lint",
                  "debug-media","release-media","release-native"]:
        source_map=load(required(S/f"{stage}-source-identities.json"))
        input_path=(S/f"{stage}-lock.json" if stage in {"ui-lint","debug-media","release-media","release-native"}
                    else S/f"{stage}-inputs.json")
        input_map=load(required(input_path))
        need(source_map==manifest,f"{stage}: frozen Rust source map differs")
        need(input_map==inputs,f"{stage}: six input map differs")
    return manifest,inputs

def verify_exec(path,expected,label):
    p=Path(path)
    need(p.is_file() and sha(p)==expected,f"executed binary identity mismatch: {label}")
def verify_manifest_binaries(path,label):
    data=load(required(path))
    if "helpers" in data:
        for name,row in data["helpers"].items():
            verify_exec(row.get("executed",row.get("path")),row["sha256"],f"{label} retained executed helper {name}")
            # Later build phases legitimately replace shared target outputs.
            # The phase producer checked the source at execution; retain and
            # verify its copied executed helper here.
        for name,row in data.get("tests",{}).items():
            if isinstance(row,str): verify_exec(name,row,f"{label} test")
            else: verify_exec(row["path"],row["sha256"],f"{label} test {name}")
    else:
        for name,row in data.items():
            executable=row.get("executed",row.get("path"))
            verify_exec(executable,row["sha256"],f"{label} {name}")
    return data

def verify_media_reports():
    steps=load(required(S/"qualification-receipt.json"))
    need(len(steps)==8 and [x["step"] for x in steps]==list(range(1,9)) and all(x["exit"]==0 for x in steps),
         "all eight qualification commands must pass")
    for prefix,names in {
        "candidate-strict":["check","format"],
        "debug-media":["build","neutral"],
        "release-media":["build","neutral","aligned"],
        "release-native":["build","performance"],
    }.items():
        for stage in names:
            row=load(required(S/f"{prefix}-{stage}-receipt.json"))
            need(row.get("exit")==0,f"{prefix}-{stage} command failed")
    ui=load(required(S/"ui-lint-receipt.json"))
    need(len(ui)==2 and {r["name"] for r in ui}=={"clippy","format"} and all(r["exit"]==0 for r in ui),
         "strict UI Clippy/format receipts incomplete")
    for path in [S/"candidate-strict-check-receipt.json",S/"candidate-strict-format-receipt.json"]:
        need(load(required(path)).get("exit")==0,f"strict receipt failed: {path.name}")

    media_results=[]
    for phase in ("debug-media","release-media"):
        verify_manifest_binaries(S/f"{phase}-binaries.json",phase)
        labels=("neutral",) if phase=="debug-media" else ("neutral","aligned")
        for name in labels:
            report=load(required(S/f"{phase}-{name}-fixture.json"))
            need(report.get("passed") is True and report.get("failures")==[],f"{phase}-{name} media check failed")
            offsets=report.get("offsets")
            need(isinstance(offsets,list) and offsets and all(len(row)==2 and row[1]==0 for row in offsets),
                 f"{phase}-{name} media offsets are not zero")
            need(report.get("independent_DSP_claim") is False and report.get("listening_claim") is False,
                 f"{phase}-{name} media claim flags changed")
            if name=="neutral":
                need(report.get("frames")==15 and report.get("pictures")==15 and
                     report.get("canonical_samples_compared")==24024,
                     f"{phase} neutral media counts differ")
                need(report.get("authored_limited_and_limiter_gain_bits_equal") is True,
                     f"{phase} neutral PCM equality flag changed")
            else:
                need(report.get("frames")==19 and report.get("pictures")==19 and
                     report.get("neutral_group_and_split_canonical_samples_compared")==24024 and
                     report.get("final_canonical_samples_compared")==30430 and
                     report.get("move_PCM_translation_claim") is False,
                     "release aligned PCM/frame counts or claim flags differ")
            media_results.append({"phase":phase,"fixture":name,"frames":report["frames"],
                                  "pictures":report["pictures"],"offsets":offsets,
                                  "canonical_samples_compared":report.get("canonical_samples_compared"),
                                  "final_canonical_samples_compared":report.get("final_canonical_samples_compared"),
                                  "claim_flags":{k:v for k,v in report.items() if k.endswith("_claim") or k.endswith("_equal")}})
    native=verify_manifest_binaries(S/"release-native-binaries.json","release-native")
    need(set(native)=={"deadpan-app","deadpan-media-worker","deadpan-transcribe","deadpan-track"},
         "release-native binary manifest incomplete")
    control=load(required(S/"qualified-control-native-binaries.json"))
    need(set(control)==set(native),"qualified control binary manifest incomplete")
    for name,row in control.items(): verify_exec(row["executed"],row["sha256"],f"qualified control {name}")

    portable=load(required(S/"owner-portable-check/receipt.json"))
    need(portable.get("safe_relative_regular_files_only") is True and len(portable.get("cases",[]))==2,
         "portable validation receipt incomplete")
    for case in portable["cases"]:
        need(case.get("regular_files")==9 and len(case.get("commands",[]))==2 and
             all(c.get("exit")==0 and c.get("package_paths_and_bytes_unchanged") is True for c in case["commands"]),
             f"portable case did not validate/doctor unchanged: {case.get('label')}")

    summaries=[];workflow=0;kestrel=0;warning_counts={};severity_counts={}
    report_paths=[S/"release-native-performance/report.json"]+[S/f"native-{name}/report.json" for name in
        ["macros","named-registers","dot-repeat","range-set-selectors","transcript-search-selectors"]]
    need(len(report_paths)==6,"expected six native/UI replay reports")
    for path in report_paths:
        report=load(required(path))
        need(report.get("failed") is False,f"native report failed: {path}")
        release_app=load(required(S/"release-native-binaries.json"))["deadpan-app"]
        need(report.get("metadata",{}).get("binary_sha256")==release_app["sha256"],
             f"native report app identity differs from final release build: {path}")
        local_work=local_k=0
        for scenario in report.get("scenarios",[]):
            checks=scenario.get("checks",[])
            need(all(c.get("passed") is True for c in checks),f"native failed check: {path} / {scenario.get('name')}")
            for finding in scenario.get("findings",[]):
                severity=finding.get("severity","warning")
                need(severity in {"info","warning"},f"native error-severity finding: {path} / {scenario.get('name')}")
                severity_counts[severity]=severity_counts.get(severity,0)+1
                prefix=finding.get("message","").split(":",1)[0].strip()
                prefix=re.sub(r"\b\d+\b","#",prefix)
                warning_counts[prefix]=warning_counts.get(prefix,0)+1
            if scenario.get("name")=="kestrel-shortcuts": local_k+=len(checks)
            else: local_work+=len(checks)
        workflow+=local_work;kestrel+=local_k
        summaries.append({"report":str(path.relative_to(S)),"workflow_checks":local_work,"kestrel_checks":local_k})
    need(workflow==1340 and kestrel==6,"six native reports do not contain 1,340 workflow + 6 Kestrel checks")
    return {"workflow_checks":workflow,"kestrel_checks":kestrel,"reports":summaries,
            "finding_severity_counts":severity_counts,"finding_prefix_counts":warning_counts,
            "media_fixtures":media_results}

def verify_added_tests():
    for phase, tests in {
      "candidate-all-core":NEW_TESTS,
    }.items():
        listing=load(required(S/f"{phase}-list.json"))["rust-suites"]
        log=required(S/f"{phase}-run.log").read_text()
        for test in tests:
            matches=[case for suite in listing.values() for name,case in suite.get("testcases",{}).items()
                     if name==test or name.endswith("::"+test)]
            need(len(matches)==1 and matches[0].get("filter-match",{}).get("status")=="matches",
                 f"added regression test not selected: {phase} {test}")
            need(any("PASS" in line and test in line for line in log.splitlines()),
                 f"added regression test did not pass: {phase} {test}")

def verify_post():
    rows=load(required(S/"post-receipt.json"))
    need(len(rows)==26 and all(r.get("exit")==0 for r in rows),"all 26 post-qualification commands must pass")
    row_names={r.get("name") for r in rows}
    expected_post={"portable"}
    expected_post.update("native-"+n for n in
        ["macros","named-registers","dot-repeat","range-set-selectors","transcript-search-selectors"])
    for pair in range(1,6):
        for kind in ("control","candidate"):
            key=f"pair-{pair}-{kind}"
            expected_post.update({key+"-make",key})
    need(row_names==expected_post,"post receipt does not contain exactly five native, one portable, and ten make/run pairs")
    verify_added_tests()
    trials=load(required(S/"pause-trials.json"))
    summary=load(required(S/"pause-summary.json"))
    need(len(trials)==10 and summary.get("n_per_kind")==5 and summary.get("diagnostic_only") is True and
         summary.get("no_percentile_or_target_pass") is True and
         {r["kind"] for r in trials}=={"control","candidate"},"timing series incomplete or overclaimed")
    expected_pairs={(pair,kind) for pair in range(1,6) for kind in ("control","candidate")}
    expected_order=[(pair,kind) for pair in range(1,6) for kind in
                    (["control","candidate"] if pair%2 else ["candidate","control"])]
    need([(r['pair'],r['kind']) for r in trials]==expected_order,"timing pair order differs from alternating producer")
    trial_rows={}
    for r in trials:
        key=f"pair-{r['pair']}-{r['kind']}"
        need(r.get("checks")==7 and r.get("source_binary_guards") is True and
             r.get("authored_input_unchanged") is True and r.get("wal_empty") is True and
             len(r.get("load",[]))==3 and r["load"][0]<=3.0,f"timing row guard failed: {key}")
        trial_rows[(r["pair"],r["kind"])]=r
        before=load(required(S/f"{key}-input-before.json"))
        after=load(required(S/f"{key}-input-after.json"))
        sidecars={"project.sqlite-shm","project.sqlite-wal"}
        normalized=lambda d:{k:v for k,v in d.items() if k not in sidecars}
        need(normalized(before)==normalized(after),f"timing fixture changed beyond SQLite sidecars: {key}")
        need(before["project.sqlite"]["sha256"]==after["project.sqlite"]["sha256"] and
             after.get("project.sqlite-wal",{}).get("bytes",0)==0,f"timing input DB/WAL guard failed: {key}")
        report=load(required(S/f"{key}-report/report.json"))
        need(report.get("failed") is False and
             report["metadata"]["fixture_sha256"]==before["project.sqlite"]["sha256"]==after["project.sqlite"]["sha256"],
             f"timing report fixture identity/failure: {key}")
        c=[c for c in report.get("scenarios",[]) if c.get("name")=="large-first-pause"]
        need(len(c)==1 and len(c[0].get("checks",[]))==7 and all(x["passed"] for x in c[0]["checks"]),
             f"seven pause checks missing/failed: {key}")
    need(set(trial_rows)==expected_pairs,"timing trials do not cover exactly five control/candidate pairs")
    # LEAK annotations are retained as warnings, not failed assertions. Root
    # must preserve the isolated reruns and verify every listed case is clean.
    leak_lines=[line for phase in ('candidate-all-core','candidate-broad','candidate-ui')
                for line in required(S/f'{phase}-run.log').read_text().splitlines()
                if re.search(r'\bLEAK\b',line)]
    leak_rows=[]
    if leak_lines:
        leak=load(required(S/"isolated-leak-reruns.json"))
        leak_rows=leak if isinstance(leak,list) else leak.get("reruns",[])
        need(leak_rows,"isolated LEAK rerun receipt is empty")
    for row in leak_rows:
        need(row.get("exit")==0 and row.get("passed") is True and row.get("leaky") is False,
             f"isolated LEAK rerun failed or remains leaky: {row.get('test')}")
        need(row.get("source_binary_guards") is True,
             f"isolated LEAK rerun lacked source/binary guards: {row.get('test')}")
    native=verify_manifest_binaries(S/"release-native-binaries.json","candidate release")
    control=load(required(S/"qualified-control-native-binaries.json"))
    candidate_row=native["deadpan-app"];cand_app=candidate_row["sha256"]; control_app=control["deadpan-app"]["sha256"]
    verify_exec(candidate_row["source"],cand_app,"candidate release app source")
    verify_exec(candidate_row["executed"],cand_app,"candidate release app copy")
    need(control_app=="c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037",
         "qualified control app identity differs")
    metric_names=set(trials[0]["metrics"])
    for kind in ("control","candidate"):
        subset=[r for r in trials if r["kind"]==kind]
        need(len(subset)==5,"expected five trials for "+kind)
        for r in subset:
            expected=control_app if kind=="control" else cand_app
            need(r.get("binary_sha256")==expected,f"timing row binary mismatch: pair {r['pair']} {kind}")
            report=load(required(S/f"pair-{r['pair']}-{kind}-report/report.json"))
            need(report["metadata"]["binary_sha256"]==expected,f"timing report binary mismatch: pair {r['pair']} {kind}")
            need(report["metadata"].get("build_profile")=="release",
                 f"timing report was not produced by release binary: pair {r['pair']} {kind}")
            kestrel=[x for x in report.get("scenarios",[]) if x.get("name")=="kestrel-shortcuts"]
            need(len(kestrel)==1 and len(kestrel[0].get("checks",[]))==1 and
                 all(c.get("passed") is True for c in kestrel[0]["checks"]),
                 f"timing report lacks its one passing Kestrel check: pair {r['pair']} {kind}")
        need(set(summary["kinds"][kind])==metric_names,f"timing summary metric set differs for {kind}")
        need(all(set(r["metrics"])==metric_names for r in subset),
             f"timing rows do not share one metric set for {kind}")
        for metric in metric_names:
            values=[r["metrics"][metric] for r in subset]
            actual={"min":min(values),"median":statistics.median(values),"max":max(values)}
            recorded=summary["kinds"][kind][metric]
            need(set(recorded)==set(actual) and all(abs(float(recorded[k])-actual[k])<1e-9 for k in actual),
                 f"timing summary does not recompute for {kind}/{metric}")
    return {"post_commands":len(rows),"timing_trials":len(trials),"candidate_app_sha256":cand_app,
            "control_app_sha256":control_app,"timing_medians":summary}

def verify_all():
    need(GO.is_file(),"root final packaging GO marker is absent")
    go=load(GO);need(go.get("authorized") is True,"GO marker not authorized")
    source_manifest,inputs=verify_source_guards()
    phases={
      "candidate-all-core":test_phase("candidate-all-core",1522,0),
      "candidate-broad":test_phase("candidate-broad",3506,740),
      "candidate-ui":test_phase("candidate-ui",1296,11),
    }
    need(phases["candidate-all-core"]["matching_suites"]==69,"Core matching-suite count changed")
    for name in ("candidate-strict-check-receipt.json","candidate-strict-format-receipt.json"):
        need(load(required(S/name)).get("exit")==0,f"strict check failed: {name}")
    ui_lint=load(required(S/"ui-lint-receipt.json"))
    need(len(ui_lint)==2 and all(r["exit"]==0 for r in ui_lint),"strict UI checks incomplete")
    binary_manifests={}
    for name in ("candidate-all-core","candidate-broad","candidate-ui"):
        binary_manifests[name]=verify_manifest_binaries(S/f"{name}-binaries.json",name)
    for name in ("candidate-broad","candidate-ui"):
        helper=load(required(S/f"{name}-helpers.json"))
        for tool,row in helper.items():
            verify_exec(row["path"],row["sha256"],f"{name} helper {tool}")
    reports=verify_media_reports()
    post=verify_post()
    review=load(required(Path("/tmp/deadpan-full-timing-closure-reuse-2026-10-23/independent-review.json")))
    need(review.get("status")=="no_findings" and review.get("findings")==[],
         "independent source review has findings or is incomplete")
    adoption={}
    phase_map=load(S/"candidate-all-core-source-identities.json")
    adoption_record=load(required(S/"adoption-source-identities.json"))
    for path in CHANGED:
        need(adoption_record.get("path")==path and adoption_record.get("tested_after")==phase_map[path],
             f"adoption source hash mismatch: {path}")
        live=REPO/path;need(sha(live)==phase_map[path],f"current changed source differs: {path}")
        adoption[path]=phase_map[path]
    return {"source_base":BASE,"rust_files":len(source_manifest),"input_files":len(inputs),
            "phases":phases,"native_report_checks":reports,"post":post,"reviews":"no findings",
            "changed_sources":adoption,"go":go}

def put_bytes(raw:bytes,dst:Path,kind:str):
    dst.parent.mkdir(parents=True,exist_ok=True)
    if len(raw)>LIMIT and kind in {"json","txt","md","log","stdout","stderr","html"}:
        out=Path(str(dst)+".gz")
        with out.open("wb") as f:
            with gzip.GzipFile(filename="",mode="wb",fileobj=f,compresslevel=9,mtime=0) as g:g.write(raw)
        with gzip.open(out,"rb") as f:need(f.read()==raw,f"compression verification failed: {dst}")
    else: dst.write_bytes(raw)
def copy_file(src:Path,dst:Path):
    put_bytes(src.read_bytes(),dst,src.suffix.lstrip("."))

def include_path(rel:Path):
    parts=rel.parts; n=rel.name
    if parts[0] in {"packaging-draft","next-profile-design","next-size-study","profile-design"}:return False
    if len(parts)==1 and (n=="next-replay-count-study.md" or n.startswith("next-")):return False
    if any(p=="target" or p.endswith(("-extracted","-overlays","-source")) for p in parts):return False
    if any(p.endswith((".deadpan",".tar",".tar.zst",".tar.gz",".mp4")) for p in parts):return False
    if any(p in {"bin","focused-bin"} for p in parts):return False
    if rel.suffix not in {".json",".py",".md",".txt",".log",".stdout",".stderr",".html",".patch",".gz"}:return False
    return True

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--finalize",action="store_true")
    args=ap.parse_args()
    need(args.finalize,"pass --finalize only when root has confirmed every driver terminal")
    gates=verify_all()
    stage=DEST.with_name(DEST.name+".staging")
    need(not DEST.exists() and not stage.exists(),"destination or staging already exists")
    stage.mkdir(parents=True)
    try:
        # Keep bounded receipts, source/input/binary manifests, lists, scripts, reports and logs.
        for p in sorted(S.rglob("*")):
            if p.is_file() and include_path(p.relative_to(S)):
                copy_file(p,stage/"raw"/p.relative_to(S))
        # Preserve this producer explicitly although packaging-draft is excluded from recursive capture.
        put_bytes(Path(__file__).read_bytes(),stage/"scripts/package_final.py","py")
        # Exact adopted formatted Rust source, tied to frozen source-map and adoption hashes.
        manifest=load(S/"candidate-all-core-source-identities.json")
        adoption=load(required(S/"adoption-source-identities.json"))
        for path in CHANGED:
            frozen=S/"candidate-all-core-overlays"/(path+".txt")
            required(frozen)
            raw=frozen.read_bytes()
            need(sha_bytes(raw)==manifest[path]==adoption["tested_after"],
                 f"changed source overlay differs from tested source identity: {path}")
            put_bytes(raw,stage/"sources/final"/(path.replace("/","__")+".txt"),"txt")
            # Root's private source snapshot holds the exact pre-adoption file.
            original=Path('/tmp/deadpan-full-timing-closure-reuse-2026-10-23/worker-notes/before')/(path+'.txt')
            need(original.is_file() and sha(original)==adoption["before"],
                 "pre-adoption source snapshot mismatch")
            put_bytes(original.read_bytes(),stage/"sources/original"/(path.replace("/","__")+".txt"),"txt")
        # Preserve the adopted one-file patch and independent review/provenance.
        prototype=Path("/tmp/deadpan-full-timing-closure-reuse-2026-10-23")
        for rel in ("adoption.patch","independent-review.json","source-basis.json","formatted-overlay-audit.json"):
            source=(S/rel) if (S/rel).is_file() else (prototype/rel)
            copy_file(required(source),stage/"provenance"/rel)
        # Preserve only the three emitted qualification movies. Source videos
        # and generated project databases remain excluded.
        media_audit=[]
        for phase,fixture in (("debug-media","neutral"),("release-media","neutral"),("release-media","aligned")):
            report=load(required(S/f"{phase}-{fixture}-fixture.json"))
            movie=required(Path(report["movie"]))
            retained=S/"retained-media/media"/phase/fixture/movie.name
            required(retained)
            digest=sha(movie)
            need(sha(retained)==digest and retained.stat().st_size==movie.stat().st_size,
                 f"emitted qualification movie copy mismatch: {phase}/{fixture}")
            copy_file(retained,stage/"emitted-media"/phase/fixture/movie.name)
            media_audit.append({"phase":phase,"fixture":fixture,"name":movie.name,
                                "bytes":movie.stat().st_size,"sha256":digest,
                                "original_and_retained_equal":True,"verified_report":str((S/f"{phase}-{fixture}-fixture.json").relative_to(S))})
        (stage/"emitted-media-audit.json").write_text(json.dumps(media_audit,indent=2)+"\n")
        # Retain compact test/review/adoption facts; no executables, archives,
        # generated project packages or source video.
        gates["schema"]={"core":58,"sqlite":87}
        (stage/"verified-gates.json").write_text(json.dumps(gates,indent=2)+"\n")
        rows=[]
        for p in sorted(stage.rglob("*")):
            if p.is_file() and p!=stage/"inventory.json":
                rows.append({"path":p.relative_to(stage).as_posix(),"bytes":p.stat().st_size,"sha256":sha(p)})
                if p.suffix==".gz":
                    with gzip.open(p,"rb") as f:f.read()
        inventory={"files":rows,"total_files":len(rows),"total_bytes":sum(x["bytes"] for x in rows)}
        (stage/"inventory.json").write_text(json.dumps(inventory,indent=2)+"\n")
        check=load(stage/"inventory.json")
        need(check["total_files"]==len(check["files"]) and check["total_bytes"]==sum(x["bytes"] for x in check["files"]),
             "inventory counts/bytes mismatch")
        for row in rows:
            p=stage/row["path"]
            need(p.is_file() and p.stat().st_size==row["bytes"] and sha(p)==row["sha256"],
                 f"inventory reread failed: {row['path']}")
        inv_sha=sha(stage/"inventory.json")
        DEST.parent.mkdir(parents=True,exist_ok=True)
        stage.rename(DEST)
        print(json.dumps({"evidence":str(DEST),"total_files":inventory["total_files"],
                          "total_bytes":inventory["total_bytes"],"inventory_sha256":inv_sha},indent=2))
    except Exception:
        shutil.rmtree(stage,ignore_errors=True)
        raise
if __name__=="__main__":main()
