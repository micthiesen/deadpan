use super::*;
use std::collections::BTreeMap;
use std::os::unix::fs::symlink;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectId, RevisionId, Subtree,
};

const PRIVATE: &str = "sensitive-transcript /Users/private-owner/private-video.mov https://example.test/source?token=secret-token Cookie: secret-cookie Authorization: secret-auth";

fn document() -> ProjectDocument {
    let root = NodeId::new("private-root-identity").unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("private-project-identity").unwrap(),
        RevisionId::new("private-initial-identity").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    let hold = NodeId::new("private-hold-identity").unwrap();
    let edit = deadpan_core::apply(
        &initial,
        &CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: RevisionId::new("private-final-identity").unwrap(),
            command: Command::Insert {
                parent: root,
                index: 0,
                subtree: Subtree {
                    root: hold.clone(),
                    nodes: BTreeMap::from([(
                        hold,
                        BeatNode::hold(
                            PRIVATE,
                            HoldRecipe {
                                duration: FrameDuration::new(24).unwrap(),
                                picture_context: None,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )
    .unwrap();
    edit.forward.apply(&initial).unwrap()
}

fn assert_private_text_absent(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    for secret in [
        "private-",
        "sensitive-transcript",
        "secret-token",
        "secret-cookie",
        "secret-auth",
        "example.test",
        "/Users/",
        ".host.json",
        "cookies.txt",
    ] {
        assert!(!text.contains(secret), "report leaked {secret:?}");
    }
}

#[test]
fn captures_structure_and_closed_failure_context_without_authored_content() {
    let document = document();
    assert!(document.to_json().unwrap().contains("secret-cookie"));
    let report = DiagnosticReport::from_document(Some(&document))
        .with_failure(Operation::Render, Failure::CleanupUnconfirmed);
    let bytes = report.bytes().unwrap();
    assert_private_text_absent(&bytes);
    assert!(bytes.len() < MAX_BYTES);
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["schema_version"], SCHEMA_VERSION);
    assert_eq!(json["project"]["status"], "available");
    let structure = &json["project"]["structure"];
    assert_eq!(structure["nodes"]["hold"], 1);
    assert_eq!(structure["nodes"]["sequence"], 1);
    assert_eq!(structure["duration_frames"], 24);
    assert_eq!(structure["raster"], serde_json::json!([640, 360]));
    assert_eq!(structure["frame_rate"], serde_json::json!([24, 1]));
    assert!(structure["plan_error_code"].is_null());
    assert_eq!(json["context"]["failure"], "cleanup_unconfirmed");
    assert_eq!(json["context"]["operation"], "render");
    assert_eq!(json["versions"]["application"], env!("CARGO_PKG_VERSION"));
    assert!(
        json["versions"]["compiled_helpers"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    assert!(json["counters"].is_object());
}

#[test]
fn package_capture_ignores_private_files_and_leaves_authored_history_unchanged() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("private-project.deadpan");
    let document = document();
    let store = ProjectStore::create(&package, &document).unwrap();
    let before = store.snapshot().unwrap().to_json().unwrap();
    drop(store);
    for name in ["cookies.txt", "transcript.txt", ".host.json"] {
        fs::write(package.join(name), PRIVATE).unwrap();
    }
    let report = capture(Some(&package));
    assert_private_text_absent(&report.bytes().unwrap());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap().to_json().unwrap(), before);
    for name in ["cookies.txt", "transcript.txt", ".host.json"] {
        assert_eq!(fs::read_to_string(package.join(name)).unwrap(), PRIVATE);
    }
}

#[test]
fn unavailable_project_exports_a_safe_code_without_its_path_or_error_text() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("private-missing-project.deadpan");
    let report = capture(Some(&package));
    let bytes = report.bytes().unwrap();
    assert_private_text_absent(&bytes);
    assert!(
        !std::str::from_utf8(&bytes)
            .unwrap()
            .contains(&scratch.path().display().to_string())
    );
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["project"]["status"], "unavailable");
    assert!(!json["project"]["error_code"].as_str().unwrap().is_empty());
    assert!(!package.exists());
}

#[test]
fn export_is_owner_only_atomic_and_never_overwrites_files_or_symlinks() {
    let scratch = tempfile::tempdir().unwrap();
    let destination = scratch.path().join("diagnostic.json");
    let report = DiagnosticReport::from_document(Some(&document()));
    let receipt = report.write(&destination).unwrap();
    let bytes = fs::read(&destination).unwrap();
    assert_eq!(bytes.len(), receipt.byte_length);
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_private_text_absent(&bytes);
    assert_eq!(
        report.write(&destination).unwrap_err().code(),
        "DiagnosticAlreadyExists"
    );
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    let link = scratch.path().join("report-link.json");
    symlink(&destination, &link).unwrap();
    assert_eq!(
        report.write(&link).unwrap_err().code(),
        "DiagnosticAlreadyExists"
    );
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    assert_eq!(
        fs::read_dir(scratch.path()).unwrap().count(),
        2,
        "staging files leaked"
    );
}

#[test]
fn bounded_serialization_stops_without_publishing_a_partial_report() {
    let scratch = tempfile::tempdir().unwrap();
    let destination = scratch.path().join("diagnostic.json");
    let mut report = DiagnosticReport::from_document(None);
    // Fault injection: public callers cannot put content into this field.
    report.counters = serde_json::Value::String("x".repeat(MAX_BYTES));
    assert_eq!(
        report.write(&destination).unwrap_err().code(),
        "DiagnosticLimitExceeded"
    );
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
}
