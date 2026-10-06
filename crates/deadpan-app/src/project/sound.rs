//! Root sound placement from the workspace's measured catalog evidence. The
//! derivation itself is [`deadpan_cli::sound_events`], shared with the
//! headless `sound` command so both commit the same events.

pub use deadpan_cli::sound_events::PauseTarget;
use deadpan_core::{AssetId, AudioSample, ProjectFrame, SoundEvent, SoundId};

use super::Workspace;

pub fn pause_target(
    workspace: &Workspace,
    id: &SoundId,
    at: ProjectFrame,
) -> Result<PauseTarget, String> {
    deadpan_cli::sound_events::pause_target(&workspace.document, &workspace.plan, id, at)
}

/// Place the complete measured audio-only catalog span at an exact 48 kHz onset.
pub fn placement(
    workspace: &Workspace,
    asset: &AssetId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    let source = workspace
        .sources
        .get(asset)
        .ok_or("The catalog sound is no longer registered.")?;
    deadpan_cli::sound_events::placement(
        &workspace.document,
        &workspace.plan,
        &source.receipt,
        asset,
        at,
    )
}

pub(super) fn moved(
    workspace: &Workspace,
    id: &SoundId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    deadpan_cli::sound_events::moved(&workspace.document, &workspace.plan, id, at)
}

pub fn nudge(workspace: &Workspace, id: &SoundId, frames: i64) -> Result<SoundEvent, String> {
    deadpan_cli::sound_events::nudge(&workspace.document, &workspace.plan, id, frames)
}

pub fn cut(workspace: &Workspace, id: &SoundId, at: ProjectFrame) -> Result<SoundEvent, String> {
    deadpan_cli::sound_events::cut(&workspace.document, &workspace.plan, id, at)
}
