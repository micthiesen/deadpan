//! Decoder-free evaluation of semantic framing against a staged document.
//!
//! The render plan owns the evaluated pose and clock. Qualified SQLite metadata
//! supplies provider geometry; this path never opens source or generated media,
//! creates a decoder, selects a proxy, or allocates a picture raster.

use deadpan_core::{
    AssetId, EditError, EditErrorCode, ExactRatio, GeneratedArtifact, GeneratedObjectRef,
    GeneratedSamplingMap, ProjectDocument, SemanticFramingSite, SemanticFramingState,
    SourceTimeBase, SourceTimestamp,
};
use deadpan_jobs::GenerationPlan;
use deadpan_media::protocol::VideoContract;
use deadpan_plan::{Picture, RenderPlan};
use deadpan_render::{
    FitMode, FrameMetadata, PictureGeometry, Primaries, Rotation, SampleAspectRatio, SourceColor,
    Transfer,
};
use deadpan_store::ProjectStore;

pub(crate) fn resolve(
    store: &ProjectStore,
    staged: &ProjectDocument,
    site: &SemanticFramingSite,
) -> Result<SemanticFramingState, EditError> {
    let plan = RenderPlan::compile(staged).map_err(unavailable)?;
    let sample = plan.picture(site.frame).map_err(unavailable)?;
    let index = sample
        .framing
        .iter()
        .position(|layer| {
            !layer.escalation
                && layer.instance.node == site.node
                && layer.instance.repeats.is_empty()
        })
        .ok_or_else(|| unavailable("move the cursor inside the selected beat to frame it"))?;
    // Follow evaluation belongs to the plan, which retains its exact live and
    // authored clocks. Framing::evaluate_exact would return Follow's fallback.
    let current = sample.framing[index].pose.unwrap_or_default();
    let metadata = provider_metadata(store, staged, &sample.picture)?;
    let original_center = if site.original_center {
        let layers = crate::picture::render_layers(&sample.framing, sample.gap_after.is_some())
            .map_err(unavailable)?;
        let basis = staged.presentation_basis();
        let canvas = [basis.width, basis.height];
        let geometry = PictureGeometry::composed(
            &metadata,
            sample.picture_context.as_deref(),
            canvas,
            canvas,
            FitMode::Fit,
            &layers,
        )
        .map_err(unavailable)?;
        let center = geometry
            .source_to_input(index + usize::from(sample.gap_after.is_some()), [0.5, 0.5])
            .map_err(unavailable)?
            .ok_or_else(|| {
                unavailable("the Original picture center is clipped before this beat")
            })?;
        Some([projected_ratio(center[0])?, projected_ratio(center[1])?])
    } else {
        None
    };
    Ok(SemanticFramingState {
        current,
        original_center,
    })
}

fn provider_metadata(
    store: &ProjectStore,
    document: &ProjectDocument,
    picture: &Picture,
) -> Result<FrameMetadata, EditError> {
    match picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            let record = document
                .assets()
                .get(asset)
                .ok_or_else(|| unavailable("the picture provider asset is absent"))?;
            let qualification = record
                .source_qualification
                .as_ref()
                .ok_or_else(|| unavailable("the picture provider has no saved qualification"))?;
            let receipt = store
                .source_qualification(qualification)
                .map_err(unavailable)?;
            if receipt
                .asset_record(record.label.clone())
                .map_err(unavailable)?
                != *record
            {
                return Err(unavailable(
                    "the picture provider differs from its saved qualification",
                ));
            }
            let video = receipt
                .snapshot()
                .video()
                .ok_or_else(|| unavailable("the picture provider has no qualified video"))?;
            let index = video.index().index();
            // The receipt's authored alias may predate a copied provider alias.
            // Rebind only this small request; measured PTS/index bytes stay put.
            let mut request = picture.clone();
            if let Picture::Source { asset, .. } | Picture::Freeze { asset, .. } = &mut request {
                *asset = index.asset().clone();
            }
            let frame = request.select_source_frame(index).map_err(unavailable)?;
            let info = video.interpretation();
            let stride = info
                .width
                .checked_mul(4)
                .ok_or_else(|| unavailable("the picture row stride exceeds its bound"))?;
            crate::picture::source_frame_metadata(info, frame.pts, stride).map_err(unavailable)
        }
        Picture::Accepted {
            asset,
            generated: Some(artifact),
            time_base,
            frame,
            ..
        } => {
            if asset != &artifact.sampled_asset {
                return Err(unavailable(
                    "the accepted picture does not name its sampled master",
                ));
            }
            let origin = store
                .accepted_generation_origin(artifact)
                .map_err(unavailable)?
                .ok_or_else(|| {
                    unavailable("the accepted picture has no retained origin receipt")
                })?;
            let accepted = store
                .snapshot_at(origin.accepted_revision())
                .map_err(unavailable)?;
            generated_metadata(
                document,
                &accepted,
                origin.plan(),
                artifact,
                *time_base,
                frame.0,
            )
        }
        Picture::Accepted {
            generated: None, ..
        } => Err(unavailable(
            "this legacy accepted picture has no qualified generated contract",
        )),
        Picture::Still { .. } => Err(unavailable("still-image framing geometry is not qualified")),
        Picture::Blank | Picture::Background => {
            Err(unavailable("this beat has no picture to frame"))
        }
    }
}

fn generated_metadata(
    document: &ProjectDocument,
    accepted: &ProjectDocument,
    plan: &GenerationPlan,
    artifact: &GeneratedArtifact,
    time_base: SourceTimeBase,
    ordinal: u64,
) -> Result<FrameMetadata, EditError> {
    let sampling = match plan {
        GenerationPlan::Bridge(plan) => {
            GeneratedSamplingMap::Bridge(plan.sampling_map().map_err(unavailable)?)
        }
        GenerationPlan::Extension(plan) => {
            GeneratedSamplingMap::Extension(plan.sampling_map().clone())
        }
    };
    if sampling != artifact.sampling {
        return Err(unavailable(
            "the accepted sampling differs from its retained native plan",
        ));
    }
    let dimensions = plan.native_dimensions();
    let contract = |frames, rate: deadpan_core::FrameRate| VideoContract {
        width: dimensions.width(),
        height: dimensions.height(),
        frames,
        rate_num: rate.numerator(),
        rate_den: rate.denominator(),
    };
    let native = contract(
        u32::try_from(sampling.native_frame_count().frames()).map_err(unavailable)?,
        sampling.native_rate(),
    );
    let sampled = contract(
        u32::try_from(sampling.output_frame_count().frames()).map_err(unavailable)?,
        sampling.project_rate(),
    );
    native.validate().map_err(unavailable)?;
    sampled.validate().map_err(unavailable)?;
    validate_generated_asset(
        document,
        accepted,
        &artifact.native_asset,
        &artifact.native_object,
        native,
    )?;
    validate_generated_asset(
        document,
        accepted,
        &artifact.sampled_asset,
        &artifact.sampled_object,
        sampled,
    )?;
    if time_base != SourceTimeBase::new(1, 1000).map_err(unavailable)? {
        return Err(unavailable(
            "the accepted picture has a different retained media clock",
        ));
    }
    let pts = sampled
        .matroska_pts(u32::try_from(ordinal).map_err(unavailable)?)
        .map_err(unavailable)?;
    // Every admitted generated master has this independently verified FFV1
    // interpretation. Raster and exact sampled PTS come from its retained plan.
    let metadata = FrameMetadata {
        width: sampled.width,
        height: sampled.height,
        row_stride_bytes: sampled
            .width
            .checked_mul(4)
            .ok_or_else(|| unavailable("generated row stride overflow"))?,
        clean_aperture: None,
        sample_aspect_ratio: SampleAspectRatio::new(1, 1).map_err(unavailable)?,
        rotation: Rotation::None,
        color: SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709,
        },
        pts: SourceTimestamp {
            ticks: pts,
            time_base,
        },
    };
    Ok(artifact.content_aspect.map_or(metadata, |aspect| {
        crate::picture::fill_canvas_aspect_metadata(metadata, aspect)
    }))
}

fn validate_generated_asset(
    document: &ProjectDocument,
    accepted: &ProjectDocument,
    asset: &AssetId,
    object: &GeneratedObjectRef,
    contract: VideoContract,
) -> Result<(), EditError> {
    let record = document
        .assets()
        .get(asset)
        .ok_or_else(|| unavailable("the accepted asset is absent"))?;
    let original = accepted
        .assets()
        .get(asset)
        .ok_or_else(|| unavailable("the accepted origin asset is absent"))?;
    let mut expected = original.clone();
    expected.label.clone_from(&record.label);
    let span = record
        .video
        .ok_or_else(|| unavailable("the accepted video span is absent"))?;
    if *record != expected
        || record.source_qualification.is_some()
        || record.content_hash != object.content().to_string()
        || record.audio.is_some()
        || record.still_image
        || record.frame_count.map(|count| count.frames()) != Some(i64::from(contract.frames))
        || span.start().time_base != SourceTimeBase::new(1, 1000).map_err(unavailable)?
        || span.start().ticks != 0
        || contract
            .matroska_pts(contract.frames - 1)
            .map_err(unavailable)?
            >= span.end().ticks
    {
        return Err(unavailable(
            "the accepted asset differs from its retained admission",
        ));
    }
    Ok(())
}

fn projected_ratio(value: f64) -> Result<ExactRatio, EditError> {
    // Match Camera's declared f64 spatial boundary and single Q32 round-even
    // quantization. Timing and layer evaluation never pass through this grid.
    if !value.is_finite() || value.abs() > 1_000_000_000.0 {
        return Err(unavailable(
            "this source projection exceeds Camera's numeric range",
        ));
    }
    ExactRatio::new(
        (value * deadpan_core::FRAMING_NUMERIC_SCALE as f64).round_ties_even() as i128,
        i128::from(deadpan_core::FRAMING_NUMERIC_SCALE),
    )
    .map_err(unavailable)
}

fn unavailable(message: impl std::fmt::Display) -> EditError {
    EditError {
        code: EditErrorCode::SelectionUnavailable,
        message: format!("the framing picture could not be resolved: {message}"),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests;
