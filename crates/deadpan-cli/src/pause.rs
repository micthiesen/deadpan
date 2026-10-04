//! The frozen picture a new pause shows: the picture before its boundary,
//! with the composition below the pause's own Sequence captured. Shared by the
//! native `,h` edit and macro pauses so both author the same Hold.

use std::sync::Arc;

use deadpan_core::{
    AssetId, CapturedCanvas, CapturedFit, CapturedFraming, FrameDuration, HoldVideo, PauseProvider,
    ProjectDocument, ProjectFrame, SourceFrameIndex, SourceTimestamp,
};
use deadpan_plan::{Picture, RenderPlan};

/// Resolve the provider for a pause at `at` in `document`, compiled as `plan`.
/// `index` supplies the measured picture index of a shown asset.
pub fn pause_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    index: &mut dyn FnMut(&AssetId) -> Result<Arc<SourceFrameIndex>, String>,
) -> Result<PauseProvider, String> {
    if plan.duration() == FrameDuration::ZERO {
        return Ok(PauseProvider {
            video: HoldVideo::Background,
            picture_context: None,
        });
    }
    let insertion_parent = document
        .insert_time_target(at)
        .map_err(|error| error.to_string())?
        .parent;
    let sample = plan
        .picture(ProjectFrame(if at.0 == 0 { 0 } else { at.0 - 1 }))
        .map_err(|error| error.to_string())?;
    let picture = &sample.picture;
    match picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            let index = index(asset)?;
            let selected = picture
                .select_source_frame(&index)
                .map_err(|error| error.to_string())?;
            // The new Hold is a child of the selected Sequence. Retain only
            // composition below that parent. The parent and its ancestors
            // stay live on the Hold and must not be captured a second time.
            let parent = sample
                .framing
                .iter()
                .position(|scope| {
                    scope.instance.node == insertion_parent && scope.instance.repeats.is_empty()
                })
                .ok_or("The stopped picture has no selected Sequence scope.")?;
            let lower = &sample.framing[..parent];
            // Even an unframed view retains its canvas and letterboxing. Fitting
            // the raw source directly into a later canvas is not equivalent to
            // fitting the already composed view into that canvas.
            let mut layers =
                Vec::with_capacity(lower.len() + usize::from(sample.gap_after.is_some()));
            if sample.gap_after.is_some() {
                layers.push(None);
            }
            layers.extend(lower.iter().map(|layer| layer.pose));
            let basis = document.presentation_basis();
            let picture_context = Some(
                CapturedFraming::capture(
                    sample.picture_context.as_deref(),
                    CapturedCanvas {
                        width: basis.width,
                        height: basis.height,
                        fit: CapturedFit::Fit,
                        layers,
                    },
                )
                .map_err(|error| error.to_string())?,
            );
            Ok(PauseProvider {
                video: HoldVideo::Freeze {
                    asset: asset.clone(),
                    timestamp: SourceTimestamp {
                        ticks: selected.pts,
                        time_base: index.time_base(),
                    },
                },
                picture_context,
            })
        }
        Picture::Blank | Picture::Background => Ok(PauseProvider {
            video: HoldVideo::Background,
            picture_context: None,
        }),
        Picture::Still { .. } | Picture::Accepted { .. } => Err(
            "Freezing still or accepted generated footage for a new pause is not available yet."
                .into(),
        ),
    }
}
