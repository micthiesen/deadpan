//! Which authored recipe a selected pause's sound edit changes.
//!
//! A selected beat names a pause sound in three shapes: an ordinary Hold, a
//! Hold fragment left by Split (transparent Partitions over that fragment's
//! own Hold copy) and a Repeat whose default gap recipe plays between plays
//! without an explicit branch. Every caller (native sheet, semantic planner,
//! headless) resolves through this one function, so they edit the same recipe.

use crate::{Command, HoldAudio, HoldRecipe, NodeId, NodeKind, ProjectDocument, RetimePurpose};

/// The recipe a pause-sound edit on a selected beat changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseSoundTarget {
    /// An ordinary Hold, directly selected or inside a Split fragment's
    /// Partitions. Only that fragment's Hold changes.
    Hold { node: NodeId, fragment: bool },
    /// A Repeat's default gap recipe: every gap without an explicit branch.
    RepeatGap { node: NodeId },
}

impl PauseSoundTarget {
    /// The node whose recipe changes: the Hold, or the Repeat owning the gap.
    pub fn node(&self) -> &NodeId {
        match self {
            Self::Hold { node, .. } | Self::RepeatGap { node } => node,
        }
    }

    pub fn command(&self, audio: HoldAudio) -> Command {
        match self {
            Self::Hold { node, .. } => Command::SetHoldAudio {
                node: node.clone(),
                audio,
            },
            Self::RepeatGap { node } => Command::SetRepeatGapAudio {
                node: node.clone(),
                audio,
            },
        }
    }

    /// The recipe whose audio this target changes.
    pub fn recipe<'a>(&self, document: &'a ProjectDocument) -> Option<&'a HoldRecipe> {
        match &document.nodes().get(self.node())?.kind {
            NodeKind::Hold { recipe } => Some(recipe),
            NodeKind::Repeat { gap, .. } => gap.as_ref(),
            _ => None,
        }
    }
}

impl ProjectDocument {
    /// The pause sound a selected beat names, or `None` when it has none: a
    /// Repeat without a positive gap, a Partition over anything but a Hold,
    /// or any other kind.
    pub fn pause_sound_target(&self, selected: &NodeId) -> Option<PauseSoundTarget> {
        let mut node = selected;
        let mut fragment = false;
        loop {
            match &self.nodes().get(node)?.kind {
                NodeKind::Hold { .. } => {
                    return Some(PauseSoundTarget::Hold {
                        node: node.clone(),
                        fragment,
                    });
                }
                NodeKind::Retime {
                    purpose: RetimePurpose::Partition,
                    child,
                    ..
                } => {
                    fragment = true;
                    node = child;
                }
                NodeKind::Repeat { gap: Some(gap), .. }
                    if !fragment && gap.duration.frames() > 0 =>
                {
                    return Some(PauseSoundTarget::RepeatGap { node: node.clone() });
                }
                _ => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests;
