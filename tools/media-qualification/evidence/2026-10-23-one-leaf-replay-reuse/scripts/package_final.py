#!/usr/bin/env python3
"""Bounded final evidence packager; run only by root after all gates are terminal."""
from __future__ import annotations
import argparse, gzip, hashlib, json, math, re, shutil, subprocess, statistics, tarfile
from pathlib import PurePosixPath
from pathlib import Path

S=Path("/tmp/deadpan-one-leaf-replay-production-2026-10-23")
REPO=Path("/Users/michael/Code/deadpan")
DEST=REPO/"tools/media-qualification/evidence/2026-10-23-one-leaf-replay-reuse"
GO=S/"FINAL-PACKAGING-GO.json"
BASE="3a32c923358c0cb7ca53f821bfb6db3e01ebb466"
DELTA_PATH=S/"expected-source-delta.json"
CHANGED=[
 "crates/deadpan-cli/src/macros.rs", "crates/deadpan-core/src/audio_reference.rs",
 "crates/deadpan-core/src/compound.rs", "crates/deadpan-core/src/insert_time/retimed.rs",
 "crates/deadpan-core/src/lib.rs", "crates/deadpan-core/src/one_leaf_replay.rs",
 "crates/deadpan-core/src/semantic/planner.rs", "crates/deadpan-core/src/semantic/planner/content.rs",
 "crates/deadpan-core/src/semantic/planner/pause.rs", "crates/deadpan-core/src/semantic/planner/tests.rs",
 "crates/deadpan-core/tests/semantic_leaf_replay.rs", "crates/deadpan-store/src/compound.rs",
 "crates/deadpan-store/src/lib.rs", "crates/deadpan-store/tests/compound_preparation.rs",
 "crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs",
 "crates/deadpan-store/src/compound/tests.rs",
 "crates/deadpan-cli/tests/preview_export/group_endpoints_export.rs",
]
NEW_FILES=["crates/deadpan-core/src/one_leaf_replay.rs", "crates/deadpan-core/tests/semantic_leaf_replay.rs"]
NEW_TEST="one_leaf_replay_qualified_media_commit_export_matches_uncached"
LIMIT=100_000
TIMING_METRICS={
    "large_first_pause_input_to_commit_ms",
    "large_first_pause_input_to_offscreen_background_composition_ms",
}

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

def test_phase(name, expected_skips=None):
    result=load(required(S/f"{name}-result.json"))
    need(result.get("source_base")==BASE and result.get("source_location")=="candidate" and
         result.get("exit")==0 and
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
    need(matches==result.get("tests") and result.get("binaries")==suite_matches,
         f"{name}: matching-suite/testcase counts mismatch")
    log=required(S/f"{name}-run.log").read_text()
    m=re.search(r"(\d+) tests run: (\d+) passed(?: \((\d+) leaky\))?, (\d+) skipped",log)
    need(m is not None,f"{name}: nextest summary absent")
    run,passed,leaky,skipped=map(lambda v: int(v) if v is not None else 0,m.groups())
    need(run==matches and passed==matches and
         result.get("exit")==0 and (expected_skips is None or skipped==expected_skips),
         f"{name}: passed/skipped counts mismatch")
    for row in binaries.values():
        p=Path(row["path"])
        need(p.is_file() and sha(p)==row["sha256"],f"{name}: retained executed test executable hash mismatch")
    return {"tests":run,"passed":passed,"skipped":skipped,"leaky":leaky,"matching_suites":suite_matches}

def verify_source_guards():
    delta=load(required(S/"expected-source-delta.json"))
    need(delta.get("baseline_commit")==BASE and delta.get("baseline_rust_files")==2051
         and delta.get("candidate_rust_files")==2053
         and set(delta.get("changed",[]))==set(CHANGED)
         and set(delta.get("new",[]))==set(NEW_FILES),
         "expected source-delta receipt is incomplete or differs")
    baseline=load(required(S/"qualified-control-source-identities.json"))
    inputs=load(required(S/"qualified-control-inputs.json"))
    need(len(baseline)==2051 and len(inputs)==6,"qualified control source/input manifest shape differs")
    original=load(required(S/"candidate-all-core-source-identities.json"))
    core_inputs=load(required(S/"candidate-all-core-inputs.json"))
    final=load(required(S/"final-source-identities.json"))
    final_inputs=load(required(S/"final-input-identities.json"))
    need(len(original)==2053 and set(original)==set(baseline)|set(NEW_FILES)
         and {path for path in original if original[path]!=baseline.get(path)}==set(CHANGED),
         "original completed Core map differs outside the exact 17-file overlay")
    pcm_path="crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs"
    old_pcm_sha="58ce580c3def2cf42c809b7a059e21058cae591eea31284641148e1bca83bd1b"
    new_pcm_sha="1b6b7a8e342f4358c9f6f04c998cd1e4247603d5d96853305f6bbb58383ad802"
    need(len(final)==2053 and set(final)==set(original)
         and {path for path in final if final[path]!=original[path]}=={pcm_path}
         and original[pcm_path]==old_pcm_sha and final[pcm_path]==new_pcm_sha
         and {path:final[path] for path in final if path!=pcm_path}=={path:digest for path,digest in original.items() if path!=pcm_path},
         "final map is not exactly the reviewed single-file test-only PCM reader amendment")
    need(core_inputs==inputs and final_inputs==inputs,"frozen six-input identity map changed")
    reviewed_map_path=Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/formatted-source-identities.json")
    reviewed_map=load(required(reviewed_map_path))
    review_basis=load(required(S/"source-review-basis.json"))
    relocation_path="crates/deadpan-core/src/insert_time/retimed.rs"
    prototype_paths=set(CHANGED[:14])
    need(sha(reviewed_map_path)==review_basis.get("prototype_final_map_sha256")
         and len(reviewed_map)==2053
         and all(original[path]==reviewed_map[path] for path in prototype_paths-{relocation_path})
         and original[pcm_path]==review_basis.get("refactored_positive_test_sha256",{}).get(pcm_path),
         "thirteen unchanged prototype paths or historical positive-test hash differ")
    amendment=load(required(S/"pcm-reader-source-amendment.json"))
    amendment_review=load(required(S/"review/pcm-stage-reader-review.json"))
    amendment_path=amendment.get("only_changed_path")
    need(amendment_path==pcm_path
         and amendment.get("before_sha256")==old_pcm_sha
         and amendment.get("after_sha256")==new_pcm_sha
         and amendment.get("other_rust_files_byte_identical")==2052
         and amendment.get("six_inputs_byte_identical") is True
         and amendment.get("production_sources_changed") is False
         and amendment.get("legacy_default_reader_contract_preserved") is True
         and amendment.get("completed_core_broad_ui_results_preserved_with_actual_original_source_maps") is True,
         "test-only PCM reader amendment receipt does not pin its single-file change")
    need(amendment_review.get("status")=="no_findings" and amendment_review.get("findings")==[]
         and amendment_review.get("file")==pcm_path
         and amendment_review.get("before_sha256")==old_pcm_sha
         and amendment_review.get("final_formatted_sha256")==new_pcm_sha
         and amendment_review.get("legacy_reader_behavior_preserved") is True
         and amendment_review.get("cold_exact_byte_identical") is True
         and amendment_review.get("new_one_leaf_full_reader_call_sites")==6
         and amendment_review.get("source_defined_full_reader_invocations")==9,
         "PCM reader amendment review does not verify the legacy/new reader contracts")
    amendment_source=S/"pcm-stage-fix-notes"
    amendment_hashes=load(required(amendment_source/"source-hashes.json"))
    amendment_before=required(amendment_source/"aligned_group_recipe.before.rs.txt")
    amendment_proposed=required(amendment_source/"aligned_group_recipe.proposed.rs.txt")
    amendment_formatted=required(amendment_source/"formatted.rs.txt")
    need(amendment_hashes.get("before")==sha(amendment_before)==old_pcm_sha
         and amendment_hashes.get("proposed")==sha(amendment_proposed)==new_pcm_sha
         and load(required(amendment_source/"root-format.json"))=={"path":pcm_path,"sha256":new_pcm_sha,"rustfmt_exit":0}
         and sha(amendment_formatted)==new_pcm_sha,
         "PCM reader amendment snapshots/root-format receipt do not match frozen identities")
    preservation_proof=load(required(amendment_source/"source-preservation-proof.json"))
    need(preservation_proof.get("legacy_wrapper_exact_stage")=="authored_bus_pcm_before_mastering"
         and preservation_proof.get("one_leaf_wrapper_exact_stage")=="edge_faded_pcm_before_voice_effects"
         and preservation_proof.get("original_reader_body_byte_equal_after_only_expected_stage_parameter_and_three_plan_assertions_removed") is True
         and preservation_proof.get("same_actual_opened_session_proved_and_read") is True
         and preservation_proof.get("one_leaf_call_sites_changed")==6
         and preservation_proof.get("source_defined_one_leaf_reader_invocations")==9
         and preservation_proof.get("cold_exact_byte_identical") is True
         and preservation_proof.get("all_other_source_bytes_reconstruct_original") is True
         and preservation_proof.get("capability_some_assertion_preserved") is True,
         "PCM reader source-preservation proof is incomplete")
    relocation=load(required(S/"test-module-relocation.json"))
    relocation_review=load(required(S/"review/test-module-relocation-review.json"))
    relocation_before=required(S/"history/test-module-layout-clippy-failure/retimed.before.rs.txt").read_bytes()
    relocation_after=required(S/"test-module-relocated.rs.txt").read_bytes()
    current_retimed=(REPO/relocation_path).read_bytes()
    marker=b"#[cfg(test)]\nmod one_leaf_replay_tests {"
    seam=b"/// Document content the retimed path cannot yet transform."
    start=relocation_before.find(marker)
    end=relocation_before.find(seam,start)
    need(start>=0 and end>start,"retimed relocation anchors are absent from the retained original")
    module=relocation_before[start:end].rstrip()+b"\n"
    expected_retimed=relocation_before[:start]+relocation_before[end:].rstrip()+b"\n\n"+module
    expected_relocation_sha="0f4a3cd202ef5f3005c928b4085a5ed7b7f0e7a45ca38e3c0eba645d8591b9e8"
    expected_module_sha="559cee457415457980a4b4334a1f9f8f091408e839f3b730529584dec834c796"
    need(relocation.get("path")==relocation_path
         and relocation.get("before_sha256")==sha_bytes(relocation_before)
         and sha_bytes(relocation_before)==reviewed_map[relocation_path]
         and relocation.get("after_sha256")==sha_bytes(relocation_after)==expected_relocation_sha
         and final[relocation_path]==expected_relocation_sha
         and relocation.get("module_sha256")==sha_bytes(module)==expected_module_sha
         and relocation.get("module_bytes")==len(module)==7421
         and relocation.get("module_unchanged") is True
         and relocation.get("production_text_and_doc_comments_unchanged_ignoring_module_and_seam_whitespace") is True
         and relocation.get("assertions_unchanged") is True
         and relocation_review.get("status")=="no_findings"
         and relocation_review.get("findings")==[]
         and relocation_review.get("before_sha256")==sha_bytes(relocation_before)
         and relocation_review.get("after_sha256")==expected_relocation_sha
         and relocation_review.get("module_bytes")==7421
         and relocation_review.get("module_sha256")==expected_module_sha
         and relocation_review.get("module_byte_identical") is True
         and relocation_review.get("assertions_weakened") is False
         and expected_retimed==relocation_after==current_retimed,
         "retimed.rs differs from the exact independently reviewed test-module relocation")
    need(review_basis.get("test_module_layout_relocation_paths_within_prototype")==1
         and review_basis.get("test_module_layout_relocation_sha256")==expected_relocation_sha,
         "source-review basis does not pin the single allowed retimed.rs relocation")
    current=set(subprocess.check_output(["rg","--files","-g","*.rs"],cwd=REPO,text=True).splitlines())
    need(current==set(final),"current Rust path set differs from frozen candidate map")
    for path,digest in final.items():
        need(sha(REPO/path)==digest,f"current Rust source guard failed: {path}")
    correction=load(required(S/"test-call-correction.json"))
    correction_path="crates/deadpan-store/src/compound/tests.rs"
    failure_dir=S/"history/initial-store-test-compile-failure"
    old_source=required(failure_dir/"store-compound-tests.before.rs.txt").read_bytes()
    new_source=(REPO/correction_path).read_bytes()
    need(correction.get("path")==correction_path
         and correction.get("before_sha256")==sha_bytes(old_source)
         and correction.get("after_sha256")==final[correction_path]
         and correction.get("production_sources_unchanged") is True
         and correction.get("assertions_unchanged") is True
         and correction.get("initial_failure_retained") is True,
         "Store compile-fix receipt does not match the retained before/final source")
    before_lines=old_source.decode().splitlines()
    after_lines=new_source.decode().splitlines()
    changed=[(a,b) for a,b in zip(before_lines,after_lines) if a!=b]
    need(len(before_lines)==len(after_lines) and len(changed)==1
         and changed[0]==("    let (counted, staged) = execute(&store.connection, &initial, &command, None).unwrap();",
                          "    let (counted, staged) = execute(&store.connection, &initial, &command, None, None).unwrap();"),
         "Store unit-test correction is not exactly the reviewed one-line optional argument fix")
    correction_review=load(required(S/"review/test-call-correction-review.json"))
    need(correction_review.get("status")=="no_findings"
         and correction_review.get("findings")==[]
         and correction_review.get("file")==correction_path
         and correction_review.get("before_sha256")==correction["before_sha256"]
         and correction_review.get("after_sha256")==correction["after_sha256"]
         and correction_review.get("exact_one_replacement_verified") is True
         and correction_review.get("assertions_weakened") is False
         and correction_review.get("production_changed_by_correction") is False,
         "independent correction review does not verify the exact compile fix")
    for path,digest in inputs.items():
        need(sha(REPO/path)==digest,f"qualified input identity changed: {path}")
    old_map_phases={"candidate-broad","candidate-ui","ui-lint"}
    final_map_phases={"candidate-strict","debug-media","release-media","release-native"}
    for phase in sorted(old_map_phases|final_map_phases):
        source_map=load(required(S/f"{phase}-source-identities.json"))
        expected_map=original if phase in old_map_phases else final
        input_file=(S/f"{phase}-lock.json" if phase in {"ui-lint","debug-media","release-media","release-native"}
                    else S/f"{phase}-inputs.json")
        need(source_map==expected_map,f"{phase}: frozen source map differs from its actual executed source")
        need(load(required(input_file))==inputs,f"{phase}: input map differs")
    return final,inputs

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
        "release-media":["build","neutral","aligned","one-leaf"],
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
        labels=("neutral",) if phase=="debug-media" else ("neutral","aligned","one-leaf")
        for name in labels:
            report=load(required(S/f"{phase}-{name}-fixture.json"))
            if name != "one-leaf":
                need(report.get("passed") is True and report.get("failures")==[],f"{phase}-{name} media check failed")
            offsets=report.get("offsets")
            need(report.get("independent_DSP_claim") is False and report.get("listening_claim") is False,
                 f"{phase}-{name} media claim flags changed")
            if name=="one-leaf":
                need(phase=="release-media" and report.get("schema")=="deadpan-one-leaf-media-parity-1"
                     and report.get("capability_present") is True
                     and report.get("callback_counts")==[1,1]
                     and report.get("pictures")==45 and report.get("samples")==72072
                     and report.get("channels")==2,
                     "one-leaf cached/uncached fixture identity/counts differ")
                comparisons=report.get("canonical_comparison",{})
                need(all(comparisons.get(key) is True for key in (
                    "document_bytes_equal","commit_undo_redo_bytes_equal","register_banks_equal",
                    "authored_limited_and_limiter_gain_bits_equal","partition_and_cold_reads_equal",
                    "source_pcm_nonzero","pause_interior_silent")),
                    "one-leaf canonical or history equality checks are incomplete")
                need(report.get("history",{}).get("undo_redo_reopen_checked") is True
                     and report["history"].get("full_replay_checked") is True
                     and report["history"].get("receipt_shortcut_used") is False
                     and report.get("compressed_byte_identity_claim") is False,
                     "one-leaf history or claim flags differ")
                legs=report.get("exports",[])
                need([leg.get("leg") for leg in legs]==["cached","uncached"],
                     "one-leaf report must retain cached and uncached legs")
                leg_checks=[]
                for leg in legs:
                    verification=leg.get("verification",{})
                    need(verification.get("passed") is True and verification.get("failures")==[]
                         and verification.get("summary",{}).get("pictures_checked")==45
                         and verification["summary"].get("pictures_failed")==0
                         and verification["summary"].get("nonzero_offsets")==0,
                         f"one-leaf {leg.get('leg')} video verification failed")
                    audio=verification.get("audio",[])
                    need(len(audio)==1 and audio[0].get("project_samples")==[0,72072]
                         and audio[0].get("flags")==[] and audio[0].get("uncovered_samples")==0
                         and audio[0].get("offset_status")=="verified_zero"
                         and audio[0].get("measured_offset_samples")==0
                         and audio[0].get("blocks",{}).get("low_snr_blocks")==0,
                         f"one-leaf {leg.get('leg')} audio verification failed")
                    leg_root=S/"retained-media/media/release-media/one-leaf"/leg["leg"]
                    movie=Path(leg["movie"])
                    retained_movie=leg_root/movie.name
                    need(movie.is_file() and retained_movie.is_file()
                         and sha(movie)==sha(retained_movie)
                         and movie.stat().st_size==retained_movie.stat().st_size,
                         f"one-leaf {leg.get('leg')} emitted movie copy differs")
                    archive=required(leg_root/"portable-package.tar.gz")
                    sums=load(required(leg_root/"portable-package-checksums.json"))
                    need(sums.get("archive_sha256")==sha(archive),
                         f"one-leaf {leg.get('leg')} portable archive digest mismatch")
                    expected={row["path"]:row for row in sums.get("regular_files",[])}
                    need(expected and len(expected)==len(sums.get("regular_files",[])),
                         f"one-leaf {leg.get('leg')} portable file manifest is empty or duplicated")
                    with tarfile.open(archive,mode="r:gz") as packed:
                        members=packed.getmembers()
                        need({member.name for member in members}==set(expected),
                             f"one-leaf {leg.get('leg')} portable archive members differ")
                        for member in members:
                            path=PurePosixPath(member.name)
                            need(member.isfile() and not path.is_absolute() and ".." not in path.parts,
                                 f"one-leaf {leg.get('leg')} portable archive contains an unsafe entry")
                            data=packed.extractfile(member).read()
                            row=expected[member.name]
                            need(len(data)==row["size"] and sha_bytes(data)==row["sha256"],
                                 f"one-leaf {leg.get('leg')} portable file digest mismatch: {member.name}")
                    leg_checks.append({"leg":leg["leg"],"pictures":45,"samples":72072,
                                      "movie":str(retained_movie.relative_to(S)),
                                      "movie_sha256":sha(retained_movie),
                                      "portable_archive":str(archive.relative_to(S)),
                                      "portable_archive_sha256":sha(archive),
                                      "portable_files":len(expected)})
                need(report.get("independent_DSP_claim") is False
                     and report.get("listening_claim") is False,
                     "one-leaf report overclaims DSP/listening verification")
                media_results.append({"phase":phase,"fixture":name,"pictures":45,"samples":72072,
                                      "exports":leg_checks,"history":report["history"],
                                      "canonical_comparison":comparisons})
                continue
            elif name=="neutral":
                need(isinstance(offsets,list) and offsets and all(len(row)==2 and row[1]==0 for row in offsets),
                     f"{phase}-{name} media offsets are not zero")
                need(report.get("frames")==15 and report.get("pictures")==15 and
                     report.get("canonical_samples_compared")==24024,
                     f"{phase} neutral media counts differ")
                need(report.get("authored_limited_and_limiter_gain_bits_equal") is True,
                     f"{phase} neutral PCM equality flag changed")
            else:
                need(isinstance(offsets,list) and offsets and all(len(row)==2 and row[1]==0 for row in offsets),
                     f"{phase}-{name} media offsets are not zero")
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
    listing=load(required(S/"candidate-all-core-list.json"))["rust-suites"]
    suites=[row for row in listing.values()
            if row.get("binary-name")=="semantic_leaf_replay" or "semantic_leaf_replay" in row.get("binary-path","")]
    need(len(suites)==1,"new Core replay test suite absent or ambiguous")
    selected=[name for name,case in suites[0].get("testcases",{}).items()
              if case.get("filter-match",{}).get("status")=="matches"]
    log=required(S/"candidate-all-core-run.log").read_text()
    need(len(selected)==12 and all(any("PASS" in line and name in line for line in log.splitlines()) for name in selected),
         "all twelve added Core replay tests must be selected and pass")
    media_list=load(required(S/"release-media-list.json"))["rust-suites"]
    media_matches=[name for suite in media_list.values() for name,case in suite.get("testcases",{}).items()
                   if name==f"group_endpoints_export::{NEW_TEST}" and case.get("filter-match",{}).get("status")=="matches"]
    media_log=required(S/"release-media-one-leaf.log").read_text()
    need(len(media_matches)==1 and any("PASS" in line and f"group_endpoints_export::{NEW_TEST}" in line for line in media_log.splitlines()),
         "positive cached/uncached real-media integration test did not run and pass in the target-local export suite")

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
    expected_order_names=["portable"]+["native-"+name for name in
        ["macros","named-registers","dot-repeat","range-set-selectors","transcript-search-selectors"]]
    for pair in range(1,6):
        for kind in (["control","candidate"] if pair%2 else ["candidate","control"]):
            key=f"pair-{pair}-{kind}"
            expected_order_names.extend([key+"-make",key])
    need(row_names==expected_post and len(expected_post)==26
         and [r.get("name") for r in rows]==expected_order_names,
         "post receipt does not contain exactly five native, one portable, and ten make/run pairs")
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
        timings=[t for t in c[0].get("timings",[]) if t.get("name") in TIMING_METRICS]
        need(len(timings)==2 and {t["name"] for t in timings}==TIMING_METRICS,
             f"exact two first-pause metrics missing or duplicated: {key}")
        observed={t["name"]:t["summary"]["completed"]["p50_ms"] for t in timings}
        need(all(isinstance(v,(int,float)) and not isinstance(v,bool) and math.isfinite(v) and v>=0
                 for v in observed.values()) and r.get("metrics")==observed,
             f"timing row differs from finite nonnegative raw report metrics: {key}")
    need(set(trial_rows)==expected_pairs,"timing trials do not cover exactly five control/candidate pairs")
    # LEAK annotations are retained as warnings, not failed assertions. Root
    # must preserve the isolated reruns and verify every listed case is clean.
    leak_cases=[]
    for phase in ('candidate-all-core','candidate-broad','candidate-ui'):
        for line in required(S/f'{phase}-run.log').read_text().splitlines():
            if not re.search(r'\bLEAK\b',line): continue
            match=re.match(r'^\s*LEAK\s+\[[^\]]+\]\s+\(\d+/\d+\)\s+(\S+)\s+(\S+)\s*$',line)
            need(match is not None,f"unrecognized LEAK case line: {phase}: {line}")
            leak_cases.append((phase,*match.groups()))
    need(len(leak_cases)==len(set(leak_cases)),"duplicate LEAK case in original reports")
    leak_rows=[]
    if leak_cases:
        leak=load(required(S/"isolated-leak-reruns.json"))
        leak_rows=leak if isinstance(leak,list) else leak.get("reruns",[])
    elif (S/"isolated-leak-reruns.json").exists():
        leak=load(S/"isolated-leak-reruns.json")
        leak_rows=leak if isinstance(leak,list) else leak.get("reruns",[])
    rerun_cases=[(row.get("phase"),row.get("binary"),row.get("test")) for row in leak_rows]
    need(len(rerun_cases)==len(set(rerun_cases)) and set(rerun_cases)==set(leak_cases),
         "isolated LEAK reruns must cover every exact phase/binary/test once, without unrelated cases")
    for row in leak_rows:
        need(row.get("exit")==0 and row.get("passed") is True and row.get("leaky") is False,
             f"isolated LEAK rerun failed or remains leaky: {row.get('test')}")
        need(row.get("source_binary_guards") is True,
             f"isolated LEAK rerun lacked source/binary guards: {row.get('test')}")
        log_path=S/row["run_log"]
        need(log_path.parent==S and log_path.name.startswith("isolated-leak-") and log_path.suffix==".log"
             and sha(required(log_path))==row.get("log_sha256"),
             "isolated LEAK log path/hash mismatch")
        rerun_log=log_path.read_text()
        passed_cases=[match.groups() for line in rerun_log.splitlines()
                      if (match:=re.match(r'^\s*PASS\s+\[[^\]]+\]\s+\(\d+/\d+\)\s+(\S+)\s+(\S+)\s*$',line))]
        need(not re.search(r'\bLEAK\b',rerun_log)
             and re.search(r'1 tests? run: 1 passed, \d+ skipped',rerun_log) is not None
             and passed_cases==[(row["binary"],row["test"])],
             "isolated rerun log must show the named case passing once without leaks")
        phase_binaries=load(required(S/f"{row['phase']}-binaries.json"))
        binary=phase_binaries.get(row.get("binary_id"),{})
        need(row.get("binary_id")==row["binary"] and row["test"] in binary.get("tests",[])
             and binary.get("sha256")==row.get("binary_sha256"),
             "isolated rerun must use the same selected test executable")
    native=verify_manifest_binaries(S/"release-native-binaries.json","candidate release")
    control=load(required(S/"qualified-control-native-binaries.json"))
    candidate_row=native["deadpan-app"];cand_app=candidate_row["sha256"]; control_app=control["deadpan-app"]["sha256"]
    verify_exec(candidate_row["source"],cand_app,"candidate release app source")
    verify_exec(candidate_row["executed"],cand_app,"candidate release app copy")
    need(control_app=="a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f",
         "qualified control app identity differs")
    metric_names=set(trials[0]["metrics"])
    need(metric_names==TIMING_METRICS,"timing metric keys must be exactly commit and composition")
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
      "candidate-all-core":test_phase("candidate-all-core"),
      "candidate-broad":test_phase("candidate-broad"),
      "candidate-ui":test_phase("candidate-ui"),
    }
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
    review=load(required(S/"review/qualified-media-independent-review.json"))
    basis=load(required(S/"source-review-basis.json"))
    positive_path="crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs"
    export_path="crates/deadpan-cli/tests/preview_export/group_endpoints_export.rs"
    need(review.get("status")=="no_findings" and review.get("findings")==[]
         and review.get("test")==f"recipes::group_endpoints::aligned::{NEW_TEST}"
         and review.get("final_formatted_sha256")==basis.get("formatted_positive_test_sha256")
         and basis.get("formatted_positive_test_sha256")=="c90e802b2181177f6e67a455e739555e0ea3f569dbf34a683b85fe002bbf0f88",
         "historical positive-test review no longer matches its retained pre-refactor source")
    historical_formatted=S/"test-worker-notes/formatted.rs.txt"
    need(historical_formatted.is_file() and sha(historical_formatted)==review["final_formatted_sha256"],
         "historical formatted positive test source differs from its review")
    refactored_hashes=basis.get("refactored_positive_test_sha256",{})
    final_positive_hashes=basis.get("final_positive_test_sha256",{})
    need(basis.get("prototype_production_overlay_paths")==14
         and basis.get("additional_positive_test_paths")==2
         and basis.get("test_call_signature_correction_paths")==1
         and set(refactored_hashes)=={positive_path,export_path}
         and refactored_hashes[positive_path]=="58ce580c3def2cf42c809b7a059e21058cae591eea31284641148e1bca83bd1b"
         and refactored_hashes[export_path]==source_manifest[export_path]
         and final_positive_hashes=={positive_path:source_manifest[positive_path],export_path:source_manifest[export_path]}
         and basis.get("pcm_reader_test_only_amendment")==load(S/"pcm-reader-source-amendment.json")
         and basis.get("schema")=={"core":58,"sqlite":87},
         "source-review basis does not preserve historical tests and final single-file PCM reader amendment")
    refactor_review=load(required(S/"review/positive-test-refactor-review.json"))
    need(refactor_review.get("status")=="no_findings" and refactor_review.get("findings")==[]
         and refactor_review.get("final_formatted_sha256")==refactored_hashes
         and refactor_review.get("assertions_weakened") is False
         and refactor_review.get("test_path",{}).get("new_export_only_path")==f"group_endpoints_export::{NEW_TEST}",
         "refactor review does not identify both final formatted test files and hashes")
    format_hashes=load(required(S/"test-refactor-notes/root-format.json"))
    need(format_hashes=={positive_path:refactored_hashes[positive_path],export_path:source_manifest[export_path]},
         "historical two-file test refactor formatting hashes differ from its source review")
    refactor_sources=load(required(S/"test-refactor-notes/source-hashes.json"))
    for path,stems in ((positive_path,"aligned_group_recipe"),(export_path,"group_endpoints_export")):
        for variant,ext in (("before","before"),("proposed","proposed")):
            snapshot=S/f"test-refactor-notes/{stems}.{ext}.rs.txt"
            need(snapshot.is_file() and sha(snapshot)==refactor_sources[path][variant],
                 f"{variant} source snapshot hash differs for {path}")
    preservation=load(required(S/"test-refactor-notes/assertion-preservation.json"))
    need(preservation.get("original_assertion_macros")==62
         and preservation.get("all_original_assertion_tokens_and_multiplicities_preserved") is True
         and preservation.get("old_recipe_prefix_byte_identical") is True
         and preservation.get("old_export_file_prefix_byte_identical") is True
         and preservation.get("new_test_full_path")==f"group_endpoints_export::{NEW_TEST}",
         "test-refactor assertion preservation evidence differs")
    review_hashes={row.get("sha256") for row in basis.get("reviews",[])}
    expected_review_hashes={
        "9107e928c9cc9f5ac024ddf9cf6fe28bba6d7d9032a3b3c9b80dca48d91aa52c",
        "cd86d74514c216208755f4a3211bc106370551c65d442b502606ce3abee8984e",
        "18fd1e2b0e9722fb4a89b1b707dbb36d7232e03cf16c34acbe5d233551c6c7c9",
        sha(required(S/"review/test-call-correction-review.json")),
        sha(required(S/"review/positive-test-refactor-review.json")),
        sha(required(S/"review/test-module-relocation-review.json")),
        sha(required(S/"review/pcm-stage-reader-review.json")),
    }
    need(review_hashes==expected_review_hashes,
         "review chain does not identify the 14-file overlay and all positive-test/test-only corrections")
    prior_core=Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/independent-review.json")
    prior_tests=Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/test-warning-independent-review.json")
    need(prior_core.is_file() and sha(prior_core)=="9107e928c9cc9f5ac024ddf9cf6fe28bba6d7d9032a3b3c9b80dca48d91aa52c"
         and prior_tests.is_file() and sha(prior_tests)=="cd86d74514c216208755f4a3211bc106370551c65d442b502606ce3abee8984e",
         "earlier 14-file Core review chain no longer matches source-review basis")
    return {"source_base":BASE,"rust_files":len(source_manifest),"input_files":len(inputs),
            "phases":phases,"native_report_checks":reports,"post":post,
            "positive_test_review":review,"positive_test_refactor_review":refactor_review,
            "store_test_call_correction":load(S/"test-call-correction.json"),
            "store_test_call_correction_review":load(S/"review/test-call-correction-review.json"),
            "test_module_relocation":load(S/"test-module-relocation.json"),
            "pcm_reader_test_only_amendment":load(S/"pcm-reader-source-amendment.json"),
            "pcm_reader_test_only_review":load(S/"review/pcm-stage-reader-review.json"),
            "pcm_reader_source_preservation":load(S/"pcm-stage-fix-notes/source-preservation-proof.json"),
            "test_module_relocation_review":load(S/"review/test-module-relocation-review.json"),
            "source_review_basis":basis,"go":go}

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
    if "retained-media" in parts:return False
    if len(parts)==1 and (n=="next-replay-count-study.md" or n.startswith("next-")):return False
    if any(p=="target" or p.endswith(("-extracted","-overlays","-source")) for p in parts):return False
    if any(p.endswith((".deadpan",".tar",".tar.zst",".tar.gz",".mp4")) for p in parts):return False
    if any(p in {"bin","focused-bin"} or p.endswith("-bin") for p in parts):return False
    if "private-packaging-witness" in parts:return False
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
        # Exact formatted Rust sources from the final release-native freeze.
        manifest=load(S/"final-source-identities.json")
        for path in CHANGED:
            raw_path=S/"release-native-source"/path
            need(raw_path.is_file() and sha(raw_path)==manifest[path],
                 f"final release source snapshot differs from tested identity: {path}")
            raw=raw_path.read_bytes()
            final_path=stage/"sources/final"/(path.replace("/","__")+".rs.txt")
            put_bytes(raw,final_path,"txt")
        positive="crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs"
        before=S/"test-worker-notes/aligned_group_recipe.before.rs.txt"
        need(before.is_file() and sha(before)==load(S/"review/qualified-media-independent-review.json")["before_sha256"],
             "positive-test original source snapshot does not match review")
        put_bytes(before.read_bytes(),stage/"sources/original"/(positive.replace("/","__")+".rs.txt"),"txt")
        for filename,repo_path in (("aligned_group_recipe","crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs"),
                                   ("group_endpoints_export","crates/deadpan-cli/tests/preview_export/group_endpoints_export.rs")):
            for stage_name in ("before","proposed","formatted"):
                snapshot=S/f"test-refactor-notes/{filename}.{stage_name}.rs.txt"
                need(snapshot.is_file(),f"missing {stage_name} refactor snapshot: {filename}")
                put_bytes(snapshot.read_bytes(),stage/f"sources/test-refactor/{stage_name}/{repo_path.replace('/', '__')}.rs.txt","txt")
        pcm_path="crates/deadpan-cli/tests/preview_export/aligned_group_recipe.rs"
        pcm_notes=S/"pcm-stage-fix-notes"
        for stage_name,filename in (("before","aligned_group_recipe.before.rs.txt"),
                                   ("proposed","aligned_group_recipe.proposed.rs.txt"),
                                   ("formatted","formatted.rs.txt")):
            snapshot=required(pcm_notes/filename)
            put_bytes(snapshot.read_bytes(),stage/f"sources/pcm-reader-amendment/{stage_name}/{pcm_path.replace('/', '__')}.rs.txt","txt")
        retimed_path="crates/deadpan-core/src/insert_time/retimed.rs"
        retimed_before=S/"history/test-module-layout-clippy-failure/retimed.before.rs.txt"
        retimed_after=S/"test-module-relocated.rs.txt"
        put_bytes(required(retimed_before).read_bytes(),stage/"sources/test-module-relocation/before/retimed.rs.txt","txt")
        put_bytes(required(retimed_after).read_bytes(),stage/"sources/test-module-relocation/after/retimed.rs.txt","txt")
        store_path="crates/deadpan-store/src/compound/tests.rs"
        store_before=S/"history/initial-store-test-compile-failure/store-compound-tests.before.rs.txt"
        need(store_before.is_file() and sha(store_before)==load(S/"test-call-correction.json")["before_sha256"],
             "original Store unit-test source does not match correction receipt")
        put_bytes(store_before.read_bytes(),stage/"sources/original"/(store_path.replace("/","__")+".rs.txt"),"txt")
        # Keep the earlier private packaging witness in an explicitly separate context.
        provenance_sources={
            "positive-media-review.json": S/"review/qualified-media-independent-review.json",
            "test-call-correction.json": S/"test-call-correction.json",
            "test-call-correction-review.json": S/"review/test-call-correction-review.json",
            "test-module-relocation.json": S/"test-module-relocation.json",
            "test-module-relocation-review.json": S/"review/test-module-relocation-review.json",
            "test-module-relocation-patch-notes.json": S/"test-module-relocation-patch-notes.json",
            "pcm-reader-source-amendment.json": S/"pcm-reader-source-amendment.json",
            "pcm-stage-reader-review.json": S/"review/pcm-stage-reader-review.json",
            "pcm-reader-source-preservation-proof.json": S/"pcm-stage-fix-notes/source-preservation-proof.json",
            "pcm-reader-amendment-handoff.md": S/"pcm-stage-fix-notes/HANDOFF.md",
            "pcm-reader-amendment-patch.patch": S/"pcm-stage-fix-notes/reader-contract-fix.patch",
            "pcm-reader-amendment-source-hashes.json": S/"pcm-stage-fix-notes/source-hashes.json",
            "pcm-reader-amendment-root-format.json": S/"pcm-stage-fix-notes/root-format.json",
            "pcm-stage-failure.json": S/"history/positive-media-pcm-stage-failure/failure.json",
            "positive-test-refactor-review.json": S/"review/positive-test-refactor-review.json",
            "test-refactor-handoff.md": S/"test-refactor-notes/HANDOFF.md",
            "test-refactor-assertion-preservation.json": S/"test-refactor-notes/assertion-preservation.json",
            "test-refactor-source-hashes.json": S/"test-refactor-notes/source-hashes.json",
            "test-refactor-patch.patch": S/"test-refactor-notes/refactor.patch",
            "initial-store-compile-failure.json": S/"history/initial-store-test-compile-failure/failure.json",
            "shared-recipe-export-compile-failure.json": S/"history/shared-recipe-export-test-compile-failure/failure.json",
            "source-review-basis.json": S/"source-review-basis.json",
            "expected-source-delta.json": S/"expected-source-delta.json",
            "worker-handoff.md": S/"test-worker-notes/HANDOFF.md",
            "worker-positive-test.patch": S/"test-worker-notes/positive-media-test.patch",
            "worker-source-hashes.json": S/"test-worker-notes/source-hashes.json",
            "prior-core-review.json": Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/independent-review.json"),
            "prior-test-correction-review.json": Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/test-warning-independent-review.json"),
            "prior-prototype-final-source-map.json": Path("/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23/formatted-source-identities.json"),
            "private-packaging-witness.json": S/"private-packaging-witness/witness.json",
            "private-package.py": S/"private-packaging-witness/package.py",
            "private-package.stdout": S/"private-packaging-witness/execution.stdout",
            "private-package.stderr": S/"private-packaging-witness/execution.stderr",
        }
        for name,source in provenance_sources.items():
            area="provenance/private-packaging-witness" if name.startswith("private-") else "provenance"
            copy_file(required(source),stage/area/name.removeprefix("private-"))
        # Retain the exact emitted media and portable tarballs, including both
        # cached and uncached one-leaf exports. Exclude generated project roots.
        media_audit=[]
        retained_root=S/"retained-media/media"
        for item in sorted(retained_root.rglob("*")):
            if not item.is_file() or any(part.endswith(".deadpan") for part in item.relative_to(retained_root).parts):
                continue
            if item.suffix.lower() not in {".json",".txt",".mp4",".gz"}:
                continue
            destination=stage/"retained-media/media"/item.relative_to(retained_root)
            copy_file(item,destination)
            media_audit.append({"path":str(item.relative_to(S)),"bytes":item.stat().st_size,
                                "sha256":sha(item),"retained":True})
        need(any("one-leaf/cached" in row["path"] and row["path"].endswith(".mp4") for row in media_audit)
             and any("one-leaf/uncached" in row["path"] and row["path"].endswith(".mp4") for row in media_audit)
             and any("one-leaf/cached/portable-package.tar.gz" in row["path"] for row in media_audit)
             and any("one-leaf/uncached/portable-package.tar.gz" in row["path"] for row in media_audit),
             "both one-leaf movies and per-leg portable packages must be retained")
        (stage/"emitted-media-audit.json").write_text(json.dumps(media_audit,indent=2)+"\n")
        # Retain compact verified results; no build executables or source archives.
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
