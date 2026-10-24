from pathlib import Path
import datetime
import difflib
import hashlib
import json
import os
import shutil

ROOT = Path("/tmp/deadpan-compound-wire-reuse-production-2026-10-24")
CONTROL = Path("/tmp/deadpan-single-leaf-net-production-2026-10-24")
PROTOTYPE = Path("/tmp/deadpan-compound-wire-reuse-prototype-2026-10-24")
PROFILE_BASE = Path("/tmp/deadpan-single-leaf-net-prototype-v2-2026-10-24")
SOURCE = ROOT / "source"
OWNED = (
    "crates/deadpan-core/src/compound.rs",
    "crates/deadpan-core/src/one_leaf_replay.rs",
)
PINS = {
    "control_map": "f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53",
    "prototype_map": "59ab40b15f9a705953320d45a9ffcef87a85f876a15455c4fb4180ce12ebdff9",
    "profile_base_map": "0bf07513646ffdfee5aec03bb1b64b6fff757196512aeea883226c675b15ed02",
    "inputs": "b783a007ca4b3ab356c3488372a276c0ffa7ad4f9f34b31f1b0743b121e44317",
    "control_app": "970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2",
}
# Only artifact/cache directory names are excluded. In particular, legitimate
# source directories named "targets" are retained and the exact Rust path set
# is checked against the qualified control.
EXCLUDED_DIRS = {
    ".git", ".claude", "target", ".cache", "__pycache__", ".pytest_cache",
    ".mypy_cache", ".ruff_cache", ".venv", "node_modules",
}
EXCLUDED_FILES = {".DS_Store"}

REMOVALS = {
    OWNED[0]: [
        ("    let _pause_replay =\n        crate::pause_route_profile::enter(crate::pause_route_profile::Phase::ReplayMisc);\n", 1),
        ("    let pause_wire =\n        crate::pause_route_profile::enter(crate::pause_route_profile::Phase::ReplayDocumentWire);\n", 2),
        ("            let pause_wire = crate::pause_route_profile::enter(\n                crate::pause_route_profile::Phase::ReplayDocumentWire,\n            );\n", 2),
        ("            drop(pause_wire);\n", 2),
        ("    drop(pause_wire);\n", 2),
        ("    let pause_net =\n        crate::pause_route_profile::enter(crate::pause_route_profile::Phase::ReplayNetAggregate);\n", 1),
        ("    drop(pause_net);\n", 1),
    ],
    OWNED[1]: [
        ("    let pause_apply =\n        crate::pause_route_profile::enter(crate::pause_route_profile::Phase::SemanticLeafApply);\n", 1),
        ("    drop(pause_apply);\n", 1),
        ("        let _pause_snapshot = crate::pause_route_profile::enter(\n            crate::pause_route_profile::Phase::CapabilitySnapshot,\n        );\n", 1),
        ("        let pause_match = crate::pause_route_profile::enter(\n            crate::pause_route_profile::Phase::ReuseEligibilityEq,\n        );\n", 1),
        ("        drop(pause_match);\n", 1),
        ("        let _pause_exit =\n            crate::pause_route_profile::enter(crate::pause_route_profile::Phase::ReuseCloneExit);\n", 1),
    ],
}
CLOSURE_BEFORE = """    .then(|| {
        OneLeafReplay {
            entry: entry.clone(),
            request: request.clone(),
            next: next.clone(),
            edit: edit.expect("retained InsertTime edit"),
        }
    });"""
CLOSURE_AFTER = """    .then(|| OneLeafReplay {
        entry: entry.clone(),
        request: request.clone(),
        next: next.clone(),
        edit: edit.expect("retained InsertTime edit"),
    });"""


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(name, value):
    with (ROOT / name).open("x") as output:
        output.write(json.dumps(value, indent=2) + "\n")


def source_map(root):
    return {
        str(path.relative_to(root)): sha(path)
        for path in sorted(root.rglob("*.rs"))
    }


def guarded_map(root, pin, count):
    manifest = root / "frozen-sources.json"
    assert sha(manifest) == pin, (root, "manifest pin")
    expected = json.loads(manifest.read_text())
    actual = source_map(root / "source")
    assert actual == expected, (root, "actual source bytes")
    assert len(actual) == count, (root, "source count")
    return actual


def remove_profile(relative, text):
    receipts = []
    for old, count in REMOVALS[relative]:
        assert text.count(old) == count, (relative, "profile removal count", old)
        receipts.append({
            "old_text": old, "new_text": "", "occurrences": count,
            "old_text_sha256": hashlib.sha256(old.encode()).hexdigest(),
        })
        text = text.replace(old, "")
    if relative == OWNED[1]:
        assert text.count(CLOSURE_BEFORE) == 1
        receipts.append({
            "old_text": CLOSURE_BEFORE,
            "new_text": CLOSURE_AFTER,
            "occurrences": 1,
            "reason": "Exact closure cleanup already present in qualified uninstrumented control",
        })
        text = text.replace(CLOSURE_BEFORE, CLOSURE_AFTER)
    assert "pause_route_profile" not in text, relative
    return text, receipts


def file_diff(before, after, relative):
    return "".join(difflib.unified_diff(
        before.splitlines(keepends=True), after.splitlines(keepends=True),
        fromfile="a/" + relative, tofile="b/" + relative,
    ))


def main():
    assert not SOURCE.exists(), "Preparation never overwrites an existing source"
    assert not (ROOT / "preparation-receipt.json").exists()
    control = guarded_map(CONTROL, PINS["control_map"], 2053)
    prototype = guarded_map(PROTOTYPE, PINS["prototype_map"], 2054)
    profile_base = guarded_map(PROFILE_BASE, PINS["profile_base_map"], 2054)
    for root in (CONTROL, PROTOTYPE, PROFILE_BASE):
        assert sha(root / "baseline-inputs.json") == PINS["inputs"]
    inputs = json.loads((CONTROL / "baseline-inputs.json").read_text())
    assert len(inputs) == 6
    for root in (CONTROL, PROTOTYPE, PROFILE_BASE):
        assert all(sha(root / "source" / path) == digest for path, digest in inputs.items())
    control_app = CONTROL / "bin/deadpan-app"
    assert sha(control_app) == PINS["control_app"]
    review = json.loads((PROTOTYPE / "source-review.json").read_text())
    assert review["status"] == "no_findings" and not review["findings"]
    assert review["source_map_sha256"] == PINS["prototype_map"]
    assert set(prototype) == set(profile_base)
    assert {path for path in prototype if prototype[path] != profile_base[path]} == set(OWNED)
    assert set(profile_base) - set(control) == {"crates/deadpan-core/src/pause_route_profile.rs"}
    assert not (set(control) - set(profile_base))

    prepared = {}
    removal_receipts = {}
    for relative in OWNED:
        original = (PROFILE_BASE / "source" / relative).read_text()
        qualified = (CONTROL / "source" / relative).read_text()
        stripped_base, receipts = remove_profile(relative, original)
        assert stripped_base == qualified, (relative, "baseline removal proof")
        raw = (PROTOTYPE / "source" / relative).read_text()
        assert sha(PROTOTYPE / "source" / relative) == review["reviewed_paths"][relative]
        prepared[relative], applied = remove_profile(relative, raw)
        assert applied == receipts
        removal_receipts[relative] = {
            "instrumented_base_sha256": profile_base[relative],
            "qualified_uninstrumented_base_sha256": control[relative],
            "reviewed_wire_prototype_sha256": prototype[relative],
            "operations": receipts,
            "stripped_base_equals_qualified_control": True,
        }
    marker = "#[cfg(test)]"
    assert prepared[OWNED[1]].split(marker, 1)[0] == (
        CONTROL / "source" / OWNED[1]
    ).read_text().split(marker, 1)[0], "OneLeafReplay production must stay exact"

    excluded = []
    def ignore(directory, names):
        ignored = []
        for name in names:
            path = Path(directory) / name
            if (path.is_dir() and name in EXCLUDED_DIRS) or name in EXCLUDED_FILES or path.suffix == ".pyc":
                ignored.append(name)
                excluded.append(str(path.relative_to(CONTROL / "source")))
        return ignored
    shutil.copytree(CONTROL / "source", SOURCE, ignore=ignore, symlinks=True)
    assert source_map(SOURCE) == control, "Copied source differs before the two-file change"
    for relative, text in prepared.items():
        assert sha(SOURCE / relative) == control[relative]
        (SOURCE / relative).write_text(text)
    candidate = source_map(SOURCE)
    assert set(candidate) == set(control) and len(candidate) == 2053
    changed = sorted(path for path in candidate if candidate[path] != control[path])
    assert changed == sorted(OWNED)
    assert all(candidate[path] == control[path] for path in candidate if path not in OWNED)
    assert all(sha(SOURCE / path) == digest for path, digest in inputs.items())
    assert not any("pause_route_profile" in (SOURCE / path).read_text() for path in candidate)
    assert not any(SOURCE.rglob(".git"))
    assert not any(SOURCE.rglob(".claude"))
    assert not any(SOURCE.rglob("target"))
    # Preserve immutable source roots and their metadata, checking again after
    # all reads/copy/application rather than assuming preflight was sufficient.
    assert guarded_map(CONTROL, PINS["control_map"], 2053) == control
    assert guarded_map(PROTOTYPE, PINS["prototype_map"], 2054) == prototype
    assert guarded_map(PROFILE_BASE, PINS["profile_base_map"], 2054) == profile_base

    for output, source in [
        ("baseline-sources.json", CONTROL / "frozen-sources.json"),
        ("baseline-inputs.json", CONTROL / "baseline-inputs.json"),
        ("prototype-sources.json", PROTOTYPE / "frozen-sources.json"),
        ("profile-baseline-sources.json", PROFILE_BASE / "frozen-sources.json"),
    ]:
        with (ROOT / output).open("xb") as handle:
            handle.write(source.read_bytes())
    save("frozen-sources.json", candidate)
    (ROOT / "prototype.patch").write_text("".join(
        file_diff((CONTROL / "source" / relative).read_text(), prepared[relative], relative)
        for relative in OWNED
    ))
    overlay_changed = sorted(
        path for path in set(profile_base) | set(control)
        if profile_base.get(path) != control.get(path)
    )
    (ROOT / "profiling-overlay.patch").write_text("".join(
        file_diff(
            (PROFILE_BASE / "source" / relative).read_text() if relative in profile_base else "",
            (CONTROL / "source" / relative).read_text() if relative in control else "",
            relative,
        ) for relative in overlay_changed
    ))
    save("profiling-removals.json", {
        "base_relationship": {"instrumented": PINS["profile_base_map"], "uninstrumented": PINS["control_map"]},
        "overlay_changed_paths": overlay_changed,
        "owned_exact_operations": removal_receipts,
        "no_other_prototype_source_copied": True,
    })
    prior = json.loads((PROTOTYPE / "baseline-provenance.json").read_text())
    save("baseline-provenance.json", {
        "qualified_main_commit": prior["qualified_main_commit"],
        "qualified_main_source_map_sha256": PINS["control_map"],
        "matched_source": str(CONTROL / "source"),
        "matched_source_map_sha256": PINS["control_map"],
        "matched_app": str(control_app),
        "matched_app_sha256": PINS["control_app"],
        "qualified_control": str(control_app),
        "qualified_control_sha256": PINS["control_app"],
        "hooks": "none; uninstrumented 2053-source wire candidate",
        "private_prototype_source": str(PROTOTYPE / "source"),
        "private_prototype_source_map_sha256": PINS["prototype_map"],
        "instrumented_parent_map_sha256": PINS["profile_base_map"],
        "private_source_review_sha256": sha(PROTOTYPE / "source-review.json"),
        "no_build_format_test_or_product_runtime": True,
    })
    save("baseline.json", {
        "commit": prior["qualified_main_commit"],
        "code_commit": prior["qualified_main_commit"],
        "source_files": 2053, "private_profile_files": 0, "input_files": 6,
        "control": {"path": str(control_app), "sha256": PINS["control_app"]},
        "matched_profile": {"path": str(control_app), "sha256": PINS["control_app"]},
        "parent_source_map_sha256": PINS["control_map"],
        "qualified_helper_manifest_sha256": json.loads((CONTROL / "baseline.json").read_text())["qualified_helper_manifest_sha256"],
    })
    save("preparation-receipt.json", {
        "prepared_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "preparation_script_sha256": sha(ROOT / "prepare.py"),
        "control_map_sha256": PINS["control_map"],
        "candidate_map_sha256": sha(ROOT / "frozen-sources.json"),
        "reviewed_prototype_map_sha256": PINS["prototype_map"],
        "instrumented_baseline_map_sha256": PINS["profile_base_map"],
        "rust_file_count": len(candidate), "unchanged_nonowned_rust_files": 2051,
        "changed_paths": {path: candidate[path] for path in changed},
        "six_inputs_identical": True, "one_leaf_production_prefix_identical": True,
        "source_roots_rehashed_after_copy": True, "excluded_paths": sorted(excluded),
        "exclusion_directory_names": sorted(EXCLUDED_DIRS),
        "format_build_tests_product_runtime_git_main_writes": False,
        "status": "source_prepared_only; Root owns format, independent review and qualification",
    })
    names = [
        "prepare.py", "baseline-sources.json", "baseline-inputs.json",
        "prototype-sources.json", "profile-baseline-sources.json", "frozen-sources.json",
        "prototype.patch", "profiling-overlay.patch", "profiling-removals.json",
        "baseline-provenance.json", "baseline.json", "preparation-receipt.json",
    ]
    save("preparation-pins.json", {name: sha(ROOT / name) for name in names})
    print(json.dumps({
        "status": "source_prepared_only",
        "candidate_map_sha256": sha(ROOT / "frozen-sources.json"),
        "owned": {path: candidate[path] for path in changed},
        "preparation_pins_sha256": sha(ROOT / "preparation-pins.json"),
        "unchanged_nonowned_rust_files": 2051,
        "six_inputs_identical": True,
    }, indent=2))


if __name__ == "__main__":
    main()
