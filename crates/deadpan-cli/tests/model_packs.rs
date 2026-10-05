//! `models` commands as a user runs them: the approved packs, their licenses,
//! explicit acceptance before anything is staged, and offline import refusal.
//! No test downloads; resumable transport and archive import are covered by
//! `deadpan_models::packs` tests.

use std::path::Path;
use std::process::{Command, Output};

fn models(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .arg("models")
        .args(arguments)
        .arg("--root")
        .arg(root)
        .output()
        .expect("run deadpan-cli models")
}

/// The final report: the whole of stdout on success, stderr on failure.
fn last_json(output: &Output) -> serde_json::Value {
    let bytes = if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    };
    serde_json::from_slice(bytes).expect("valid JSON")
}

#[test]
fn list_shows_every_pack_with_size_licenses_and_free_space() {
    let root = tempfile::tempdir().unwrap();
    let output = models(&root.path().join("Models"), &["list"]);
    assert!(output.status.success(), "{output:?}");
    let report = last_json(&output);
    assert!(report["free_bytes"].as_u64().unwrap() > 0);
    let packs = report["packs"].as_array().unwrap();
    let ids: Vec<&str> = packs
        .iter()
        .map(|pack| pack["pack_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["whisper-base-en", "ltx-2.3-q4-bridge"]);
    let bridge = &packs[1];
    assert_eq!(bridge["bytes"], 36_152_862_913_u64);
    assert_eq!(bridge["remaining_bytes"], 36_152_862_913_u64);
    assert_eq!(bridge["files"], 31);
    assert!(bridge["installed"].is_null());
    let licenses = bridge["licenses"].as_array().unwrap();
    assert_eq!(licenses[0]["title"], "LTX-2 Community License Agreement");
    assert_eq!(licenses[1]["title"], "Gemma Terms of Use");
    assert!(
        licenses
            .iter()
            .all(|license| license["acceptance_required"] == true)
    );
    assert_eq!(
        licenses[0]["bytes"].as_u64().unwrap() + licenses[1]["bytes"].as_u64().unwrap(),
        36_152_862_913
    );
    assert_eq!(packs[0]["licenses"][0]["acceptance_required"], false);
}

#[test]
fn license_prints_summaries_links_and_full_texts() {
    let root = tempfile::tempdir().unwrap();
    let output = models(root.path(), &["license", "ltx-2.3-q4-bridge"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "LTX-2 Community License Agreement\n=================================",
        "US$10,000,000",
        "https://huggingface.co/Lightricks/LTX-2.3/blob/main/LICENSE",
        "License date: January 5, 2026",
        "Gemma Terms of Use\n==================",
        "Section 1: DEFINITIONS",
        "ai.google.dev/gemma/terms",
    ] {
        assert!(text.contains(expected), "{expected}");
    }
}

#[test]
fn install_refuses_without_explicit_acceptance_before_staging_anything() {
    let root = tempfile::tempdir().unwrap();
    let models_root = root.path().join("Models");
    for command in [
        vec!["install", "ltx-2.3-q4-bridge"],
        vec!["import", "ltx-2.3-q4-bridge", "/nonexistent-but-unused"],
    ] {
        let output = models(&models_root, &command);
        assert!(!output.status.success());
        if command[0] == "install" {
            // The size and licenses are announced before the refusal.
            let first: serde_json::Value = serde_json::from_str(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(first["event"], "installing");
            assert_eq!(first["bytes"], 36_152_862_913_u64);
            assert_eq!(last_json(&output)["error"]["code"], "ModelPackLicense");
        }
        assert!(!models_root.exists(), "nothing was staged");
    }
}

#[test]
fn offline_import_names_missing_files_and_installs_nothing() {
    let root = tempfile::tempdir().unwrap();
    let models_root = root.path().join("Models");
    let source = root.path().join("offline");
    std::fs::create_dir_all(source.join("mlx_ltx_q4_pack")).unwrap();
    let output = models(
        &models_root,
        &[
            "import",
            "ltx-2.3-q4-bridge",
            source.to_str().unwrap(),
            "--accept-license",
        ],
    );
    assert!(!output.status.success());
    let error = &last_json(&output)["error"];
    assert_eq!(error["code"], "ModelPackVerification");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("lacks 31 of this pack's files"),
        "{error}"
    );
    let listed = last_json(&models(&models_root, &["list"]));
    assert!(listed["packs"][1]["installed"].is_null());
}

#[test]
fn export_requires_an_installed_pack_and_unknown_packs_are_usage_errors() {
    let root = tempfile::tempdir().unwrap();
    let output = models(
        root.path(),
        &[
            "export",
            "ltx-2.3-q4-bridge",
            root.path().join("pack.tar").to_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert!(!root.path().join("pack.tar").exists());
    let output = models(root.path(), &["install", "no-such-pack"]);
    assert!(!output.status.success());
    assert!(
        last_json(&output)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no approved model pack named no-such-pack")
    );
}
