//! Authored definition outputs, explicitly distinct from project occurrences.

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, InstancePath, MIX_SAMPLE_RATE, NodeId, PitchPolicy,
};
use serde::{Deserialize, Serialize};

use super::audio::EnvelopeConstraint;
use super::audio_domain::AudioWalkSeed;
use super::{
    AudioBoundaryKind, AudioDomain, AudioRootPlacement, AudioSignal, AudioTransform, CompiledKind,
    RenderPlan,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, PlanError};

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
    pub(super) plan: &'plan RenderPlan,
    pub(super) selector: AudioDefinitionSelector,
    pub(super) root: usize,
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

    /// Evaluate this current owned recipe in an explicit, possibly signed root
    /// clock. Only one physical leaf or opaque Preserve output can own a domain;
    /// a Sequence, Repeat or transparent Retime needs per-domain placement.
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
        super::AudioPointDomain::new(
            self.plan,
            self.root,
            self.selector.clone(),
            placement,
            grid_origin,
        )
    }

    fn validate_physical_placement(&self, placement: &AudioRootPlacement) -> Result<(), PlanError> {
        match &self.plan.nodes[self.root].kind {
            CompiledKind::Source { .. } | CompiledKind::Hold { .. } => {}
            CompiledKind::Retime {
                pitch: PitchPolicy::Preserve,
                scale,
                ..
            } if *scale != ExactRatio::ONE => {}
            _ => {
                return Err(PlanError::InvalidAudioRootPlacement(
                    "definition must be a Source, Hold or nonunity Preserve",
                ));
            }
        }
        let support = placement.local_support();
        if support
            .end
            .compare_integer(self.duration().frames())
            .is_gt()
        {
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
        let support = placement.local_support();
        let rate = self.plan.metadata.presentation_basis.frame_rate;
        let frames_per_sample = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let transform = AudioTransform {
            project_origin: placement.origin(),
            project_frames_per_local_frame: placement.root_frames_per_local_frame(),
            project_frames_per_sample: frames_per_sample,
        };
        let project_at = |point: ExactRatio| {
            placement
                .origin()
                .checked_add(point.checked_mul(placement.root_frames_per_local_frame())?)
        };
        let extent = project_at(support.start)?..project_at(support.end)?;
        let grid = AudioSampleGrid::<AudioSample>::new(grid_origin, frames_per_sample, rule)?;
        let samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        Ok(AudioDomain {
            plan: self.plan,
            seed: AudioWalkSeed {
                definition: Some(self.selector.clone()),
                bypass_binding: None,
                node: self.root,
                transform,
                grid,
                extent: extent.clone(),
                envelope: Some(extent.clone()),
                constraints: vec![EnvelopeConstraint {
                    placement_support: true,
                    range: extent,
                    node: self.root,
                    repeat_count: 0,
                    gap_after: None,
                    kinds: (AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd),
                }],
                repeats: Vec::new(),
                retimes: Vec::new(),
                gap: None,
            },
            samples: samples.clone(),
            visible: samples,
            instance: InstancePath {
                node: self.root().clone(),
                repeats: Vec::new(),
            },
            placement: Some(placement),
        })
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
}
