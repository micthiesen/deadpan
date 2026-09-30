//! Destination stages authorized by durable store permits. Run all media and
//! filesystem work off the store/UI threads. Each stage returns before the
//! caller records the next transition; no SQLite connection crosses this API.

use std::ffi::OsStr;

use deadpan_jobs::render::{
    RenderDiagnostic,
    publication::{
        PreparedPublicationEvidence, PublicationIdentity, PublicationOperationKind,
        PublicationOutcome as StoredOutcome, PublicationPhase, PublicationReconciliation,
        StoredPublication,
    },
};
use deadpan_store::publication::PublicationPermit;
use serde::Deserialize;

use super::*;
use crate::encoded_render::{jobs::encoder_choice, protocol::EncodedRenderContract};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesystemEvidence {
    schema_version: u32,
    directory: filesystem::DirectoryEvidence,
    movie: filesystem::FileEvidence,
    report: filesystem::FileEvidence,
}

/// Complete, sealed destination files. Owning this object retains their locks
/// and the live verifier capability. Store transitions revoke each old permit.
pub struct PreparedPublication {
    candidate: VerifiedCandidate,
    files: staging::PreparedFiles,
    initial: StoredPublication,
    evidence: PreparedPublicationEvidence,
}

/// Prepare both files under an Intent permit, without renaming either one.
/// Persist `evidence()` before authorizing the report stage.
pub fn prepare(
    mut candidate: VerifiedCandidate,
    package: &Path,
    permit: &PublicationPermit,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(PublicationStage),
) -> Result<PreparedPublication, Box<PublicationFailure>> {
    let mut retained = RetainedPublicationArtifacts::default();
    let result = (|| {
        validate_permit(
            permit,
            PublicationOperationKind::Publish,
            PublicationPhase::Intent,
        )?;
        validate_candidate(&candidate, permit.record(), cancelled, deadline)?;
        let intent = &permit.record().intent;
        let live = || {
            permit_live(permit)?;
            candidate
                .candidate()
                .check_live(cancelled, deadline)
                .map_err(encoded_error)
        };
        // The candidate is also checked by copy_to. The permit closure cannot
        // borrow it while copy_to holds its mutable borrow.
        live()?;
        let permit_check = || permit_live(permit);
        let controls = staging::StageControl {
            cancelled,
            deadline,
            live: &permit_check,
        };
        let files = staging::PreparedFiles::prepare(
            &mut candidate,
            staging::Preparation {
                package,
                destination: &intent.destination,
                names: staging::Names {
                    publication_id: intent.publication_id.as_str().into(),
                    report: intent.report_name(),
                    movie_partial: intent.movie_partial_name(),
                    report_partial: intent.report_partial_name(),
                },
                recoverable: true,
            },
            &controls,
            &mut progress,
            &mut retained,
        )?;
        let filesystem = FilesystemEvidence {
            schema_version: 1,
            directory: files
                .directory
                .clone()
                .ok_or_else(|| invalid("missing destination identity"))?,
            movie: files.movie.evidence().map_err(fs_error)?,
            report: files.report.evidence().map_err(fs_error)?,
        };
        let evidence = PreparedPublicationEvidence {
            schema_version: 1,
            movie_sha256: files.receipt.movie_sha256.clone(),
            movie_bytes: files.receipt.movie_bytes,
            report_sha256: files.receipt.report_sha256.clone(),
            report_bytes: files.receipt.report_bytes,
            contains_generated_pictures: files.receipt.contains_generated_pictures,
            filesystem: serde_json::to_value(filesystem)
                .map_err(|error| invalid(error.to_string()))?,
        };
        evidence
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        validate_candidate(&candidate, permit.record(), cancelled, deadline)?;
        permit_live(permit)?;
        Ok((files, evidence))
    })();
    match result {
        Ok((files, evidence)) => Ok(PreparedPublication {
            candidate,
            files,
            initial: permit.record().clone(),
            evidence,
        }),
        Err(error) => Err(Box::new(PublicationFailure {
            error,
            candidate,
            retained,
        })),
    }
}

impl PreparedPublication {
    pub fn evidence(&self) -> &PreparedPublicationEvidence {
        &self.evidence
    }

    pub fn retained(&self) -> RetainedPublicationArtifacts {
        self.files.retained()
    }

    /// Requires the durably recorded ReportCommitting phase. On error retain
    /// this object until the host has recorded failure and stopped the stage.
    pub fn commit_report(
        &mut self,
        permit: &PublicationPermit,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<(), PublicationDiagnostic> {
        self.validate_stage(
            permit,
            PublicationPhase::ReportCommitting,
            cancelled,
            deadline,
        )?;
        let live = || {
            permit_live(permit)?;
            self.candidate
                .candidate()
                .check_live(cancelled, deadline)
                .map_err(encoded_error)
        };
        self.files.commit_report(&staging::StageControl {
            cancelled,
            deadline,
            live: &live,
        })
    }

    /// Requires the durably recorded MovieCommitting phase. A successful movie
    /// rename is always returned as committed, including subsequent failures.
    pub fn commit_movie(
        mut self,
        permit: &PublicationPermit,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<PublicationOutcome, Box<PublicationFailure>> {
        let result = (|| {
            self.validate_stage(
                permit,
                PublicationPhase::MovieCommitting,
                cancelled,
                deadline,
            )?;
            let live = || {
                permit_live(permit)?;
                self.candidate
                    .candidate()
                    .check_live(cancelled, deadline)
                    .map_err(encoded_error)
            };
            self.files.commit_movie(&staging::StageControl {
                cancelled,
                deadline,
                live: &live,
            })
        })();
        result.map_err(|error| {
            Box::new(PublicationFailure {
                error,
                retained: self.files.retained(),
                candidate: self.candidate,
            })
        })
    }

    fn validate_stage(
        &self,
        permit: &PublicationPermit,
        phase: PublicationPhase,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<(), PublicationDiagnostic> {
        validate_permit(permit, PublicationOperationKind::Publish, phase)?;
        let record = permit.record();
        if record.intent != self.initial.intent
            || record.render_intent != self.initial.render_intent
            || record.encoding_attempt_id != self.initial.encoding_attempt_id
            || record.operation.operation_id != self.initial.operation.operation_id
            || record.operation.cancellation_token != self.initial.operation.cancellation_token
            || record.prepared.as_ref() != Some(&self.evidence)
        {
            return Err(invalid(
                "stage permit differs from the prepared publication",
            ));
        }
        validate_candidate(&self.candidate, record, cancelled, deadline)
    }
}

#[derive(Debug)]
pub enum RecoveryOutcome {
    /// No movie rename permit was ever issued. No destination entry is adopted
    /// or removed, including unrecorded partial files.
    NotPublished,
    Committed(PublicationOutcome),
}

/// Keep this object alive through finish_publication_reconciliation. Its owned
/// descriptors keep cooperative publishers from changing the admitted files.
pub struct RecoveryInspection {
    identity: PublicationIdentity,
    outcome: RecoveryOutcome,
    _files: Option<RecoveredFiles>,
}

struct RecoveredFiles {
    movie: filesystem::RecoveredFile,
    report: Option<filesystem::RecoveredFile>,
}

impl RecoveryInspection {
    pub fn identity(&self) -> &PublicationIdentity {
        &self.identity
    }
    pub fn outcome(&self) -> &RecoveryOutcome {
        &self.outcome
    }
    pub fn completion(&self) -> PublicationReconciliation {
        match &self.outcome {
            RecoveryOutcome::NotPublished => {
                PublicationReconciliation::NotPublished(RenderDiagnostic {
                    code: "movie_not_authorized".into(),
                    detail: "The durable journal never authorized the movie rename".into(),
                })
            }
            RecoveryOutcome::Committed(PublicationOutcome::Published(_)) => {
                PublicationReconciliation::Confirmed
            }
            RecoveryOutcome::Committed(PublicationOutcome::PublishedUnconfirmed {
                diagnostic,
                ..
            }) => PublicationReconciliation::CommittedUnconfirmed(RenderDiagnostic {
                code: diagnostic.code.clone(),
                detail: diagnostic.message.clone(),
            }),
        }
    }
}

/// Inspect a prior operation under a fresh reconciliation permit and a fresh
/// verifier capability for the same retained checkpoint. This API never
/// renames, deletes, recreates or adopts a foreign destination entry.
pub fn reconcile(
    candidate: &VerifiedCandidate,
    permit: &PublicationPermit,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RecoveryInspection, PublicationDiagnostic> {
    let record = permit.record();
    validate_permit(permit, PublicationOperationKind::Reconcile, record.phase)?;
    validate_candidate(candidate, record, cancelled, deadline)?;
    if record.phase != PublicationPhase::MovieCommitting {
        if record.observed_movie_commit {
            return Err(invalid("movie commit predates its authorization"));
        }
        return Ok(RecoveryInspection {
            identity: permit.identity(),
            outcome: RecoveryOutcome::NotPublished,
            _files: None,
        });
    }
    let prepared = record
        .prepared
        .as_ref()
        .ok_or_else(|| invalid("missing prepared evidence"))?;
    prepared
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let evidence: FilesystemEvidence = serde_json::from_value(prepared.filesystem.clone())
        .map_err(|error| invalid(error.to_string()))?;
    if evidence.schema_version != 1
        || Some(evidence.directory.selected()) != record.intent.destination.parent()
        || evidence.movie.byte_length() != prepared.movie_bytes
        || evidence.report.byte_length() != prepared.report_bytes
    {
        return Err(invalid(
            "filesystem evidence differs from the publication intent",
        ));
    }
    let live = || {
        permit_live(permit)?;
        candidate
            .candidate()
            .check_live(cancelled, deadline)
            .map_err(encoded_error)
    };
    let controls = staging::StageControl {
        cancelled,
        deadline,
        live: &live,
    };
    controls.check()?;
    let directory = filesystem::RecoveredDirectory::open(&evidence.directory).map_err(fs_error)?;
    let movie = directory
        .open_file(
            OsStr::new(
                record
                    .intent
                    .movie_name()
                    .map_err(|error| invalid(error.to_string()))?,
            ),
            &evidence.movie,
        )
        .map_err(fs_error)?
        .ok_or_else(|| {
            PublicationDiagnostic::new(
                "publication_unresolved",
                "the authorized movie rename has no matching final entry",
            )
        })?;
    if staging::hash_guarded(
        movie.reader(cancelled, deadline).map_err(fs_error)?,
        prepared.movie_bytes,
        &controls,
    )? != prepared.movie_sha256
    {
        return Err(PublicationDiagnostic::new(
            "published_hash_mismatch",
            "recorded final movie bytes differ",
        ));
    }
    movie.confirm().map_err(fs_error)?;
    // Matching persistent identity and complete movie bytes prove the final
    // entry was committed. Every later failure must preserve that observation.
    let receipt = PublicationReceipt {
        publication_id: record.intent.publication_id.as_str().into(),
        movie: evidence.directory.canonical().join(
            record
                .intent
                .movie_name()
                .map_err(|error| invalid(error.to_string()))?,
        ),
        report: evidence
            .directory
            .canonical()
            .join(record.intent.report_name()),
        movie_sha256: prepared.movie_sha256.clone(),
        movie_bytes: prepared.movie_bytes,
        report_sha256: prepared.report_sha256.clone(),
        report_bytes: prepared.report_bytes,
        contains_generated_pictures: prepared.contains_generated_pictures,
    };
    let mut files = RecoveredFiles {
        movie,
        report: None,
    };
    let result = (|| {
        controls.check()?;
        if directory
            .open_file(
                OsStr::new(&record.intent.movie_partial_name()),
                &evidence.movie,
            )
            .map_err(fs_error)?
            .is_some()
        {
            return Err(invalid("both partial and final movie entries are present"));
        }
        files.report = Some(
            directory
                .open_file(OsStr::new(&record.intent.report_name()), &evidence.report)
                .map_err(fs_error)?
                .ok_or_else(|| {
                    PublicationDiagnostic::new(
                        "report_missing",
                        "recorded publication report is absent",
                    )
                })?,
        );
        let report = files.report.as_ref().expect("report was retained above");
        if staging::hash_guarded(
            report.reader(cancelled, deadline).map_err(fs_error)?,
            prepared.report_bytes,
            &controls,
        )? != prepared.report_sha256
        {
            return Err(PublicationDiagnostic::new(
                "report_hash_mismatch",
                "recorded final report bytes differ",
            ));
        }
        if directory
            .open_file(
                OsStr::new(&record.intent.report_partial_name()),
                &evidence.report,
            )
            .map_err(fs_error)?
            .is_some()
        {
            return Err(invalid("both partial and final report entries are present"));
        }
        controls.check()?;
        files.movie.sync_verified().map_err(fs_error)?;
        report.sync_verified().map_err(fs_error)?;
        files.movie.confirm().map_err(fs_error)?;
        report.confirm().map_err(fs_error)?;
        directory.confirm().map_err(fs_error)?;
        controls.check()
    })();
    let outcome = match result {
        Ok(()) => PublicationOutcome::Published(receipt),
        Err(diagnostic) => PublicationOutcome::PublishedUnconfirmed {
            receipt,
            diagnostic,
        },
    };
    Ok(RecoveryInspection {
        identity: permit.identity(),
        outcome: RecoveryOutcome::Committed(outcome),
        _files: Some(files),
    })
}

fn validate_candidate(
    candidate: &VerifiedCandidate,
    record: &StoredPublication,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), PublicationDiagnostic> {
    candidate
        .candidate()
        .check_live(cancelled, deadline)
        .map_err(encoded_error)?;
    let identity = candidate.verification_identity();
    let contract = candidate.candidate().contract();
    let report = candidate.report();
    if identity.request_id != record.intent.job_id
        || identity.attempt_id != record.operation.verified_attempt_id
        || contract.project_id() != &record.render_intent.project_id
        || contract.revision_id() != &record.render_intent.revision_id
        || contract.range() != record.render_intent.range
        || candidate.candidate().document_sha256() != &record.render_intent.document_sha256
        || report.document_sha256 != record.render_intent.document_sha256
        || report.movie_sha256 != record.movie_sha256
        || report.movie_bytes != record.movie_bytes
        || candidate.candidate().byte_length() != record.movie_bytes
        || report.contract
            != EncodedRenderContract::from_contract(
                contract,
                encoder_choice(&record.render_intent.policy),
            )
    {
        return Err(invalid(
            "live verification differs from the exact recorded attempt, intent or movie",
        ));
    }
    Ok(())
}

fn validate_permit(
    permit: &PublicationPermit,
    kind: PublicationOperationKind,
    phase: PublicationPhase,
) -> Result<(), PublicationDiagnostic> {
    permit_live(permit)?;
    let record = permit.record();
    record
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    if record.operation.kind != kind
        || record.phase != phase
        || !record.operation.active
        || record.outcome != StoredOutcome::InProgress
        || record.cancellation_requested
    {
        return Err(invalid(
            "publication requires its exact active operation and phase",
        ));
    }
    Ok(())
}

fn permit_live(permit: &PublicationPermit) -> Result<(), PublicationDiagnostic> {
    permit
        .check_live()
        .map_err(|error| PublicationDiagnostic::new("publication_revoked", error))
}

fn invalid(message: impl ToString) -> PublicationDiagnostic {
    PublicationDiagnostic::new("invalid_publication", message)
}
