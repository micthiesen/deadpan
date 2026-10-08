//! Operation-specific summaries of retained host evidence, never media admission.

use super::QualityReading;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{GeneratedObjectRef, NodeId, ProjectId, RevisionId};
use deadpan_jobs::{GenerationPlan, MessageIdentity, VideoSpec};
use deadpan_models::{
    ExtensionContext, ExtensionEndpointJoin, ExtensionGenerationBinding, ExtensionJoinRole,
    ModelInputConversion, StoredBridgeProvenance, StoredExtensionProvenance,
    ValidatedExtensionEvidence,
};
use deadpan_store::generated_media::{GeneratedReadHandle, GeneratedReadLimits};
use deadpan_store::generation_attempts::BundleValidationReceipt;
use sha2::Digest;

const MAX_MANIFEST_BYTES: u64 = 1 << 20;
const MAX_PROVENANCE_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct VariantFacts {
    pub project: ProjectId,
    pub identity: MessageIdentity,
    pub origin: RevisionId,
    pub hold: NodeId,
}

fn metadata(
    handle: &GeneratedReadHandle,
    object: &GeneratedObjectRef,
    maximum: u64,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("reading was cancelled".into());
    }
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or("reading timed out")?;
    let limits = GeneratedReadLimits::new(maximum, remaining).map_err(|error| error.to_string())?;
    let snapshot = handle
        .snapshot(object, limits, cancelled)
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(object.byte_length()).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    snapshot
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != object.byte_length()
        || cancelled.load(Ordering::Acquire)
        || Instant::now() >= deadline
    {
        return Err(
            "retained metadata changed length, was cancelled, or exceeded its deadline".into(),
        );
    }
    Ok(bytes)
}

pub(super) fn conditioning_colour(
    handle: &GeneratedReadHandle,
    receipt: &BundleValidationReceipt,
    cancelled: &AtomicBool,
) -> Option<String> {
    let inputs = receipt.admission()?.inputs();
    let bytes = metadata(
        handle,
        inputs.manifest(),
        MAX_MANIFEST_BYTES,
        Instant::now() + Duration::from_secs(15),
        cancelled,
    )
    .ok()?;
    let digest: String = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != inputs.context_sha256().as_str() {
        return None;
    }
    match receipt.plan() {
        GenerationPlan::Bridge(_) => {
            deadpan_cli::generation::conditioning::ConditioningColour::from_manifest(&bytes)
                .map(|colour| colour.describe())
        }
        GenerationPlan::Extension(plan) => {
            let context: ExtensionContext = serde_json::from_slice(&bytes).ok()?;
            if context.plan() != plan {
                return None;
            }
            Some(extension_colour(&context))
        }
    }
}

fn extension_colour(context: &ExtensionContext) -> String {
    let mut counts = [0; 4];
    for picture in context.context() {
        let index = match picture.picture.decoded().map(|decoded| decoded.model_input) {
            None => 0,
            Some(ModelInputConversion::SrgbCodesUnchanged) => 1,
            Some(ModelInputConversion::Rec709CodesAsSrgb) => 2,
            Some(ModelInputConversion::Rec709ToSrgb) => 3,
        };
        counts[index] += 1;
    }
    let opposite = match context.opposite() {
        deadpan_models::ExtensionOppositeSeam::Absent => "No opposite neighbor.",
        deadpan_models::ExtensionOppositeSeam::PresentUnconditioned { .. } => {
            "The opposite seam is unconditioned."
        }
    };
    format!("{} {opposite}", colour_counts(counts))
}

fn colour_counts(counts: [usize; 4]) -> String {
    let descriptions = counts
        .into_iter()
        .zip([
            "authored black",
            "sRGB unchanged",
            "BT.709 read as sRGB",
            "BT.709 converted to sRGB",
        ])
        .filter(|(count, _)| *count != 0)
        .map(|(count, label)| format!("{count} {label}"))
        .collect::<Vec<_>>()
        .join(", ");
    let approximation = if counts[2] > 0 {
        " Includes an older transfer approximation."
    } else {
        ""
    };
    format!("Extension conditioning colour: {descriptions}.{approximation}")
}

pub(super) fn quality_status(
    handle: &GeneratedReadHandle,
    receipt: &BundleValidationReceipt,
    facts: &VariantFacts,
    cancelled: &AtomicBool,
) -> QualityReading {
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = (|| {
        let bytes = metadata(
            handle,
            receipt.provenance_object(),
            MAX_PROVENANCE_BYTES,
            deadline,
            cancelled,
        )?;
        match receipt.plan() {
            GenerationPlan::Bridge(_) => {
                StoredBridgeProvenance::from_bytes(&bytes, receipt.provenance_object())
                    .map(bridge_quality)
                    .map_err(|error| error.to_string())
            }
            GenerationPlan::Extension(_) => {
                let stored =
                    StoredExtensionProvenance::from_bytes(&bytes, receipt.provenance_object())
                        .map_err(|error| error.to_string())?;
                let binding = stored.binding().clone();
                validate_request_facts(&binding, receipt.plan(), receipt.provider(), facts)?;
                let context = metadata(
                    handle,
                    stored.context_object(),
                    MAX_MANIFEST_BYTES,
                    deadline,
                    cancelled,
                )?;
                let evidence = stored
                    .validate_for(&binding, &context)
                    .map_err(|error| error.to_string())?;
                validate_receipt_facts(&evidence, receipt)?;
                if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err("reading was cancelled or timed out".into());
                }
                Ok(extension_quality(&evidence))
            }
        }
    })();
    result.unwrap_or_else(|error| {
        QualityReading::unavailable(format!("Motion/lighting report unavailable: {error}."))
    })
}

fn validate_request_facts(
    binding: &ExtensionGenerationBinding,
    plan: &GenerationPlan,
    provider: &deadpan_jobs::ProviderSelection,
    facts: &VariantFacts,
) -> Result<(), String> {
    let GenerationPlan::Extension(plan) = plan else {
        return Err("Extension evidence cannot summarize a Bridge variant".into());
    };
    if binding.project_id != facts.project
        || binding.identity != facts.identity
        || binding.revision_id != facts.origin
        || binding.target.hold_id != facts.hold
        || plan != &binding.plan
        || provider != &binding.provider
    {
        return Err("retained Extension evidence differs from this variant's request".into());
    }
    Ok(())
}

fn validate_receipt_facts(
    evidence: &ValidatedExtensionEvidence,
    receipt: &BundleValidationReceipt,
) -> Result<(), String> {
    let admission = receipt
        .admission()
        .ok_or("Extension admission inputs are absent")?;
    let inputs = admission.inputs();
    let conditioning = evidence.conditioning();
    let objects = std::iter::once(conditioning.manifest())
        .chain(conditioning.context())
        .chain(conditioning.opposite())
        .chain(std::iter::once(conditioning.signatures()))
        .map(|input| input.object());
    let contract_matches = |actual: deadpan_media::protocol::VideoContract,
                            expected: &VideoSpec| {
        i64::from(actual.frames) == expected.frames().frames()
            && actual.width == expected.width()
            && actual.height == expected.height()
            && actual.rate_num == expected.frame_rate().numerator()
            && actual.rate_den == expected.frame_rate().denominator()
    };
    if evidence.native_object() != receipt.native_object()
        || evidence.sampled_object() != receipt.sampled_object()
        || evidence.provenance_object() != receipt.provenance_object()
        || evidence.native_span() != admission.native_span()
        || evidence.sampled_span() != admission.sampled_span()
        || !contract_matches(evidence.native_contract(), receipt.native_video())
        || !contract_matches(evidence.sampled_contract(), receipt.sampled_video())
        || inputs.context_sha256() != conditioning.manifest().declaration().sha256()
        || !inputs.objects().eq(objects)
    {
        return Err("retained Extension evidence differs from its Ready receipt".into());
    }
    Ok(())
}

fn extension_quality(evidence: &ValidatedExtensionEvidence) -> QualityReading {
    let motion = evidence.pixels().motion();
    extension_summary(ExtensionSummary {
        total: motion.transitions().len(),
        unavailable: motion.unavailable_motion_pairs(),
        entry: evidence.pixels().endpoints().entry(),
        exit: evidence.pixels().endpoints().exit(),
        geometry: evidence.geometry().geometry(),
        region: evidence.geometry().region(),
        region_unavailable: evidence.geometry().region_unavailable_reason(),
    })
}

struct ExtensionSummary<'a> {
    total: usize,
    unavailable: usize,
    entry: &'a ExtensionEndpointJoin,
    exit: &'a ExtensionEndpointJoin,
    geometry: &'a deadpan_analysis::generated_geometry::extension::ExtensionGeometryReport,
    region: Option<&'a deadpan_analysis::generated_region::extension::ExtensionRegionReport>,
    region_unavailable: Option<&'a str>,
}

fn extension_summary(summary: ExtensionSummary<'_>) -> QualityReading {
    let ExtensionSummary {
        total,
        unavailable,
        entry,
        exit,
        geometry,
        region,
        region_unavailable,
    } = summary;
    let faces = &geometry.geometry;
    let mouth = &geometry.mouth;
    let mut detail = format!(
        "Extension motion/lighting checked across {total} adjacent generated-picture pairs; motion unavailable in {unavailable}. Conditioning context is excluded from these pairs. Audition before accepting."
    );
    detail.push_str(&format!(
        " {} {}",
        endpoint_summary("Entry", entry),
        endpoint_summary("Exit", exit)
    ));
    if faces.measured_tracks == 0 {
        detail.push_str(
            " Face geometry unavailable: no reliable face track from the retained anchor.",
        );
    } else {
        detail.push_str(&format!(" Face geometry from one retained anchor: {} measured tracks, {} unavailable; eye/nose landmarks unavailable on {} tracks.", faces.measured_tracks, faces.unavailable_tracks, faces.feature_unavailable_tracks));
    }
    if mouth.measured_tracks == 0 {
        detail.push_str(
            " Mouth motion unavailable: no reliable eye and lip track in generated pictures.",
        );
    } else {
        detail.push_str(&format!(" Mouth motion within generated pictures: {} measured continuous segments, {} unavailable.", mouth.measured_tracks, mouth.unavailable_tracks));
    }
    if let Some(region) = region {
        detail.push_str(&format!(" Selected-region coverage from the retained anchor: {} measured of {} generated pictures; {} unavailable.", region.measured_frames, region.generated_frames, region.unavailable_frames));
    } else {
        detail.push_str(&format!(
            " Selected-region check unavailable: {}.",
            region_unavailable.unwrap_or("no reliable authored region")
        ));
    }
    QualityReading {
        compact: format!(
            "Motion coverage {}/{total}",
            total.saturating_sub(unavailable)
        ),
        detail,
    }
}

fn endpoint_summary(name: &str, join: &ExtensionEndpointJoin) -> String {
    match join {
        ExtensionEndpointJoin::Absent {} => format!("{name}: no neighbor; join absent."),
        ExtensionEndpointJoin::Measured { role, .. } => format!(
            "{name}: {} join checked for gross discontinuity.",
            match role {
                ExtensionJoinRole::Conditioned => "conditioned",
                ExtensionJoinRole::Unconditioned => "unconditioned opposite",
            }
        ),
    }
}

fn bridge_quality(provenance: deadpan_models::StoredBridgeProvenance) -> QualityReading {
    let Some(report) = provenance.quality() else {
        return QualityReading::unavailable(
            "Older candidate: motion/lighting, endpoint, face, mouth and selected-region checks unavailable.",
        );
    };
    let unavailable = report.unavailable_motion_pairs();
    let total = report.transitions().len();
    let measurable = total.saturating_sub(unavailable);
    let mut detail: String = if unavailable == 0 {
        "Motion/lighting sampled; audition before accepting. Motion uses bounded block matching and may cover only part of each frame.".into()
    } else {
        format!(
            "Motion/lighting sampled; audition before accepting. Motion unavailable in {unavailable} of {total} frame pairs. Motion uses bounded block matching and may cover only part of each frame."
        )
    };
    detail.push_str(if provenance.endpoints().is_some() {
        " Both edit joins checked for gross discontinuity."
    } else {
        " Endpoint checks unavailable for this older candidate."
    });
    if let Some(geometry) = provenance.geometry() {
        let assessment = geometry.assessment();
        let faces = &assessment.geometry;
        let mouth = &assessment.mouth;
        if faces.measured_tracks == 0 {
            detail.push_str(
                " Face geometry unavailable: no reliable track connects both input pictures.",
            );
        } else {
            detail.push_str(&format!(" Face geometry: {} measured tracks, {} unavailable; eye/nose landmarks unavailable on {} tracks.", faces.measured_tracks, faces.unavailable_tracks, faces.feature_unavailable_tracks));
        }
        if mouth.measured_tracks == 0 {
            detail.push_str(" Mouth motion unavailable: no reliable eye and lip track.");
        } else {
            detail.push_str(&format!(
                " Mouth motion: {} measured continuous segments, {} unavailable.",
                mouth.measured_tracks, mouth.unavailable_tracks
            ));
        }
    } else {
        detail.push_str(" Face and mouth checks unavailable for this older candidate.");
    }
    if let Some(region) = provenance.region() {
        match region.assessment() {
            Some(assessment) => {
                detail.push_str(&format!(
                    " Selected-region coverage: {} measured of {} native frames; {} unavailable.",
                    assessment.measured_frames,
                    assessment.native_frames,
                    assessment.unavailable_frames
                ));
                if assessment.measured_frames == 0 {
                    detail.push_str(
                        " Selected-region drift unavailable: no reliable continuous track.",
                    );
                }
                detail.push_str(&format!(
                    " Region endpoints: entry {}, exit {}.",
                    if assessment.left_boundary.measured {
                        "measured"
                    } else {
                        "unavailable"
                    },
                    if assessment.right_boundary.measured {
                        "measured"
                    } else {
                        "unavailable"
                    }
                ));
                if let (Some(center), Some(size)) = (
                    assessment.maximum_center_residual,
                    assessment.maximum_log_size_residual,
                ) {
                    detail.push_str(&format!(" Maximum center deviation {:.1}% of the picture diagonal; maximum size/aspect ratio {:.2}×.", center * 100.0, size.exp()));
                }
                if !assessment.unavailable_reasons.is_empty() {
                    let reasons = assessment
                        .unavailable_reasons
                        .iter()
                        .map(|reason| {
                            use deadpan_analysis::generated_region::RegionUnavailableReason;
                            match reason {
                                RegionUnavailableReason::Missing => "missing observation",
                                RegionUnavailableReason::InvalidGeometry => {
                                    "invalid measured rectangle"
                                }
                                RegionUnavailableReason::LowConfidence => "low tracking confidence",
                                RegionUnavailableReason::LostTrack => "tracking lost",
                                RegionUnavailableReason::Unsupported => {
                                    "unsupported tracking input"
                                }
                                RegionUnavailableReason::OutsidePresentation => {
                                    "subject outside the picture"
                                }
                                RegionUnavailableReason::BoundarySeedMismatch => {
                                    "endpoint differs from the saved target"
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    detail.push_str(&format!(" Region coverage limits: {reasons}."));
                }
            }
            None => detail.push_str(&format!(
                " Selected-region check unavailable: {}.",
                region
                    .unavailable_reason()
                    .unwrap_or("no reliable authored region")
            )),
        }
    } else {
        detail.push_str(" Selected-region check unavailable for this older candidate.");
    }
    QualityReading {
        compact: format!("Motion coverage {measurable}/{total}"),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_analysis::generated_extension::ExtensionCoverage;
    use deadpan_analysis::generated_geometry::extension::{
        self as face, RawExtensionLandmarkBatch,
    };
    use deadpan_analysis::generated_geometry::{FaceObservationSet, FrameObservation};
    use deadpan_core::{ExactRatio, ExtensionDirection, GeneratedContentId};

    fn binding() -> ExtensionGenerationBinding {
        // Only the genuine adapter's request binding is used here. Its older
        // context is not presented as current qualification evidence.
        let value: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/model-qualification/evidence/2026-10-07-extension-worker/from_left/workspace/outputs/provenance.json"
        ))).unwrap();
        serde_json::from_value(value["request_binding"].clone()).unwrap()
    }

    #[test]
    fn extension_quality_stays_bound_to_the_captured_variant_request() {
        let binding = binding();
        let facts = || VariantFacts {
            project: binding.project_id.clone(),
            identity: binding.identity.clone(),
            origin: binding.revision_id.clone(),
            hold: binding.target.hold_id.clone(),
        };
        let plan = GenerationPlan::Extension(binding.plan.clone());
        validate_request_facts(&binding, &plan, &binding.provider, &facts()).unwrap();
        for field in ["project", "request", "attempt", "revision", "hold"] {
            let mut changed = facts();
            match field {
                "project" => changed.project = ProjectId::new("another-project").unwrap(),
                "request" => {
                    changed.identity.request_id =
                        deadpan_jobs::RequestId::new("another-request").unwrap()
                }
                "attempt" => {
                    changed.identity.attempt_id =
                        deadpan_jobs::AttemptId::new("another-attempt").unwrap()
                }
                "revision" => changed.origin = RevisionId::new("another-revision").unwrap(),
                "hold" => changed.hold = NodeId::new("another-hold").unwrap(),
                _ => unreachable!(),
            }
            assert!(
                validate_request_facts(&binding, &plan, &binding.provider, &changed).is_err(),
                "{field}"
            );
        }
        let mut provider = binding.provider.clone();
        provider.seed += 1;
        assert!(validate_request_facts(&binding, &plan, &provider, &facts()).is_err());
        let mut wrong = serde_json::to_value(&binding.plan).unwrap();
        wrong["sampling"]["direction"] = serde_json::json!("from_right");
        let plan = GenerationPlan::Extension(serde_json::from_value(wrong).unwrap());
        assert!(validate_request_facts(&binding, &plan, &binding.provider, &facts()).is_err());
    }

    fn endpoint(role: ExtensionJoinRole) -> ExtensionEndpointJoin {
        let binding = binding();
        ExtensionEndpointJoin::Measured {
            role,
            picture: Box::new(deadpan_models::BoundaryPicture::AuthoredBlack {
                clock: deadpan_models::BoundaryClock::Definition {
                    project_id: binding.project_id,
                    revision_id: binding.revision_id,
                    definition: binding.target.hold_id,
                    position: ExactRatio::ZERO,
                },
            }),
            object: GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 1)
                .unwrap(),
            content: None,
            endpoint: deadpan_models::EndpointObservation {
                sampled_frame: 0,
                sampled_pts: 0,
                mean_absolute_rgb_difference: 0.0,
                gross_cell_fraction: 0.0,
            },
            quality: deadpan_models::FrameObservation {
                after_frame: 0,
                mean_luma_shift: 0.0,
                mean_absolute_luma_change: 0.0,
                lighting_agreement_fraction: 1.0,
                textured_blocks: 0,
                matched_blocks: 0,
                motion: deadpan_models::MotionObservation::Unavailable,
            },
        }
    }

    #[test]
    fn extension_rows_show_one_anchor_and_only_present_join_roles() {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let coverage = match direction {
                ExtensionDirection::FromLeft => ExtensionCoverage {
                    direction,
                    start: 9,
                    end: 17,
                },
                ExtensionDirection::FromRight => ExtensionCoverage {
                    direction,
                    start: 0,
                    end: 8,
                },
            };
            let pts: Vec<_> = (0..17).map(|n| n * 42).collect();
            let raw = RawExtensionLandmarkBatch {
                schema_version: 1,
                coverage,
                anchor: FaceObservationSet::Detected { faces: vec![] },
                frames: coverage
                    .ordinals()
                    .map(|ordinal| FrameObservation {
                        ordinal,
                        pts: pts[ordinal as usize],
                        observation: FaceObservationSet::Detected { faces: vec![] },
                    })
                    .collect(),
            };
            let geometry = face::analyze(&raw, &pts, [64, 36], [0, 0, 64, 36], false).unwrap();
            for opposite in [false, true] {
                let conditioned = endpoint(ExtensionJoinRole::Conditioned);
                let unconditioned = if opposite {
                    endpoint(ExtensionJoinRole::Unconditioned)
                } else {
                    ExtensionEndpointJoin::Absent {}
                };
                let (entry, exit) = if direction == ExtensionDirection::FromLeft {
                    (&conditioned, &unconditioned)
                } else {
                    (&unconditioned, &conditioned)
                };
                let reading = extension_summary(ExtensionSummary {
                    total: 7,
                    unavailable: 7,
                    entry,
                    exit,
                    geometry: &geometry,
                    region: None,
                    region_unavailable: Some("no selected region target"),
                });
                assert_eq!(reading.compact, "Motion coverage 0/7");
                assert!(
                    reading
                        .detail
                        .contains("7 adjacent generated-picture pairs")
                );
                assert!(reading.detail.contains("retained anchor"));
                assert!(reading.detail.contains("conditioned join checked"));
                assert_eq!(
                    reading
                        .detail
                        .contains("unconditioned opposite join checked"),
                    opposite
                );
                assert_eq!(
                    reading.detail.contains("no neighbor; join absent"),
                    !opposite
                );
                assert!(!reading.detail.contains("both input pictures"));
                assert!(!reading.detail.contains("Both edit joins"));
                assert!(!reading.detail.contains("older candidate"));
                assert!(!reading.detail.contains("provenance is invalid"));
            }
        }
    }

    #[test]
    fn extension_colour_summary_counts_the_temporal_context_without_two_side_assumptions() {
        assert_eq!(
            colour_counts([9, 0, 0, 0]),
            "Extension conditioning colour: 9 authored black."
        );
        let mixed = colour_counts([1, 3, 2, 3]);
        assert!(mixed.contains("3 sRGB unchanged"));
        assert!(mixed.contains("2 BT.709 read as sRGB"));
        assert!(mixed.contains("3 BT.709 converted to sRGB"));
        assert!(mixed.contains("older transfer approximation"));
        assert!(!colour_counts([0, 0, 0, 9]).contains("approximation"));
    }
}
