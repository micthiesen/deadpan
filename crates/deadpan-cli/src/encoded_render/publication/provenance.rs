//! Report data derived from the exact committed input of a verified candidate.
//!
//! The catalog is an explicit superset, not a claim that every asset rendered.
//! Generated intervals enumerate at most one million actual output frames using
//! the indexed picture resolver, then coalesce equal effective artifacts. Repeat
//! structures are never expanded. Capacity exhaustion fails without truncation.
//! The three direct generated references include the immutable provenance hash;
//! that object transitively binds conditioning inputs, which are not expanded or
//! read again here. This is renderer-dependency reporting, not media admission.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_core::{
    AssetId, AssetRecord, FrameRange, GeneratedArtifact, ProjectDocument, ProjectFrame, ProjectId,
    RevisionId, SourceQualificationId,
};
use deadpan_jobs::{
    Sha256,
    render::{RenderIntent, admission::RenderEncodingDecision},
};
use deadpan_plan::{Picture, RenderPlan};
use deadpan_store::{AccessMode, ProjectStore, original_media::OriginalObjectRef};
use serde::Serialize;

use crate::encoded_render::{
    protocol::EncodedManifest,
    verification::{VerificationReport, VerifiedCandidate},
};
use crate::export_picture::ExportPictureContract;
use crate::render_worker::{RenderWorkerError, document_hash};

const MAX_OUTPUT_FRAMES: u64 = 1_000_000;
const MAX_CATALOG_ASSETS: usize = 4_096;
const MAX_GENERATED_ARTIFACTS: usize = 4_096;
const MAX_GENERATED_INTERVALS: usize = 65_536;
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    #[error("publication provenance was cancelled")]
    Cancelled,
    #[error("publication provenance exceeded its deadline")]
    Deadline,
    #[error("publication provenance exceeds its {0} capacity")]
    Capacity(&'static str),
    #[error("publication provenance differs from its captured input: {0}")]
    Binding(&'static str),
    #[error(transparent)]
    Store(#[from] deadpan_store::StoreError),
    #[error(transparent)]
    Plan(#[from] deadpan_plan::PlanError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    DocumentHash(#[from] RenderWorkerError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl ProvenanceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline_exceeded",
            _ => "provenance_failed",
        }
    }
}

/// Serialize-only evidence, constructible only from a verified candidate and
/// a matching historical read-only snapshot. No paths or authored labels leak.
#[derive(Debug, Serialize)]
pub struct PublicationProvenance {
    schema_version: u32,
    #[serde(flatten)]
    document: DocumentProvenance,
    encoder_selection: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    render_intent: Option<RenderIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding_decision: Option<RenderEncodingDecision>,
    encoded_manifest: EncodedManifest,
    verification: VerificationReport,
}

impl PublicationProvenance {
    pub fn has_generated(&self) -> bool {
        !self.document.generated_intervals.is_empty()
    }
}

#[derive(Debug, Serialize)]
struct DocumentProvenance {
    project_id: ProjectId,
    revision_id: RevisionId,
    document_sha256: Sha256,
    range: FrameRange,
    catalog_scope: &'static str,
    catalog: Vec<CatalogAsset>,
    generated_dependency_scope: &'static str,
    generated_artifacts: Vec<GeneratedArtifact>,
    generated_intervals: Vec<GeneratedInterval>,
}

#[derive(Debug, Serialize)]
struct CatalogAsset {
    asset: AssetId,
    qualification: CatalogQualification,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum CatalogQualification {
    Qualified {
        receipt: SourceQualificationId,
        original: OriginalObjectRef,
        source_sha256: String,
    },
    /// Legacy and generated catalog records have no Original receipt. Their
    /// arbitrary content_hash strings are not promoted into verified hashes.
    Unqualified,
}

#[derive(Debug, Serialize)]
struct GeneratedInterval {
    artifact_index: usize,
    project_range: FrameRange,
    /// Relative output frame ordinals, half-open, independently of source PTS.
    output_range: [u64; 2],
}

pub fn capture(
    package: &Path,
    candidate: &VerifiedCandidate,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PublicationProvenance, ProvenanceError> {
    check_control(cancelled, deadline)?;
    let store = ProjectStore::open(package, AccessMode::ReadOnly);
    check_control(cancelled, deadline)?;
    let store = store?;
    let encoded = candidate.candidate();
    let document = capture_revision(
        &store,
        encoded.contract(),
        encoded.document_sha256(),
        cancelled,
        deadline,
    )?;
    let (render_intent, encoding_decision) =
        match (encoded.encoding_decision(), encoded.encoding_binding()) {
            (None, None) => (None, None),
            (Some(decision), Some(binding)) => {
                let intent = store.render_job(&decision.job_id)?;
                let stored = store
                    .render_encoding_decision(&decision.job_id, &decision.encoding_attempt_id)?;
                if stored.as_ref() != Some(decision) {
                    return Err(ProvenanceError::Binding("original encoding decision"));
                }
                let expected = crate::encoded_render::admission::durable::binding_for_decision(
                    &intent,
                    &decision.encoding_attempt_id,
                    encoded.contract(),
                    decision,
                )
                .map_err(|_| ProvenanceError::Binding("automatic output controls and runtime"))?;
                if binding != &expected {
                    return Err(ProvenanceError::Binding("recorded encoding runtime"));
                }
                (Some(intent), Some(decision.clone()))
            }
            _ => {
                return Err(ProvenanceError::Binding(
                    "incomplete durable automatic provenance",
                ));
            }
        };
    let automatic = encoding_decision.is_some();
    let result = PublicationProvenance {
        schema_version: if automatic { 2 } else { 1 },
        document,
        encoder_selection: encoding_decision
            .as_ref()
            .map_or("explicit_engineering_choice", |decision| {
                decision.algorithm.as_str()
            }),
        render_intent,
        encoding_decision,
        encoded_manifest: encoded.manifest().clone(),
        verification: candidate.report().clone(),
    };
    check_serialized_size(&result, cancelled, deadline)?;
    Ok(result)
}

fn capture_revision(
    store: &ProjectStore,
    contract: &ExportPictureContract,
    expected_hash: &Sha256,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<DocumentProvenance, ProvenanceError> {
    check_control(cancelled, deadline)?;
    if contract.frame_count() > MAX_OUTPUT_FRAMES {
        return Err(ProvenanceError::Capacity("output-frame scan"));
    }
    let document = store.snapshot_at(contract.revision_id());
    check_control(cancelled, deadline)?;
    let document = document?;
    if document.project_id() != contract.project_id()
        || document.revision_id() != contract.revision_id()
    {
        return Err(ProvenanceError::Binding("project or revision identity"));
    }
    let actual_hash = document_hash(&document, cancelled, deadline);
    check_control(cancelled, deadline)?;
    let actual_hash = actual_hash?;
    if &actual_hash != expected_hash {
        return Err(ProvenanceError::Binding("complete document hash"));
    }
    let catalog = catalog(store, &document, cancelled, deadline)?;
    let plan = RenderPlan::compile(&document)?;
    check_control(cancelled, deadline)?;
    let (generated_artifacts, generated_intervals) = generated_intervals(
        &plan,
        contract.range(),
        MAX_GENERATED_ARTIFACTS,
        MAX_GENERATED_INTERVALS,
        cancelled,
        deadline,
    )?;
    Ok(DocumentProvenance {
        project_id: document.project_id().clone(),
        revision_id: document.revision_id().clone(),
        document_sha256: actual_hash,
        range: contract.range(),
        catalog_scope: "committed_catalog_superset",
        catalog,
        generated_dependency_scope: "effective_picture_masters_and_transitive_provenance",
        generated_artifacts,
        generated_intervals,
    })
}

fn catalog(
    store: &ProjectStore,
    document: &ProjectDocument,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<CatalogAsset>, ProvenanceError> {
    if document.assets().len() > MAX_CATALOG_ASSETS {
        return Err(ProvenanceError::Capacity("committed catalog"));
    }
    // Receipt indexes can be large. Retain only compact output and its admitted
    // asset contract; aliases sharing one receipt do not repeat receipt reads.
    let mut receipts: BTreeMap<SourceQualificationId, (AssetRecord, CatalogQualification)> =
        BTreeMap::new();
    let mut result = Vec::with_capacity(document.assets().len());
    for (asset, record) in document.assets() {
        check_control(cancelled, deadline)?;
        let qualification = if let Some(id) = &record.source_qualification {
            let mut unlabeled = record.clone();
            unlabeled.label.clear();
            if let Some((admitted, qualification)) = receipts.get(id) {
                if &unlabeled != admitted {
                    return Err(ProvenanceError::Binding("aliased source receipt"));
                }
                qualification.clone()
            } else {
                let receipt = store.registered_source(document.revision_id(), asset);
                check_control(cancelled, deadline)?;
                let receipt = receipt?;
                let content = receipt.snapshot().content();
                if content.byte_length() != receipt.original().byte_length() {
                    return Err(ProvenanceError::Binding("source byte length"));
                }
                let mut source_sha256 = String::with_capacity(64);
                for byte in content.sha256() {
                    write!(&mut source_sha256, "{byte:02x}").expect("String writes cannot fail");
                }
                let qualification = CatalogQualification::Qualified {
                    receipt: receipt.id().clone(),
                    original: receipt.original().clone(),
                    source_sha256,
                };
                receipts.insert(id.clone(), (unlabeled, qualification.clone()));
                qualification
            }
        } else {
            CatalogQualification::Unqualified
        };
        result.push(CatalogAsset {
            asset: asset.clone(),
            qualification,
        });
    }
    check_control(cancelled, deadline)?;
    Ok(result)
}

fn generated_intervals(
    plan: &RenderPlan,
    range: FrameRange,
    maximum_artifacts: usize,
    maximum_intervals: usize,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(Vec<GeneratedArtifact>, Vec<GeneratedInterval>), ProvenanceError> {
    check_control(cancelled, deadline)?;
    if range.start().0 < 0
        || range.end().0 > plan.duration().frames()
        || range.duration().frames() == 0
    {
        return Err(ProvenanceError::Binding("output range"));
    }
    let count = u64::try_from(range.duration().frames())
        .map_err(|_| ProvenanceError::Binding("output frame count"))?;
    if count > MAX_OUTPUT_FRAMES {
        return Err(ProvenanceError::Capacity("output-frame scan"));
    }
    let mut artifacts: Vec<GeneratedArtifact> = Vec::new();
    let mut indexes = BTreeMap::new();
    let mut intervals: Vec<GeneratedInterval> = Vec::new();
    for project_frame in range.start().0..range.end().0 {
        check_control(cancelled, deadline)?;
        let picture = plan.picture(ProjectFrame(project_frame))?;
        let Picture::Accepted {
            generated: Some(artifact),
            ..
        } = picture.picture
        else {
            continue;
        };
        let end = project_frame
            .checked_add(1)
            .ok_or(deadpan_core::TimeError::Overflow)?;
        let output_end = u64::try_from(end - range.start().0)
            .map_err(|_| ProvenanceError::Binding("relative output frame"))?;
        if let Some(previous) = intervals.last_mut()
            && previous.project_range.end().0 == project_frame
            && artifacts[previous.artifact_index] == *artifact
        {
            previous.project_range =
                FrameRange::new(previous.project_range.start(), ProjectFrame(end))?;
            previous.output_range[1] = output_end;
            continue;
        }
        if intervals.len() >= maximum_intervals {
            return Err(ProvenanceError::Capacity("generated intervals"));
        }
        // Compare complete durable artifacts, not a label, sampled asset alone
        // or current request relevance. Serialization here is small and bounded
        // by the validated GeneratedArtifact grammar, once per interval.
        let key = serde_json::to_string(artifact.as_ref())?;
        let index = if let Some(index) = indexes.get(&key) {
            *index
        } else {
            if artifacts.len() >= maximum_artifacts {
                return Err(ProvenanceError::Capacity("generated artifact table"));
            }
            let index = artifacts.len();
            artifacts.push(artifact.as_ref().clone());
            indexes.insert(key, index);
            index
        };
        intervals.push(GeneratedInterval {
            artifact_index: index,
            project_range: FrameRange::new(ProjectFrame(project_frame), ProjectFrame(end))?,
            output_range: [output_end - 1, output_end],
        });
    }
    check_control(cancelled, deadline)?;
    Ok((artifacts, intervals))
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), ProvenanceError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ProvenanceError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(ProvenanceError::Deadline);
    }
    Ok(())
}

fn check_serialized_size(
    report: &impl Serialize,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), ProvenanceError> {
    struct Counter<'a> {
        bytes: usize,
        exceeded: bool,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    }
    impl Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            check_control(self.cancelled, self.deadline).map_err(io::Error::other)?;
            if bytes.len() > MAX_REPORT_BYTES.saturating_sub(self.bytes) {
                self.exceeded = true;
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "provenance byte limit",
                ));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        bytes: 0,
        exceeded: false,
        cancelled,
        deadline,
    };
    let serialized = serde_json::to_writer(&mut counter, report);
    check_control(cancelled, deadline)?;
    if counter.exceeded {
        return Err(ProvenanceError::Capacity("serialized report"));
    }
    serialized?;
    Ok(())
}

#[cfg(test)]
mod tests;
