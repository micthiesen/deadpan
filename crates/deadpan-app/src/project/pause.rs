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
    black: bool,
    allocate: impl FnMut() -> NodeId,
) -> Result<CommandRequest, String> {
    prepare_selected(
        workspace,
        duration,
        new_revision,
        id,
        Selection {
            at,
            black,
            ai: false,
            within: None,
            all_plays: false,
        },
        allocate,
    )
}

pub(super) fn prepare_scoped(
    workspace: &Workspace,
    target: &super::scoped::Target,
    duration: FrameDuration,
    black: bool,
    ai: bool,
) -> Result<CommandRequest, String> {
    let (within, all_plays) = target.pause_instance(workspace)?;
    if all_plays && ai {
        return Err(
            "An AI pause needs one play. Choose This play (:scope play N), then insert it.".into(),
        );
    }
    let within = within.clone();
    prepare_selected(
        workspace,
        duration,
        super::service::revision(),
        super::service::node(),
        Selection {
            at: target.cursor,
            black,
            ai,
            within: Some(within),
            all_plays,
        },
        super::service::node,
    )
}

struct Selection {
    at: ProjectFrame,
    black: bool,
    ai: bool,
    within: Option<deadpan_core::InstancePath>,
    /// Into the Repeat's shared definition: no play is isolated.
    all_plays: bool,
}

fn prepare_selected(
    workspace: &Workspace,
    duration: FrameDuration,
    new_revision: RevisionId,
    id: NodeId,
    selection: Selection,
    mut allocate: impl FnMut() -> NodeId,
) -> Result<CommandRequest, String> {
    let Selection {
        at,
        black,
        ai,
        within,
        all_plays,
    } = selection;
    let document = &workspace.document;
    let total = workspace.plan.duration().frames();
    if at.0 < 0 || at.0 > total {
        return Err("Pause boundary is outside the sequence.".into());
    }
    if duration == FrameDuration::ZERO {
        return Err("Pause resolves to 0 frames; no edit was made.".into());
    }
    // Resolve ownership before sampling a fallback or allocating an edit.
    let target = match &within {
        Some(within) => document
            .pause_insertion_within(at, within)
            .map(deadpan_core::PauseInsertionTarget::Occurrence),
        None => document.pause_insertion_target(at),
    }
    .map_err(|error| error.to_string())?;
    // Black punctuation shows the project background: no picture is sampled
    // or captured, so inner framing has nothing to retain.
    let (video, picture_context) = if black {
        (HoldVideo::Background, None)
    } else {
        fallback(workspace, at, within.as_ref())?
    };
    let hold = HoldRecipe {
        duration,
        video,
        audio: HoldAudio::Silence,
        picture_context,
    };
    let timing = AudioTimingId {
        allocation: new_revision.clone(),
        ordinal: 0,
    };
    let command = match target {
        deadpan_core::PauseInsertionTarget::Ordinary(target) => Command::InsertTime {
            at,
            hold,
            id,
            timing,
            identities: SplitIdentities {
                nodes: target
                    .split
                    .map(|split| (0..split.required_ids).map(|_| allocate()).collect())
                    .unwrap_or_default(),
            },
        },
        deadpan_core::PauseInsertionTarget::Retimed(_) if ai => {
            return Err(
                "An AI pause inside a speed change is not supported yet; insert a freeze pause."
                    .into(),
            );
        }
        deadpan_core::PauseInsertionTarget::Retimed(target) => Command::InsertTime {
            at,
            hold,
            id,
            timing,
            identities: SplitIdentities {
                nodes: (0..target.required_ids).map(|_| allocate()).collect(),
            },
        },
        deadpan_core::PauseInsertionTarget::Occurrence(target) => Command::InsertOccurrenceTime {
            at,
            within,
            hold,
            id,
            timing,
            ai,
            all_plays,
            identities: deadpan_core::OccurrencePauseIdentities {
                isolation: deadpan_core::OccurrenceIdentities {
                    nodes: (0..target.isolation_nodes)
                        .filter(|_| !all_plays)
                        .map(|_| allocate())
                        .collect(),
                    marks: (0..target.isolation_marks)
                        .filter(|_| !all_plays)
                        .map(|_| {
                            deadpan_core::MarkId::new(uuid::Uuid::new_v4().to_string())
                                .expect("UUID is a mark identity")
                        })
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..target.split_nodes).map(|_| allocate()).collect(),
                },
                container: target.container.then(&mut allocate),
                gap: (target.gap && !all_plays).then(&mut allocate),
            },
        },
    };
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        command,
        new_revision,
    })
}

fn fallback(
    workspace: &Workspace,
    at: ProjectFrame,
    within: Option<&deadpan_core::InstancePath>,
) -> Result<(HoldVideo, Option<deadpan_core::CapturedFraming>), String> {
    let provider = deadpan_cli::pause::pause_provider_within(
        &workspace.document,
        &workspace.plan,
        at,
        within,
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
