use super::*;
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
use sha2::{Digest, Sha256 as Hash};
use std::io::Read;

pub(super) fn extension_request() -> HostMessage {
    let mut request: HostMessage =
        serde_json::from_str(include_str!("../fixtures/generate_extension_v3.json")).unwrap();
    if let HostMessage::GenerateExtension {
        identity: target_identity,
        cancellation_token,
        ..
    } = &mut request
    {
        *target_identity = identity();
        *cancellation_token = CancellationToken::new("cancel").unwrap();
    }
    request
}

fn completion() -> WorkerMessage {
    let mut message: WorkerMessage =
        serde_json::from_str(include_str!("../fixtures/completed_extension_v3.json")).unwrap();
    if let WorkerMessage::CompletedExtension {
        identity: target_identity,
        ..
    } = &mut message
    {
        *target_identity = identity();
    }
    message
}

#[test]
fn extension_supervisor_delivers_distinct_bundle_after_clean_exit_then_snapshots_exact_bytes() {
    for direction in ["from_left", "from_right"] {
        let workspace = tempfile::tempdir().unwrap();
        fs::create_dir(workspace.path().join("outputs")).unwrap();
        let native = b"native complete movie including retained context";
        let provenance = b"immutable extension provenance";
        fs::write(workspace.path().join("outputs/native.mp4"), native).unwrap();
        fs::write(workspace.path().join("outputs/provenance.json"), provenance).unwrap();
        let mut completed = completion();
        if let WorkerMessage::CompletedExtension { candidate, .. } = &mut completed {
            candidate.native = WorkspaceArtifact::new(
                candidate.native.reference().clone(),
                Sha256::new(
                    Hash::digest(native)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                )
                .unwrap(),
                native.len() as u64,
            )
            .unwrap();
            candidate.provenance = WorkspaceArtifact::new(
                candidate.provenance.reference().clone(),
                Sha256::new(
                    Hash::digest(provenance)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                )
                .unwrap(),
                provenance.len() as u64,
            )
            .unwrap();
        }
        responses(
            workspace.path(),
            &[
                WorkerMessage::Stage {
                    protocol: ProtocolVersion::V3,
                    identity: identity(),
                    stage: WorkerStage::Preflight,
                },
                WorkerMessage::Stage {
                    protocol: ProtocolVersion::V3,
                    identity: identity(),
                    stage: WorkerStage::Inference,
                },
                completed.clone(),
            ],
        );
        let mut wire = serde_json::to_value(extension_request()).unwrap();
        wire["plan"]["sampling"]["direction"] = serde_json::json!(direction);
        wire["constraints"]["conditioning"] = serde_json::json!(if direction == "from_left" {
            "extend_from_left"
        } else {
            "extend_from_right"
        });
        let request: HostMessage = serde_json::from_value(wire).unwrap();
        let mut process =
            WorkerProcess::spawn(spec(workspace.path(), "fragmented"), request.clone()).unwrap();
        let events = finish(&mut process);
        assert!(faults(&events).is_empty(), "{:?}", faults(&events));
        let received: HostMessage =
            serde_json::from_slice(&fs::read(workspace.path().join("received.json")).unwrap())
                .unwrap();
        assert_eq!(received, request);
        assert!(events.iter().any(
            |event| matches!(event, ProcessEvent::Message(message) if **message == completed)
        ));
        let HostMessage::GenerateExtension {
            identity,
            cancellation_token,
            project_id,
            target,
            input,
            output_workspace,
            ..
        } = request
        else {
            unreachable!()
        };
        let mut job = JobLifecycle::new_with_protocol(
            identity,
            cancellation_token,
            TargetBinding {
                project_id,
                hold_id: target.hold_id,
                request_version: target.request_version,
                context_sha256: input.sha256,
            },
            ProtocolVersion::V3,
        );
        for event in events {
            if let ProcessEvent::Message(message) = event {
                job.apply_worker_message(&message).unwrap();
            }
        }
        assert_eq!(job.state(), JobState::Validating);
        assert!(job.candidate_bundle().is_none());
        assert!(!job.can_authorize_acceptance());
        let artifacts = ArtifactWorkspace::open(workspace.path()).unwrap();
        let bundle = job.extension_candidate_bundle().unwrap();
        for (declaration, expected) in [
            (&bundle.native, native.as_slice()),
            (&bundle.provenance, provenance.as_slice()),
        ] {
            let mut snapshot = artifacts
                .snapshot(
                    &output_workspace,
                    declaration,
                    ArtifactLimits::new(1024).unwrap(),
                )
                .unwrap();
            fs::write(
                workspace.path().join(declaration.reference().as_str()),
                b"changed after capture",
            )
            .unwrap();
            let mut actual = Vec::new();
            snapshot.read_to_end(&mut actual).unwrap();
            assert_eq!(actual, expected);
        }
        assert_eq!(
            JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap(),
            job
        );
    }
}

#[test]
fn extension_supervisor_rejects_crossed_versions_wrong_attempts_and_failed_teardown() {
    let mut wrong_identity = completion();
    if let WorkerMessage::CompletedExtension { identity, .. } = &mut wrong_identity {
        identity.attempt_id = AttemptId::new("old-attempt").unwrap();
    }
    let WorkerMessage::CompletedExtension { candidate, .. } = completion() else {
        unreachable!()
    };
    let crossed = WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: identity(),
        candidate,
    };
    for (mode, messages) in [
        ("messages", vec![wrong_identity]),
        ("messages", vec![crossed]),
        ("messages", vec![completion(), completion()]),
        ("exit-failure", vec![completion()]),
        ("hang-after-terminal", vec![completion()]),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        responses(workspace.path(), &messages);
        let mut specification = spec(workspace.path(), mode);
        specification.limits.exit_grace = Duration::from_millis(100);
        let mut process = WorkerProcess::spawn(specification, extension_request()).unwrap();
        let events = finish(&mut process);
        assert!(!faults(&events).is_empty(), "{mode}");
        assert!(!events.iter().any(|event| matches!(event, ProcessEvent::Message(message) if matches!(message.as_ref(), WorkerMessage::CompletedExtension { .. }))), "{mode}");
    }
}
