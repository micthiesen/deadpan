//! Resolve deterministic pause intent from one immutable native workspace.

use deadpan_core::{
    AudioTimingId, CapturedCanvas, CapturedFit, CapturedFraming, Command, CommandRequest,
    FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, ProjectFrame, RevisionId,
    SourceTimestamp, SplitIdentities,
};
use deadpan_plan::Picture;

use super::Workspace;

pub(super) fn prepare(
    workspace: &Workspace,
    at: ProjectFrame,
    duration: FrameDuration,
    new_revision: RevisionId,
    id: NodeId,
    mut allocate: impl FnMut() -> NodeId,
) -> Result<CommandRequest, String> {
    let document = &workspace.document;
    let total = workspace.plan.duration().frames();
    if at.0 < 0 || at.0 > total {
        return Err("Pause boundary is outside the sequence.".into());
    }
    if duration == FrameDuration::ZERO {
        return Err("Pause resolves to 0 frames; no edit was made.".into());
    }
    // Resolve support before sampling a fallback. This keeps native admission
    // aligned with the core command and avoids preparing a picture for a
    // Repeat or authored Retime interior that core will refuse.
    let target = document
        .insert_time_target(at)
        .map_err(|error| error.to_string())?;
    let (video, picture_context) = fallback(workspace, at, &target.parent)?;
    let identities = target
        .split
        .map(|split| (0..split.required_ids).map(|_| allocate()).collect())
        .unwrap_or_default();
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        command: Command::InsertTime {
            at,
            hold: HoldRecipe {
                duration,
                video,
                audio: HoldAudio::Silence,
                picture_context,
            },
            id,
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: new_revision.clone(),
                ordinal: 0,
            },
        },
        new_revision,
    })
}

fn fallback(
    workspace: &Workspace,
    at: ProjectFrame,
    insertion_parent: &NodeId,
) -> Result<(HoldVideo, Option<CapturedFraming>), String> {
    if workspace.plan.duration() == FrameDuration::ZERO {
        return Ok((HoldVideo::Background, None));
    }
    let sample = workspace
        .plan
        .picture(ProjectFrame(if at.0 == 0 { 0 } else { at.0 - 1 }))
        .map_err(|error| error.to_string())?;
    let picture = &sample.picture;
    match picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            let index = workspace
                .sources
                .get(asset)
                .and_then(|source| source.video_index.as_ref())
                .ok_or("Pause needs the registered original's measured video index.")?;
            let selected = picture
                .select_source_frame(index)
                .map_err(|error| error.to_string())?;
            // The new Hold is a child of the selected Sequence. Retain only
            // composition below that parent. The parent and its ancestors
            // stay live on the Hold and must not be captured a second time.
            let parent = sample
                .framing
                .iter()
                .position(|scope| {
                    scope.instance.node == *insertion_parent && scope.instance.repeats.is_empty()
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
            let basis = workspace.document.presentation_basis();
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
            Ok((
                HoldVideo::Freeze {
                    asset: asset.clone(),
                    timestamp: SourceTimestamp {
                        ticks: selected.pts,
                        time_base: index.time_base(),
                    },
                },
                picture_context,
            ))
        }
        Picture::Blank | Picture::Background => Ok((HoldVideo::Background, None)),
        Picture::Still { .. } | Picture::Accepted { .. } => Err(
            "Freezing still or accepted generated footage for a new pause is not available yet."
                .into(),
        ),
    }
}
