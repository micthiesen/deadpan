//! Checked projection between live native admission and frozen pure evidence.
//! Reading this evidence never recreates a QualifiedEncoder.

use std::io::Cursor;

use deadpan_jobs::{
    AttemptId, Diagnostic,
    render::{RenderIntent, admission::*},
};
use serde::{Serialize, de::DeserializeOwned};

use super::{AdmissionFailure, AdmissionLimits, ProbeReport, QualifiedEncoder, RejectedProbe};
use crate::{
    encoded_render::{
        EncodedRenderError,
        protocol::EncodedRenderContract,
        runtime::{EncodingBinding, ResolvedSdrSettings},
    },
    export_picture::ExportPictureContract,
};

fn invalid(error: impl ToString) -> EncodedRenderError {
    EncodedRenderError::Protocol(error.to_string())
}

/// Both sides have strict typed wire shapes. The fixed sink bounds serialization
/// before parsing, including error evidence supplied by a worker.
fn observation<T: Serialize, U: DeserializeOwned>(value: &T) -> Result<U, EncodedRenderError> {
    let mut buffer = vec![0_u8; MAX_RENDER_DECISION_BYTES];
    let mut sink = Cursor::new(buffer.as_mut_slice());
    serde_json::to_writer(&mut sink, value)?;
    let used = usize::try_from(sink.position()).map_err(invalid)?;
    Ok(serde_json::from_slice(&buffer[..used])?)
}

fn rejected(probes: &[RejectedProbe]) -> Result<Vec<RenderProbeObservation>, EncodedRenderError> {
    if probes.len() > MAX_RENDER_PROBES {
        return Err(invalid(
            "automatic rejection count exceeds its frozen bound",
        ));
    }
    probes
        .iter()
        .enumerate()
        .map(|(ordinal, probe)| {
            Ok(RenderProbeObservation {
                ordinal: u32::try_from(ordinal).map_err(invalid)?,
                identity: observation(&probe.identity)?,
                spec: observation(&probe.spec)?,
                result: RenderProbeOutcome::Rejected {
                    failure: observation(&probe.failure)?,
                    runtime: probe.runtime.as_ref().map(observation).transpose()?,
                },
            })
        })
        .collect()
}

pub(crate) fn from_qualified(
    intent: &RenderIntent,
    attempt: &AttemptId,
    contract: &ExportPictureContract,
    qualified: &QualifiedEncoder,
) -> Result<RenderEncodingDecision, EncodedRenderError> {
    let live = qualified.decision();
    if live.identity.request_id != intent.job_id
        || &live.identity.attempt_id != attempt
        || live.policy_version != super::POLICY_VERSION
    {
        return Err(invalid(
            "fresh admission belongs to another encoding attempt or algorithm",
        ));
    }
    live.selected
        .validate(AdmissionLimits::default().encode)
        .map_err(invalid)?;
    if live.selected.spec.color_policy != contract.color_policy() {
        return Err(invalid(
            "fresh admission probed a different output color than the committed branch",
        ));
    }
    let binding = EncodingBinding::from_contract(
        &live.selected.manifest.contract,
        live.selected.runtime.clone(),
    )
    .map_err(invalid)?;
    let selected = RenderProbeReport {
        schema_version: live.selected.schema_version,
        spec: observation(&live.selected.spec)?,
        manifest: observation(&live.selected.manifest)?,
        verification: observation(&live.selected.verification)?,
        content: observation(&live.selected.content)?,
        runtime: observation(&live.selected.runtime)?,
        settings: observation(&binding.settings)?,
    };
    let mut probes = rejected(&live.rejected)?;
    let ordinal = u32::try_from(probes.len()).map_err(invalid)?;
    probes.push(RenderProbeObservation {
        ordinal,
        identity: observation(&live.selected_identity)?,
        spec: selected.spec.clone(),
        result: RenderProbeOutcome::Succeeded {
            report: Box::new(selected),
        },
    });
    let decision = RenderEncodingDecision {
        schema_version: 1,
        job_id: intent.job_id.clone(),
        encoding_attempt_id: attempt.clone(),
        algorithm: intent
            .policy
            .automatic()
            .ok_or_else(|| invalid("automatic decision requires an automatic intent"))?
            .algorithm,
        document_sha256: intent.document_sha256.clone(),
        output: observation(contract)?,
        runtime: Some(observation(&live.runtime)?),
        probes,
        outcome: RenderDecisionOutcome::Selected {
            probe_ordinal: ordinal,
        },
    };
    binding_for_decision(intent, attempt, contract, &decision)?;
    Ok(decision)
}

/// Reconstruct only the historical expected controls/runtime. This function
/// neither probes the current host nor grants permission to encode on it.
pub(crate) fn binding_for_decision(
    intent: &RenderIntent,
    attempt: &AttemptId,
    contract: &ExportPictureContract,
    decision: &RenderEncodingDecision,
) -> Result<EncodingBinding, EncodedRenderError> {
    decision.validate_for(intent, attempt).map_err(invalid)?;
    if decision.output != observation(contract)? {
        return Err(invalid(
            "durable encoder decision changed the committed output contract",
        ));
    }
    let selected = decision
        .selected()
        .ok_or_else(|| invalid("encoding has no successful selected probe"))?;
    let report = ProbeReport {
        schema_version: selected.schema_version,
        spec: observation(&selected.spec)?,
        manifest: observation(&selected.manifest)?,
        verification: observation(&selected.verification)?,
        content: observation(&selected.content)?,
        runtime: observation(&selected.runtime)?,
    };
    report
        .validate_retained(AdmissionLimits::default().encode)
        .map_err(invalid)?;
    if report.spec.color_policy != contract.color_policy() {
        return Err(invalid(
            "durable encoder decision probed a different output color",
        ));
    }
    let encoding = EncodedRenderContract::from_contract(contract, report.spec.choice);
    let binding = EncodingBinding::from_contract(&encoding, report.runtime).map_err(invalid)?;
    if binding.settings != observation::<_, ResolvedSdrSettings>(&selected.settings)? {
        return Err(invalid(
            "durable encoder controls differ from frozen native policy",
        ));
    }
    Ok(binding)
}

/// Retain failed qualification only after the host established process cleanup.
/// A terminal error may contain useful rejected probes without selecting one.
pub(crate) fn from_failure(
    intent: &RenderIntent,
    attempt: &AttemptId,
    contract: &ExportPictureContract,
    failure: &AdmissionFailure,
) -> Result<RenderEncodingDecision, EncodedRenderError> {
    if !failure.error.cleanup_confirmed() {
        return Err(invalid(
            "unconfirmed probe cleanup cannot become terminal decision evidence",
        ));
    }
    let kind = failure_kind(&failure.error)?;
    let mut detail = match &failure.error {
        EncodedRenderError::WorkerFailure(failure) => failure.diagnostic.as_str().to_owned(),
        error => error.to_string().replace('\0', "�"),
    };
    let mut end = detail
        .len()
        .min(deadpan_jobs::render::MAX_RENDER_DIAGNOSTIC_BYTES);
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    let terminal = RenderAdmissionFailure {
        kind,
        diagnostic: Diagnostic::new(detail).map_err(invalid)?,
    };
    let probes = rejected(&failure.rejected)?;
    let mut decision = RenderEncodingDecision {
        schema_version: 1,
        job_id: intent.job_id.clone(),
        encoding_attempt_id: attempt.clone(),
        algorithm: intent
            .policy
            .automatic()
            .ok_or_else(|| invalid("failed automatic admission requires an automatic intent"))?
            .algorithm,
        document_sha256: intent.document_sha256.clone(),
        output: observation(contract)?,
        runtime: None,
        probes,
        outcome: RenderDecisionOutcome::Rejected {
            failure: terminal.clone(),
        },
    };
    // A stop due to changed runtime, control, I/O or protocol is not a terminal
    // capability rejection, even when earlier probes returned such a failure.
    if decision.validate_for(intent, attempt).is_err() {
        decision.outcome = RenderDecisionOutcome::Aborted { failure: terminal };
    }
    decision.validate_for(intent, attempt).map_err(invalid)?;
    Ok(decision)
}

fn failure_kind(
    error: &EncodedRenderError,
) -> Result<RenderAdmissionFailureKind, EncodedRenderError> {
    use RenderAdmissionFailureKind as K;
    Ok(match error {
        EncodedRenderError::Configuration(_) => K::Configuration,
        EncodedRenderError::Protocol(_) => K::Protocol,
        EncodedRenderError::Worker(_) => K::Worker,
        EncodedRenderError::WorkerFailure(failure) => K::WorkerFailure(observation(&failure.kind)?),
        EncodedRenderError::WorkerFault { .. } => K::WorkerFault,
        EncodedRenderError::Cancelled => K::Cancelled,
        EncodedRenderError::Deadline => K::Deadline,
        EncodedRenderError::Picture(_) => K::Picture,
        EncodedRenderError::Output(_) => K::Output,
        EncodedRenderError::Audio(_) => K::Audio,
        EncodedRenderError::Encode(error) => K::Encode(observation(&error.kind())?),
        EncodedRenderError::Render(_) => K::Render,
        EncodedRenderError::Supervisor(_) => K::Supervisor,
        EncodedRenderError::Artifact(_) => K::Artifact,
        EncodedRenderError::RetainedMedia(_) => K::RetainedMedia,
        EncodedRenderError::Io(_) => K::Io,
        EncodedRenderError::Json(_) => K::Json,
        EncodedRenderError::CleanupUnconfirmed { .. } => K::UnresolvedCleanup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_measured_probe_preserves_every_native_observation_and_frozen_control() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
        ));
        let decision = RenderEncodingDecision::from_json(bytes).unwrap();
        let selected = decision.selected().unwrap();
        let report = ProbeReport {
            schema_version: selected.schema_version,
            spec: observation(&selected.spec).unwrap(),
            manifest: observation(&selected.manifest).unwrap(),
            verification: observation(&selected.verification).unwrap(),
            content: observation(&selected.content).unwrap(),
            runtime: observation(&selected.runtime).unwrap(),
        };
        assert!(report.validate(AdmissionLimits::default().encode).is_err());
        report
            .validate_retained(AdmissionLimits::default().encode)
            .unwrap();
        assert_eq!(
            observation::<_, RenderProbeManifest>(&report.manifest).unwrap(),
            selected.manifest
        );
        assert_eq!(
            observation::<_, RenderProbeVerification>(&report.verification).unwrap(),
            selected.verification
        );
        assert_eq!(
            observation::<_, RenderProbeContent>(&report.content).unwrap(),
            selected.content
        );
        assert_eq!(
            observation::<_, RenderRuntimeFingerprint>(&report.runtime).unwrap(),
            selected.runtime
        );
        let native = report.manifest.contract.native_contract().unwrap();
        assert_eq!(
            observation::<_, RenderSdrSettings>(&ResolvedSdrSettings::from(native.policy()))
                .unwrap(),
            selected.settings
        );
        for probe in &decision.probes {
            if let RenderProbeOutcome::Rejected { failure, .. } = &probe.result {
                let actual: crate::encoded_render::protocol::EncodedFailure =
                    observation(failure).unwrap();
                assert_eq!(
                    observation::<_, RenderProbeFailure>(&actual).unwrap(),
                    *failure
                );
            }
        }
    }

    #[test]
    fn observation_projection_is_bounded_before_parsing() {
        assert!(observation::<_, String>(&"x".repeat(MAX_RENDER_DECISION_BYTES)).is_err());
        assert_eq!(observation::<_, String>(&"bounded").unwrap(), "bounded");
    }

    #[test]
    fn invalidated_native_failure_retains_fault_boundary_instead_of_capability() {
        let reported =
            EncodedRenderError::WorkerFailure(crate::encoded_render::protocol::EncodedFailure {
                kind: crate::encoded_render::protocol::EncodedFailureKind::Encoder(
                    deadpan_encode::EncodeFailureKind::VideoTimestampOrder,
                ),
                diagnostic: Diagnostic::new("native timestamp order").unwrap(),
            });
        assert_eq!(
            failure_kind(&reported).unwrap(),
            RenderAdmissionFailureKind::WorkerFailure(RenderProbeFailureKind::Encoder(
                RenderEncodeFailureKind::VideoTimestampOrder
            ))
        );
        let invalidated = EncodedRenderError::WorkerFault {
            primary: Box::new(reported),
            fault: "late malformed response".into(),
        };
        assert!(invalidated.cleanup_confirmed());
        assert_eq!(
            failure_kind(&invalidated).unwrap(),
            RenderAdmissionFailureKind::WorkerFault
        );
    }
}
