//! Owned physical definitions evaluated on an explicitly selected point grid.
//! Integer label rebasing does not change the selected origin or sampling phase.

use std::ops::Range;

use deadpan_core::{ExactRatio, MIX_SAMPLE_RATE, NodeId, TimeError};

use super::{
    AudioDefinitionSelector, AudioRootPlacement, AudioSignal, RenderPlan, SignalSample,
    SignalTransform,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap, PlanError, ReferenceSample};

/// One current owned recipe on a PointCeil clock. Reference samples preserve
/// the original signed labels. The returned signal uses nonnegative storage
/// labels by subtracting only the already allocated integer reference start.
/// No rounded root allocation or artificial half-sample shift is involved.
#[derive(Debug, Clone)]
pub struct AudioPointDomain<'plan> {
    plan: &'plan RenderPlan,
    selector: AudioDefinitionSelector,
    root: usize,
    placement: AudioRootPlacement,
    grid: AudioSampleGrid<ReferenceSample>,
    sampling: AudioSampleMap<ReferenceSample>,
    samples: Range<ReferenceSample>,
    signal: AudioSignal<'plan>,
}

impl<'plan> AudioPointDomain<'plan> {
    pub(super) fn new(
        plan: &'plan RenderPlan,
        root: usize,
        selector: AudioDefinitionSelector,
        placement: AudioRootPlacement,
        grid_origin: ExactRatio,
    ) -> Result<Self, PlanError> {
        let rate = plan.metadata.presentation_basis.frame_rate;
        let spacing = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid = AudioSampleGrid::<ReferenceSample>::new(
            grid_origin,
            spacing,
            AudioBoundaryRule::PointCeil,
        )?;
        let support = placement.local_support();
        let project = |local: ExactRatio| {
            placement
                .origin()
                .checked_add(local.checked_mul(placement.root_frames_per_local_frame())?)
        };
        let support = project(support.start)?..project(support.end)?;
        let samples = grid.boundary(support.start)?..grid.boundary(support.end)?;
        // AudioSignal has bounded, nonnegative i64 storage coordinates. Keep
        // the exact nonempty frame support even when it contains no grid point.
        samples
            .end
            .0
            .checked_sub(samples.start.0)
            .ok_or(TimeError::Overflow)?;
        let rebased_origin = grid.at(samples.start)?;
        let transform = SignalTransform {
            signal_origin: placement.origin(),
            signal_frames_per_local_frame: placement.root_frames_per_local_frame(),
            grid_origin: rebased_origin,
            signal_frames_per_sample: spacing,
        };
        let sampling = AudioSampleMap::new(
            samples.start,
            rebased_origin
                .checked_sub(placement.origin())?
                .checked_div(placement.root_frames_per_local_frame())?,
            spacing.checked_div(placement.root_frames_per_local_frame())?,
        )?;
        let signal =
            AudioSignal::for_placed_definition(plan, root, selector.clone(), support, transform);
        Ok(Self {
            plan,
            selector,
            root,
            placement,
            grid,
            sampling,
            samples,
            signal,
        })
    }

    pub fn definition(&self) -> &AudioDefinitionSelector {
        &self.selector
    }
    pub fn root(&self) -> &NodeId {
        &self.plan.nodes[self.root].inspection.id
    }
    pub fn placement(&self) -> &AudioRootPlacement {
        &self.placement
    }
    pub fn reference_grid(&self) -> AudioSampleGrid<ReferenceSample> {
        self.grid
    }
    /// Exact reference-sample to owned-local-frame mapping, including positions
    /// outside visible support. This conversion does not authorize a PCM read.
    pub fn reference_sampling(&self) -> AudioSampleMap<ReferenceSample> {
        self.sampling
    }
    pub fn reference_samples(&self) -> Range<ReferenceSample> {
        self.samples.clone()
    }
    pub fn signal(&self) -> AudioSignal<'plan> {
        self.signal.clone()
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    pub fn reference_at_local(&self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        local
            .checked_sub(self.sampling.local_at_anchor())?
            .checked_div(self.sampling.local_frames_per_sample())?
            .checked_add(ExactRatio::integer(self.sampling.anchor().0))
    }

    /// Convert a retained label to a storage label. The half-open end is
    /// accepted so callers can convert complete range boundaries exactly.
    pub fn signal_at_reference(&self, sample: ReferenceSample) -> Result<SignalSample, PlanError> {
        if sample < self.samples.start || sample > self.samples.end {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(SignalSample(
            sample
                .0
                .checked_sub(self.samples.start.0)
                .ok_or(TimeError::Overflow)?,
        ))
    }

    pub fn reference_at_signal(&self, sample: SignalSample) -> Result<ReferenceSample, PlanError> {
        if sample.0 < 0 || sample > self.signal.sample_count()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(ReferenceSample(
            self.samples
                .start
                .0
                .checked_add(sample.0)
                .ok_or(TimeError::Overflow)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::*;

    fn plan(rate: FrameRate) -> RenderPlan {
        let doc = ProjectDocument::new(
            ProjectId::new("point").unwrap(),
            RevisionId::new("r0").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let mut wire = serde_json::to_value(doc).unwrap();
        wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
            "Root",
            vec![NodeId::new("hold").unwrap()],
        ))
        .unwrap();
        wire["nodes"]["hold"] = serde_json::to_value(BeatNode::hold(
            "Physical Hold",
            HoldRecipe {
                duration: FrameDuration::new(2).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        ))
        .unwrap();
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
    }
    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }

    #[test]
    fn signed_ntsc_point_labels_rebase_without_changing_selected_origin() {
        let plan = plan(FrameRate::new(30_000, 1001).unwrap());
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new("hold").unwrap(),
            })
            .unwrap();
        let domain = definition
            .in_point_clock(
                AudioRootPlacement::new(
                    ratio(-1, 3),
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::integer(2),
                )
                .unwrap(),
                ratio(1, 7),
            )
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(-762)..ReferenceSample(2441)
        );
        assert_eq!(domain.reference_grid().frame_origin(), ratio(1, 7));
        assert_eq!(
            domain.reference_grid().boundary_rule(),
            AudioBoundaryRule::PointCeil
        );
        assert_eq!(domain.signal().sample_count().unwrap(), SignalSample(3203));
        assert_eq!(
            domain
                .reference_sampling()
                .local_at(ReferenceSample(-762))
                .unwrap(),
            ratio(5, 12012)
        );
        assert_eq!(
            domain.reference_at_local(ratio(5, 12012)).unwrap(),
            ExactRatio::integer(-762)
        );
        for label in [-762, -1, 0, 2441] {
            let stored = domain.signal_at_reference(ReferenceSample(label)).unwrap();
            assert_eq!(
                domain.reference_at_signal(stored).unwrap(),
                ReferenceSample(label)
            );
        }
        assert!(domain.signal_at_reference(ReferenceSample(-763)).is_err());
        assert!(domain.reference_at_signal(SignalSample(3204)).is_err());
        let query = domain
            .signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap();
        assert_eq!(
            query.spans[0].sampling.local_at(SignalSample(0)).unwrap(),
            ratio(5, 12012)
        );
        assert_eq!(query.spans[0].grid.frame_origin(), ratio(-1333, 4004));
    }

    #[test]
    fn point_empty_support_keeps_exact_policy_extent_and_can_gain_output_points() {
        let plan = plan(FrameRate::new(192_000, 1).unwrap());
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new("hold").unwrap(),
            })
            .unwrap();
        let domain = definition
            .in_point_clock(
                AudioRootPlacement::new(
                    ExactRatio::ONE,
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::ONE,
                )
                .unwrap(),
                ExactRatio::ZERO,
            )
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(1)..ReferenceSample(1)
        );
        assert_eq!(domain.signal().sample_count().unwrap(), SignalSample(0));
        assert_eq!(
            domain.signal().support(),
            ExactRatio::ONE..ExactRatio::integer(2)
        );
        assert!(
            domain
                .signal()
                .query_flattened(SignalSample(0)..SignalSample(0), Default::default())
                .unwrap()
                .spans
                .is_empty()
        );
        let output = definition
            .in_point_clock(
                AudioRootPlacement::new(
                    ExactRatio::integer(8),
                    ExactRatio::integer(8),
                    ExactRatio::ZERO..ExactRatio::ONE,
                )
                .unwrap(),
                ExactRatio::ZERO,
            )
            .unwrap();
        assert_eq!(
            output.reference_samples(),
            ReferenceSample(2)..ReferenceSample(4)
        );
        let query = output
            .signal()
            .query_flattened(SignalSample(0)..SignalSample(2), Default::default())
            .unwrap();
        assert!(matches!(
            query.spans[0].content,
            super::super::AudioSignalContent::Leaf(super::super::AudioContent::Silence {
                reason: super::super::SilenceReason::SilentHold
            })
        ));
    }
}
