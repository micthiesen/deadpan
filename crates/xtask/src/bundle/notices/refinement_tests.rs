use super::*;

#[test]
fn compiled_derivative_notices_are_exact_and_cannot_be_omitted_or_relabelled() {
    let workspace = super::super::workspace_root();
    for (name, expected) in REFINEMENT_NOTICE_PINS {
        let mut missing = vendored_notices(&workspace).unwrap();
        missing.retain(|notice| notice.path != name);
        assert!(require_refinement_notices(&missing).is_err(), "{name}");
        let mut changed = vendored_notices(&workspace).unwrap();
        let item = changed
            .iter_mut()
            .find(|notice| notice.path == name)
            .unwrap();
        assert_eq!(sha256_file(&item.file).unwrap(), expected);
        item.file = workspace.join("packaging/notices/ffmpeg/LICENSE.md");
        assert!(require_refinement_notices(&changed).is_err(), "{name}");
    }
    for (name, original) in [
        (
            "AUTHOR-LICENSE",
            "native/deadpan-align/assets/AUTHOR-LICENSE",
        ),
        (
            "UNICODE-LICENSE",
            "crates/deadpan-analysis/src/refinement/UNICODE-LICENSE",
        ),
        (
            "PYTHON-LICENSE",
            "crates/deadpan-analysis/src/refinement/PYTHON-LICENSE",
        ),
    ] {
        assert_eq!(
            fs::read(workspace.join(format!("packaging/notices/refinement/{name}"))).unwrap(),
            fs::read(workspace.join(original)).unwrap()
        );
    }
}

#[test]
fn actual_notice_generation_copies_and_references_all_compiled_derivative_terms() {
    let workspace = super::super::workspace_root();
    let root =
        std::env::temp_dir().join(format!("xtask-refinement-notices-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let ffmpeg = Ffmpeg {
        version: "test".into(),
        tag: "test".into(),
        commit: "0".repeat(40),
        archive_url: "https://example.invalid/source".into(),
        archive_sha256: "0".repeat(64),
        configuration: "test-only".into(),
        libraries: Vec::new(),
    };
    let vendored = vendored_notices(&workspace).unwrap();
    write_notices(&root, &workspace, &[], &vendored, &ffmpeg, &[]).unwrap();
    let text = fs::read_to_string(root.join("THIRD_PARTY_NOTICES.txt")).unwrap();
    for (name, expected) in REFINEMENT_NOTICE_PINS {
        assert!(text.contains(name), "{name}");
        assert_eq!(sha256_file(&root.join(name)).unwrap(), expected);
    }
    assert!(text.contains("separate from model weights"));
    assert!(text.contains("no CPython code, Python interpreter"));
    let bom = sbom("test", "2026-10-25T00:00:00Z", &[], &ffmpeg, &[], &[]);
    for component in refinement_components() {
        assert!(
            bom["components"].as_array().unwrap().contains(&component),
            "{}",
            component["bom-ref"]
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn derivative_sbom_uses_independent_terms_and_claims_no_python_or_model_runtime() {
    let components = refinement_components();
    assert_eq!(components.len(), 2);
    assert_eq!(
        components[0]["licenses"][0]["license"]["id"],
        "BSD-2-Clause"
    );
    assert_eq!(
        components[1]["licenses"][0]["expression"],
        "Unicode-3.0 AND Python-2.0"
    );
    let serialized = serde_json::to_string(&components).unwrap();
    for (name, expected) in REFINEMENT_NOTICE_PINS {
        assert!(serialized.contains(name));
        assert!(serialized.contains(expected));
    }
    assert!(serialized.contains("no CPython code or interpreter"));
    assert!(serialized.contains("no separately explicit model-weight grant"));
}
