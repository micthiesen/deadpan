//! Signed update commands as a user runs them: `update-signing`, `downloader
//! update|rollback|status` and `models update|rollback`. No test downloads;
//! verified installs, probes and rollbacks with trusted test keys are covered
//! by the `youtube::updates` and `packs::updates` unit tests.

use std::path::Path;
use std::process::{Command, Output};

use deadpan_models::updates::{SignedManifest, UpdateKind, generate_key};

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .env_remove("DEADPAN_UPDATE_SIGNING_KEY")
        .output()
        .expect("run deadpan-cli")
}

fn error_code(output: &Output) -> String {
    assert!(!output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    report["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// An envelope signed by a fresh key this build does not trust.
fn untrusted(directory: &Path, kind: UpdateKind, payload: &str) -> std::path::PathBuf {
    let (pkcs8, _) = generate_key().unwrap();
    let signed = SignedManifest::sign(kind, payload.into(), "stranger", &pkcs8).unwrap();
    let file = directory.join(format!("{}.json", kind.as_str()));
    std::fs::write(&file, signed.to_bytes()).unwrap();
    file
}

#[test]
fn untrusted_updates_are_refused_before_anything_is_written() {
    let directory = tempfile::tempdir().unwrap();
    let helpers = directory.path().join("helpers");
    let models = directory.path().join("models");
    let downloader = untrusted(directory.path(), UpdateKind::Downloader, "{}");
    let output = cli(&[
        "downloader",
        "update",
        "--manifest",
        path(&downloader),
        "--root",
        path(&helpers),
    ]);
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(error_code(&output), "UpdateUntrusted");
    }
    assert!(!helpers.join("updates/state.json").exists());

    let pack = untrusted(directory.path(), UpdateKind::ModelPack, "{}");
    let output = cli(&["models", "update", path(&pack), "--root", path(&models)]);
    assert_eq!(error_code(&output), "UpdateUntrusted");
    assert!(!models.join(".updates").exists());
    assert!(!models.join(".active").exists());

    // A downloader envelope offered as a model pack is a different kind.
    let output = cli(&[
        "update-signing",
        "verify",
        "--kind",
        "model-pack",
        path(&downloader),
    ]);
    assert_eq!(error_code(&output), "UpdateSignatureInvalid");
    let output = cli(&["models", "update", "https://", "--root", path(&models)]);
    assert!(!output.status.success());
    let output = cli(&[
        "models",
        "update",
        "file:///etc/passwd",
        "--root",
        path(&models),
    ]);
    assert_eq!(error_code(&output), "UpdateManifestInvalid");
}

#[test]
fn rollback_without_a_previous_version_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let output = cli(&[
        "models",
        "rollback",
        "whisper-base-en",
        "--root",
        path(directory.path()),
    ]);
    assert_eq!(error_code(&output), "ModelPackNoPrevious");
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        let output = cli(&["downloader", "rollback", "--root", path(directory.path())]);
        assert_eq!(error_code(&output), "DownloaderNoPrevious");
    }
    // Status names the update state and that the compiled pins are selected.
    let output = cli(&["downloader", "status", "--root", path(directory.path())]);
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source"], "managed");
    assert_eq!(report["updates"]["active"], "baseline");
    assert!(report["updates"]["state"].is_null());
}

#[test]
fn signing_refuses_untrusted_keys_and_describes_helpers() {
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("keys/new.pk8");
    let output = cli(&["update-signing", "keygen", "--out", path(&key)]);
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ed25519"].as_str().unwrap().len(), 64);
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // Never overwrites an existing key.
    let output = cli(&["update-signing", "keygen", "--out", path(&key)]);
    assert!(!output.status.success());

    let payload = directory.path().join("pack.json");
    std::fs::write(
        &payload,
        serde_json::json!({
            "schema": 1, "serial": 1, "issued": "2026-10-06", "min_app_version": "0.1.0",
            "pack": deadpan_models::packs::approved_pack("whisper-base-en").map(|mut pack| {
                pack.pack_version = "3".into();
                pack
            }),
        })
        .to_string(),
    )
    .unwrap();
    let signed = directory.path().join("signed.json");
    let output = cli(&[
        "update-signing",
        "sign",
        "--kind",
        "model-pack",
        "--key",
        path(&key),
        path(&payload),
        path(&signed),
    ]);
    assert_eq!(error_code(&output), "UpdateUntrusted");
    assert!(!signed.exists());

    // A payload the app cannot parse is refused before signing.
    std::fs::write(&payload, "{\"schema\": 1}").unwrap();
    let output = cli(&[
        "update-signing",
        "sign",
        "--kind",
        "model-pack",
        "--key",
        path(&key),
        path(&payload),
        path(&signed),
    ]);
    assert_eq!(error_code(&output), "UpdateManifestInvalid");

    let output = cli(&["update-signing", "describe", "/usr/bin/true"]);
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["content_sha256"].as_str().unwrap().len(), 64);
}
