//! Cold admission shared by native preview and committed project rendering.

use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, GeneratedArtifact, GeneratedObjectRef, ProjectDocument, SourceSpan};
use deadpan_media::protocol::VideoContract;
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_models::StoredBridgeProvenance;
use deadpan_source::{ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer};
use deadpan_store::generated_media::{GeneratedReadHandle, GeneratedReadLimits};

use super::{ProjectPictureError, check_cancel};

const OPEN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_MEDIA_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_PROVENANCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CONTEXT_BYTES: u64 = 1024 * 1024;
const MAX_CONDITIONING_BYTES: u64 = 64 * 1024 * 1024;

/// Admit the sampled master of an already accepted Generated Hold. All six
/// retained objects are required on a cold read. No worker path, mutable
/// candidate selection, request relevance or installed model is consulted.
/// The caller must release its previous decoder before calling this and keep
/// the resulting decoder keyed by the complete artifact, both asset records,
/// color policy and project session. Revision and framing changes alone do not
/// invalidate those immutable media inputs.
pub fn open_generated_picture(
    handle: &GeneratedReadHandle,
    document: &ProjectDocument,
    artifact: &GeneratedArtifact,
    cancelled: &AtomicBool,
) -> Result<SourceSession, ProjectPictureError> {
    // Generated footage is always SDR sRGB. In an HDR-basis project its
    // presence makes the automatic branch SDR (see `decide_output_color`).
    let deadline = Instant::now() + OPEN_TIMEOUT;
    let remaining = || {
        check_cancel(cancelled)?;
        handle.check_live(cancelled)?;
        deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or(ProjectPictureError::Deadline)
    };
    let read = |reference: &GeneratedObjectRef, maximum: u64| {
        let limits = GeneratedReadLimits::new(maximum, remaining()?)?;
        let snapshot = handle.snapshot(reference, limits, cancelled)?;
        remaining()?;
        Ok::<_, ProjectPictureError>(snapshot)
    };
    let bytes = |reference: &GeneratedObjectRef, maximum: u64| {
        let mut snapshot = read(reference, maximum)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(
                usize::try_from(reference.byte_length())
                    .map_err(|_| ProjectPictureError::Limits("generated metadata allocation"))?,
            )
            .map_err(|_| ProjectPictureError::Limits("generated metadata allocation"))?;
        snapshot
            .by_ref()
            .take(maximum + 1)
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).ok() != Some(reference.byte_length()) {
            return Err(invalid("generated metadata length changed"));
        }
        remaining()?;
        Ok::<_, ProjectPictureError>(bytes)
    };
    let provenance = bytes(&artifact.provenance, MAX_PROVENANCE_BYTES)?;
    let stored = StoredBridgeProvenance::from_bytes(&provenance, &artifact.provenance)?;
    let context = bytes(stored.context_object(), MAX_CONTEXT_BYTES)?;
    let evidence = stored.validate_for(artifact, document.project_id(), &context)?;
    remaining()?;
    for (object, sha256) in [
        (evidence.left_object(), evidence.left_sha256()),
        (evidence.right_object(), evidence.right_sha256()),
    ] {
        let snapshot = read(object, MAX_CONDITIONING_BYTES)?;
        let actual: String = snapshot
            .sha256()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual != sha256 {
            return Err(invalid("retained conditioning SHA-256 differs"));
        }
    }
    validate_asset(
        document,
        &artifact.native_asset,
        evidence.native_object(),
        evidence.native_contract(),
        evidence.native_span(),
    )?;
    validate_asset(
        document,
        &artifact.sampled_asset,
        evidence.sampled_object(),
        evidence.sampled_contract(),
        evidence.sampled_span(),
    )?;
    // Verify the retained native dependency, then release its private file
    // before copying/decoding the sampled master. Shared objects need one read.
    if evidence.native_object() != evidence.sampled_object() {
        drop(read(evidence.native_object(), MAX_MEDIA_BYTES)?);
    }
    let mut snapshot = read(evidence.sampled_object(), MAX_MEDIA_BYTES)?;
    let content = SourceContentIdentity::new(snapshot.sha256(), snapshot.reference().byte_length())
        .map_err(|_| invalid("generated input identity is invalid"))?;
    let contract = evidence.sampled_contract();
    let mut limits = SourceSessionLimits::default();
    limits.decode.max_input_bytes = MAX_MEDIA_BYTES;
    limits.maximum_index_frames = usize::try_from(contract.frames)
        .map_err(|_| ProjectPictureError::Limits("generated frame count"))?;
    limits.opening_timeout = remaining()?;
    let source = SourceSession::open_verified(
        &mut snapshot,
        content,
        artifact.sampled_asset.clone(),
        limits,
        cancelled,
    )?;
    remaining()?;
    validate_decoded(&source, contract, evidence.sampled_span())?;
    remaining()?;
    Ok(source)
}

fn invalid(reason: &'static str) -> ProjectPictureError {
    ProjectPictureError::GeneratedEvidence(reason)
}

fn validate_asset(
    document: &ProjectDocument,
    asset: &AssetId,
    object: &GeneratedObjectRef,
    contract: VideoContract,
    span: SourceSpan,
) -> Result<(), ProjectPictureError> {
    let record = document
        .assets()
        .get(asset)
        .ok_or_else(|| invalid("accepted asset is absent"))?;
    if record.source_qualification.is_some()
        || record.content_hash != object.content().to_string()
        || record.video != Some(span)
        || record.audio.is_some()
        || record.still_image
        || record.frame_count.map(|count| count.frames()) != Some(i64::from(contract.frames))
    {
        return Err(invalid(
            "accepted asset disagrees with its retained provenance",
        ));
    }
    Ok(())
}

fn validate_decoded(
    source: &SourceSession,
    contract: VideoContract,
    span: SourceSpan,
) -> Result<(), ProjectPictureError> {
    let info = source.info();
    let index = source.index().index();
    if info.width != contract.width
        || info.height != contract.height
        || info.codec != "ffv1"
        || info.pixel_format != "bgr0"
        || info.stream_index != 0
        || info.time_base_num != 1
        || info.time_base_den != 1000
        || info.sample_aspect_num != 1
        || info.sample_aspect_den != 1
        || info.rotation_quarter_turns != 0
        || !info.audio_streams.is_empty()
        || info.color.range != ColorRange::Full
        || info.color.matrix != ColorMatrix::Rgb
        || info.color.transfer != ColorTransfer::Srgb
        || info.color.primaries != ColorPrimaries::Bt709
        || index.time_base() != span.start().time_base
        || u64::try_from(index.frames().len()).ok() != Some(u64::from(contract.frames))
        || index.terminal_end() != span.end().ticks
        || index
            .frames()
            .first()
            .is_none_or(|frame| frame.pts != span.start().ticks)
    {
        return Err(invalid(
            "decoded accepted interpretation or measured span differs",
        ));
    }
    for (ordinal, frame) in index.frames().iter().enumerate() {
        let ordinal = u32::try_from(ordinal).map_err(|_| invalid("generated ordinal overflow"))?;
        if frame.pts
            != contract
                .matroska_pts(ordinal)
                .map_err(|_| invalid("generated clock overflow"))?
            || frame.identity.0 != u64::from(ordinal)
        {
            return Err(invalid("decoded accepted frame index differs"));
        }
    }
    let last = index
        .frames()
        .last()
        .ok_or_else(|| invalid("accepted picture index is empty"))?;
    if last
        .reported_duration
        .and_then(|duration| last.pts.checked_add(duration))
        != Some(span.end().ticks)
    {
        return Err(invalid(
            "accepted terminal duration is not independently observed",
        ));
    }
    Ok(())
}
