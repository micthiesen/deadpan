//! Authored definition outputs, explicitly distinct from project occurrences.

use deadpan_core::{AudioSample, ExactRatio, FrameDuration, InstancePath, MIX_SAMPLE_RATE, NodeId};
use serde::{Deserialize, Serialize};

use super::audio::EnvelopeConstraint;
use super::audio_domain::{AudioWalkSeed, DomainGap};
use super::{
    AudioBoundaryKind, AudioDomain, AudioRootPlacement, AudioSignal, AudioTransform, CompiledKind,
    RenderPlan,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, PlanError};

#[path = "audio_definition_query.rs"]
mod query;

/// An authored alias request, not a rendered occurrence or a live binding.
/// RepeatDefault selects the default child even when every play is overridden.
/// RepeatGap selects a positive configured gap even when there is only one play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioDefinitionSelector {
    Node { node: NodeId },
    RepeatDefault { repeat: NodeId },
    RepeatGap { repeat: NodeId },
}

/// A definition's intrinsic output in its own local-zero 48 kHz point grid.
/// Relative occurrence paths are branded by `selector`, never by an invented
/// outer Repeat play. This handle grants neither media admission nor an
/// authored binding to a future play.
#[derive(Debug, Clone)]
pub struct AudioDefinition<'plan> {
    pub(super) plan: &'plan RenderPlan,
    pub(super) selector: AudioDefinitionSelector,
    pub(super) root: usize,
    pub(super) gap: Option<DomainGap>,
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
            AudioDefinitionSelector::RepeatDefault { repeat }
            | AudioDefinitionSelector::RepeatGap { repeat } => repeat,
        };
        let Some(&node) = self.by_id.get(alias) else {
            return Err(PlanError::InvalidAudioDefinitionSelector(selector));
        };
        let (root, gap) = match &selector {
            AudioDefinitionSelector::Node { .. } => (node, None),
            AudioDefinitionSelector::RepeatDefault { .. } => {
                let CompiledKind::Repeat { default_child, .. } = &self.nodes[node].kind else {
                    return Err(PlanError::InvalidAudioDefinitionSelector(selector));
                };
                (*default_child, None)
            }
            AudioDefinitionSelector::RepeatGap { .. } => {
                let CompiledKind::Repeat {
                    gap_duration,
                    gap_audio: Some(_),
                    ..
                } = &self.nodes[node].kind
                else {
                    return Err(PlanError::InvalidAudioDefinitionSelector(selector));
                };
                if *gap_duration == FrameDuration::ZERO {
                    return Err(PlanError::InvalidAudioDefinitionSelector(selector));
                }
                (
                    node,
                    Some(DomainGap {
                        after: None,
                        duration: *gap_duration,
                    }),
                )
            }
        };
        Ok(AudioDefinition {
            plan: self,
            selector,
            root,
            gap,
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

    /// Integral count in the authored recipe graph. Use [`Self::exact_duration`]
    /// for output allocation rather than an intrinsic processing operand.
    pub fn duration(&self) -> FrameDuration {
        self.gap
            .as_ref()
            .map_or(self.plan.nodes[self.root].inspection.duration, |gap| {
                gap.duration
            })
    }

    /// Current output extent. The integral recipe duration remains separate
    /// so a derived allocation cannot resize an intrinsic DSP operand.
    pub fn exact_duration(&self) -> ExactRatio {
        self.gap
            .as_ref()
            .map_or(self.plan.nodes[self.root].extent, |gap| {
                ExactRatio::integer(gap.duration.frames())
            })
    }

    pub fn signal(&self) -> AudioSignal<'plan> {
        AudioSignal::for_definition(self)
    }

    /// Evaluate this current owned recipe in an explicit, possibly signed root
    /// clock. Only one physical leaf or opaque Preserve output can own a domain;
    /// a Sequence, whole Repeat or transparent Retime needs per-domain placement.
    /// A RepeatGap selects its one physical Hold recipe without an outer play.
    ///
    /// Support constrains Source filter input and output envelope. Preserve
    /// preparation still owns its complete intrinsic input/output history.
    /// Support edges are Automatic; authored edge policies apply only where
    /// their original exact boundaries coincide with the supplied support.
    pub fn in_root_clock(
        &self,
        placement: AudioRootPlacement,
    ) -> Result<AudioDomain<'plan>, PlanError> {
        self.validate_physical_placement(&placement)?;
        self.root_domain(placement)
    }

    /// Retain a distinct selected point-grid origin while placing this owned
    /// physical recipe. The returned storage signal rebases integer labels only.
    pub fn in_point_clock(
        &self,
        placement: AudioRootPlacement,
        grid_origin: ExactRatio,
    ) -> Result<super::AudioPointDomain<'plan>, PlanError> {
        self.validate_physical_placement(&placement)?;
        super::AudioPointDomain::new(self, placement, grid_origin)
    }

    pub(in crate::plan) fn validate_physical_placement(
        &self,
        placement: &AudioRootPlacement,
    ) -> Result<(), PlanError> {
        match (&self.plan.nodes[self.root].kind, &self.gap) {
            (CompiledKind::Repeat { .. }, Some(_)) => {}
            (CompiledKind::Source { .. } | CompiledKind::Hold { .. }, None) => {}
            (CompiledKind::Retime { pitch, scale, .. }, None)
                if pitch.processes(*scale == ExactRatio::ONE) => {}
            _ => {
                return Err(PlanError::InvalidAudioRootPlacement(
                    "definition must be a Source, Hold, Repeat gap or nonunity Preserve",
                ));
            }
        }
        let support = placement.local_support();
        if support.end.compare(self.exact_duration()).is_gt() {
            return Err(PlanError::InvalidAudioRootPlacement(
                "support is outside the definition output",
            ));
        }
        Ok(())
    }

    pub(super) fn root_domain(
        &self,
        placement: AudioRootPlacement,
    ) -> Result<AudioDomain<'plan>, PlanError> {
        self.root_domain_on_grid(placement, ExactRatio::ZERO, AudioBoundaryRule::RoundEven)
    }

    pub(super) fn root_domain_on_grid(
        &self,
        placement: AudioRootPlacement,
        grid_origin: ExactRatio,
        rule: AudioBoundaryRule,
    ) -> Result<AudioDomain<'plan>, PlanError> {
        let mut local = super::audio::Budget {
            remaining: 65_536,
            lookup: super::LookupStats::default(),
        };
        let mut policy = crate::plan::audio_owners::admission::Unmetered;
        let mut budget = crate::plan::audio_owners::admission::Budget::new(&mut local, &mut policy);
        crate::plan::audio_owners::admission::outer::<
            _,
            crate::plan::audio_owners::admission::Unmetered,
        >(self.root_domain_on_grid_query(placement, grid_origin, rule, &mut budget))
        .map(crate::plan::audio_owners::admission::Owned::unmetered)
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
}
