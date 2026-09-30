#![cfg(any(target_os = "macos", target_os = "linux"))]

use deadpan_cli::{
    encoded_render::{
        EncodedRenderError,
        admission::{AdmissionLimits, AdmissionRequest, qualify},
    },
    render_worker::{RenderWorkerRuntime, protocol::RenderIdentity},
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn runtime(root: &Path, mode: &str) -> RenderWorkerRuntime {
    let path = root.join("probe-fixture");
    let config = serde_json::json!({"mode":mode,"trace":root.join("requests.jsonl")});
    let string = serde_json::to_string(&config.to_string()).unwrap();
    fs::write(
        &path,
        format!(
            "#!/usr/bin/env python3\nimport json\nCONFIG = json.loads({string})\n{}",
            include_str!("encoder_admission/fixture.py")
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    RenderWorkerRuntime {
        executable: path,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    }
}
fn request() -> AdmissionRequest {
    AdmissionRequest {
        identity: RenderIdentity {
            request_id: RequestId::new("job").unwrap(),
            attempt_id: AttemptId::new("encoding-attempt").unwrap(),
        },
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        raster: [320, 180],
        frame_rate: [30, 1],
    }
}
fn rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn limits() -> AdmissionLimits {
    let mut limits = AdmissionLimits::default();
    limits.process.maximum_duration = Duration::from_secs(3);
    limits.process.cancellation_grace = Duration::from_millis(100);
    limits.process.exit_grace = Duration::from_millis(100);
    limits
}

#[test]
fn real_supervision_keeps_each_probe_identity_and_only_allowed_selection_steps() {
    for (mode, expected) in [
        (
            "sequence",
            vec![("hardware", "target_two"), ("hardware", "none")],
        ),
        (
            "unavailable",
            vec![("hardware", "target_two"), ("software", "target_two")],
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let error = qualify(
            &runtime(root.path(), mode),
            request(),
            limits(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(5),
            |_, _, _| {},
        )
        .err()
        .expect("last probe fails");
        assert!(error.error.cleanup_confirmed());
        assert!(
            matches!(error.error, EncodedRenderError::WorkerFailure(_)),
            "{mode}: {error:?}"
        );
        let attempts = rows(root.path());
        assert_eq!(attempts.len(), 2);
        for (row, (mode, b_frames)) in attempts.iter().zip(expected) {
            assert_eq!(row["spec"]["choice"]["mode"], mode);
            assert_eq!(row["spec"]["choice"]["b_frames"], b_frames);
        }
        assert_ne!(
            attempts[0]["identity"]["attempt_id"],
            attempts[1]["identity"]["attempt_id"]
        );
        assert_eq!(error.rejected.len(), 2);
    }
}

#[test]
fn capacity_prose_and_faults_after_failure_never_start_a_second_probe() {
    for mode in [
        "capacity",
        "misleading",
        "stale",
        "unknown",
        "regressed_progress",
        "malformed_tail",
        "duplicate",
        "crash",
        "exit2",
        "hang",
    ] {
        let root = tempfile::tempdir().unwrap();
        let error = qualify(
            &runtime(root.path(), mode),
            request(),
            limits(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(5),
            |_, _, _| {},
        )
        .err()
        .expect("hostile probe must fail");
        assert!(error.error.cleanup_confirmed(), "{mode}: {error:?}");
        assert_eq!(rows(root.path()).len(), 1, "{mode}");
        if mode == "regressed_progress" {
            assert_eq!(
                fs::read_to_string(root.path().join("requests.cancelled")).unwrap(),
                "matching cancellation received"
            );
        }
        if ["malformed_tail", "duplicate", "crash", "exit2", "hang"].contains(&mode) {
            assert!(
                matches!(error.error, EncodedRenderError::WorkerFault { .. }),
                "{mode}: {error:?}"
            );
        }
    }
}

#[test]
fn cancel_while_probe_is_live_stops_and_confirms_teardown() {
    let root = tempfile::tempdir().unwrap();
    let cancelled = AtomicBool::new(false);
    let error = qualify(
        &runtime(root.path(), "wait"),
        request(),
        limits(),
        &cancelled,
        Instant::now() + Duration::from_secs(5),
        |_, _, _| cancelled.store(true, Ordering::Release),
    )
    .err()
    .expect("cancelled");
    assert!(
        matches!(error.error, EncodedRenderError::Cancelled),
        "{error:?}"
    );
    assert!(error.error.cleanup_confirmed());
    assert_eq!(rows(root.path()).len(), 1);
}
