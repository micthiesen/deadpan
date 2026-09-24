//! Authored definition outputs, explicitly distinct from project occurrences.

use deadpan_core::{FrameDuration, NodeId};
use serde::{Deserialize, Serialize};

use super::{AudioSignal, CompiledKind, RenderPlan};
use crate::PlanError;

/// An authored alias request, not a rendered occurrence or a live binding.
/// RepeatDefault selects the default child even when every play is overridden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioDefinitionSelector {
    Node { node: NodeId },
    RepeatDefault { repeat: NodeId },
}

/// A definition's intrinsic output in its own local-zero 48 kHz point grid.
/// Relative occurrence paths are branded by `selector`, never by an invented
/// outer Repeat play. This handle grants neither media admission nor an
/// authored binding to a future play.
#[derive(Debug, Clone)]
pub struct AudioDefinition<'plan> {
    plan: &'plan RenderPlan,
    selector: AudioDefinitionSelector,
    root: usize,
}

impl RenderPlan {
    /// Resolve an authored definition directly, without probing visible root
    /// allocation. Nested traversal remains bounded by each signal query.
    pub fn audio_definition(
        &self,
        selector: AudioDefinitionSelector,
    ) -> Result<AudioDefinition<'_>, PlanError> {
        let alias = match &selector {
            AudioDefinitionSelector::Node { node } => node,
            AudioDefinitionSelector::RepeatDefault { repeat } => repeat,
        };
        let Some(&node) = self.by_id.get(alias) else {
            return Err(PlanError::InvalidAudioDefinitionSelector(selector));
        };
        let root = match &selector {
            AudioDefinitionSelector::Node { .. } => node,
            AudioDefinitionSelector::RepeatDefault { .. } => {
                let CompiledKind::Repeat { default_child, .. } = &self.nodes[node].kind else {
                    return Err(PlanError::InvalidAudioDefinitionSelector(selector));
                };
                *default_child
            }
        };
        Ok(AudioDefinition {
            plan: self,
            selector,
            root,
        })
    }
}

impl<'plan> AudioDefinition<'plan> {
    pub fn selector(&self) -> &AudioDefinitionSelector {
        &self.selector
    }

    pub fn root(&self) -> &NodeId {
        &self.plan.nodes[self.root].inspection.id
    }

    pub fn duration(&self) -> FrameDuration {
        self.plan.nodes[self.root].inspection.duration
    }

    pub fn signal(&self) -> AudioSignal<'plan> {
        AudioSignal::for_definition(self.plan, self.root, self.selector.clone())
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
}
