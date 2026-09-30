#![cfg(any(target_os = "macos", target_os = "linux"))]

use deadpan_cli::render::{
    RenderContext, RenderOperation, RenderRequest, STATUS_PAGE_SIZE, start_request,
};
use deadpan_core::{
    BeatNode, ColorPolicy, CommandRequest, FrameDuration, FrameRate, HoldAudio, HoldRecipe,
    HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectFrame, ProjectId, RevisionId,
    Subtree,
};
use deadpan_jobs::{
    RequestId,
    render::{RenderIntent, document_sha256_for_validation},
};
use deadpan_store::ProjectStore;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Output, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"));
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(deadpan_native_process::spawn(&mut command)?.wait_with_output()?)
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn empty() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("public-render")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}
fn document() -> Result<ProjectDocument> {
    let empty = empty()?;
    let edit = deadpan_core::apply(
        &empty,
        &CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: RevisionId::new("baseline")?,
            command: deadpan_core::Command::Insert {
                parent: empty.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "Hold",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(12)?,
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
    )?;
    Ok(edit.forward.apply(&empty)?)
}
fn error(output: &Output) -> Result<Value> {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error["schema_version"], 1);
    Ok(error)
}

#[test]
fn public_render_help_and_strict_automatic_options_are_exposed() -> Result {
    let help = cli(&["--help"])?;
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout)?;
    assert!(text.contains("render retry"));
    assert!(text.contains("render reconcile"));
    let failure = cli(&[
        "render",
        "unused.deadpan",
        "--output",
        "/tmp",
        "--encoder",
        "software",
    ])?;
    assert_eq!(error(&failure)?["error"]["code"], "RenderInvalidRequest");
    Ok(())
}

#[test]
fn render_status_is_read_only_during_another_writer_and_pages_without_losing_jobs() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("status.deadpan");
    let document = document()?;
    let mut store = ProjectStore::create(&package, &document)?;
    let context = RenderContext::from_document(&document);
    for ordinal in 0..=STATUS_PAGE_SIZE {
        let request = start_request(&context, root.path().join("unused.mp4"), Instant::now())?;
        store.create_render_job(
            RenderIntent {
                schema_version: 2,
                job_id: RequestId::new(format!("job-{ordinal:03}"))?,
                project_id: context.project_id.clone(),
                revision_id: context.revision_id.clone(),
                document_sha256: document_sha256_for_validation(&document)?,
                range: deadpan_core::FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
                policy: request.policy,
            },
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(5),
        )?;
    }
    let first = cli(&["render", "status", path(&package)])?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(first.stderr.is_empty());
    let first: Value = serde_json::from_slice(&first.stdout)?;
    assert_eq!(first["event"], "stored_status");
    assert_eq!(first["live_progress"], false);
    assert_eq!(
        first["page"]["items"].as_array().unwrap().len(),
        STATUS_PAGE_SIZE as usize
    );
    let cursor = first["page"]["next_after"].as_str().unwrap();
    let second = cli(&["render", "status", path(&package), "--after", cursor])?;
    assert!(second.status.success());
    let second: Value = serde_json::from_slice(&second.stdout)?;
    assert_eq!(second["page"]["items"].as_array().unwrap().len(), 1);
    assert!(second["page"]["next_after"].is_null());
    let attempts = cli(&["render", "status", path(&package), "--job", "job-000"])?;
    assert!(attempts.status.success());
    let attempts: Value = serde_json::from_slice(&attempts.stdout)?;
    assert!(attempts["page"]["items"].as_array().unwrap().is_empty());
    assert_eq!(store.snapshot()?, document);
    assert!(!root.path().join("unused.mp4").exists());
    Ok(())
}

#[test]
fn invalid_request_schema_is_rejected_before_opening_any_project() -> Result {
    let root = tempfile::tempdir()?;
    let json = root.path().join("request.json");
    let request = RenderRequest {
        schema_version: 2,
        request_id: RequestId::new("request")?,
        context: RenderContext::from_document(&document()?),
        operation: RenderOperation::Start {
            destination: root.path().join("unused.mp4"),
        },
    };
    std::fs::write(&json, serde_json::to_vec(&request)?)?;
    let result = cli(&[
        "render",
        path(&root.path().join("missing.deadpan")),
        "--json",
        path(&json),
    ])?;
    assert_eq!(
        error(&result)?["error"]["code"],
        "RenderProtocolUnsupported"
    );
    assert!(!root.path().join("missing.deadpan").exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn public_render_rejects_an_open_writer_without_creating_an_attempt() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("locked.deadpan");
    let store = ProjectStore::create(&package, &document()?)?;
    let result = cli(&["render", path(&package), "--output", path(root.path())])?;
    assert_eq!(error(&result)?["error"]["code"], "RenderOwnerUnavailable");
    assert!(store.render_jobs(None, 1)?.is_empty());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn empty_and_stale_public_starts_fail_before_worker_admission() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("empty.deadpan");
    drop(ProjectStore::create(&package, &empty()?)?);
    let result = cli(&["render", path(&package), "--output", path(root.path())])?;
    assert_eq!(error(&result)?["error"]["code"], "RenderEmpty");
    let result = cli(&[
        "render",
        path(&package),
        "--output",
        path(root.path()),
        "--expected",
        "stale",
    ])?;
    let error = error(&result)?;
    assert_eq!(error["error"]["code"], "RenderRevisionChanged");
    assert_eq!(error["error"]["current_revision"], "initial");
    let reader = ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)?;
    assert!(reader.render_jobs(None, 1)?.is_empty());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn unqualified_platform_fails_explicitly_without_opening_or_migrating() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("missing.deadpan");
    let result = cli(&["render", path(&package), "--output", path(root.path())])?;
    assert_eq!(
        error(&result)?["error"]["code"],
        "RenderPlatformUnsupported"
    );
    assert!(!package.exists());
    Ok(())
}
