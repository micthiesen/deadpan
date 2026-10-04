//! Resolve deterministic pause intent from one immutable native workspace.

use deadpan_core::{
    AudioTimingId, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    NodeId, ProjectFrame, RevisionId, SplitIdentities,
};

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
    let (video, picture_context) = fallback(workspace, at)?;
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
) -> Result<(HoldVideo, Option<deadpan_core::CapturedFraming>), String> {
    let provider = deadpan_cli::pause::pause_provider(
        &workspace.document,
        &workspace.plan,
        at,
        &mut |asset| {
            workspace
                .sources
                .get(asset)
                .and_then(|source| source.video_index.clone())
                .ok_or_else(|| "Pause needs the registered original's measured video index.".into())
        },
    )?;
    Ok((provider.video, provider.picture_context))
}
