//! Resolve deterministic pause intent from one immutable native workspace.

use deadpan_core::{
    AudioTimingId, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    NodeId, NodeKind, ProjectFrame, RevisionId, SourceTimestamp, SplitIdentities,
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
    let video = fallback(workspace, at)?;
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        return Err("Pause insertion requires a root Sequence.".into());
    };
    let mut start = 0_i64;
    let mut identities = Vec::new();
    for child in children {
        let end = start
            .checked_add(
                workspace
                    .plan
                    .node_duration(child)
                    .ok_or("A sequence beat is missing from the current render plan.")?
                    .frames(),
            )
            .ok_or("Sequence duration overflow")?;
        if start < at.0 && at.0 < end {
            let mut pending = vec![child.clone()];
            let mut count = 3_usize;
            while let Some(id) = pending.pop() {
                count = count.checked_add(1).ok_or("Pause node budget exhausted")?;
                if count > deadpan_core::MAX_DOCUMENT_NODES {
                    return Err("Pause exceeds the document node limit.".into());
                }
                pending.extend(document.children(&id).cloned());
            }
            identities = (0..count).map(|_| allocate()).collect();
            break;
        }
        start = end;
    }
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        command: Command::InsertTime {
            at,
            hold: HoldRecipe {
                duration,
                video,
                audio: HoldAudio::Silence,
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

fn fallback(workspace: &Workspace, at: ProjectFrame) -> Result<HoldVideo, String> {
    if workspace.plan.duration() == FrameDuration::ZERO {
        return Ok(HoldVideo::Background);
    }
    let sample = workspace
        .plan
        .picture(ProjectFrame(if at.0 == 0 { 0 } else { at.0 - 1 }))
        .map_err(|error| error.to_string())?;
    if sample.framing.iter().any(|layer| layer.pose.is_some()) {
        return Err("Freezing an already framed picture needs a retained composition snapshot. Insert the pause before framing it; the current edit was not changed.".into());
    }
    let picture = sample.picture;
    match &picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            let index = workspace
                .sources
                .get(asset)
                .and_then(|source| source.video_index.as_ref())
                .ok_or("Pause needs the registered original's measured video index.")?;
            let selected = picture
                .select_source_frame(index)
                .map_err(|error| error.to_string())?;
            Ok(HoldVideo::Freeze {
                asset: asset.clone(),
                timestamp: SourceTimestamp {
                    ticks: selected.pts,
                    time_base: index.time_base(),
                },
            })
        }
        Picture::Blank | Picture::Background => Ok(HoldVideo::Background),
        Picture::Still { .. } | Picture::Accepted { .. } => Err(
            "Freezing still or accepted generated footage for a new pause is not available yet."
                .into(),
        ),
    }
}
