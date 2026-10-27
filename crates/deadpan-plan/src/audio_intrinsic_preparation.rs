//! Allocation-free discovery for an optional cold-preparation optimization.
//! This describes immutable plan recipes, never PCM residency or media access.

use super::{AudioQueryLimits, CompiledKind, RenderPlan};
use crate::PlanError;
use deadpan_core::{ExactRatio, FrozenAudioLayout};

/// Whether this plan's checked authored/history graph is free of intrinsic
/// time/pitch processing. This does not classify arbitrary caller-created
/// projected PCM operands. A positive result is conservative: an unreachable
/// old operand can require processing even when the next read does not visit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioIntrinsicPreparation {
    /// All owned current and retained recipes were examined and none requires
    /// an intrinsic Preserve/Shift processor or its automatic projected input.
    /// This does not prove that an external operand allocates no projected PCM.
    NotRequired { work: usize },
    /// At least one current or retained operand can construct such a processor.
    /// Its source, audibility, cache state and next-read reachability are unknown.
    MayBeRequired { work: usize },
    /// The next record was not visited because the query allowance was spent.
    /// This is not a proof of absence and must select the conservative path.
    WorkLimit { work: usize },
}

impl AudioIntrinsicPreparation {
    /// Every charged visit actually performed by this query, including an
    /// incomplete discovery. Hosts must bill this before any scalar fallback.
    pub const fn work(self) -> usize {
        match self {
            Self::NotRequired { work }
            | Self::MayBeRequired { work }
            | Self::WorkLimit { work } => work,
        }
    }

    /// Only a complete negative proof permits an intrinsic-free optimization.
    pub const fn is_intrinsic_free(self) -> bool {
        matches!(self, Self::NotRequired { .. })
    }
}

struct Visits {
    maximum: usize,
    spent: usize,
}

impl Visits {
    fn next(&mut self) -> bool {
        if self.spent == self.maximum {
            return false;
        }
        self.spent += 1;
        true
    }

    fn limited(&self) -> AudioIntrinsicPreparation {
        AudioIntrinsicPreparation::WorkLimit { work: self.spent }
    }

    fn required(&self) -> AudioIntrinsicPreparation {
        AudioIntrinsicPreparation::MayBeRequired { work: self.spent }
    }

    fn layout(&mut self, layout: &FrozenAudioLayout) -> Option<AudioIntrinsicPreparation> {
        // Charge the layout header before reading its immutable node inventory.
        // No validation, compilation, Repeat expansion or graph clone occurs.
        if !self.next() {
            return Some(self.limited());
        }
        for (id, node) in layout.nodes() {
            if !self.next() {
                return Some(self.limited());
            }
            // This Core predicate checks both integral processing recipes and
            // effective derived clocks. Nominal durations alone are inadequate.
            if layout.retime_processes(id, node) {
                return Some(self.required());
            }
        }
        None
    }
}

impl RenderPlan {
    /// Discover whether this immutable plan can prepare intrinsic time/pitch
    /// PCM, without constructing a query graph, resolving sources, invoking
    /// native code, allocating memory or retaining a new plan/identity handle.
    ///
    /// The scan includes every compiled node (including definitions and sparse
    /// overrides), its own retained layout when present, and **all** timing
    /// layouts. The last inventory covers bound Original operands and every
    /// sound journal clock, including a later clock that can become first for
    /// another occurrence. Unrelated historical nodes deliberately count too.
    ///
    /// Ordinary SourceAudioMapping, sound placement/selection and root sound
    /// delivery routes are affine sample maps, not intrinsic processors. Saved
    /// BeatSounds acquire projected processing only at a processing structural
    /// Retime edge, covered by this complete current/historical scan. Owned
    /// Preserve inputs are separately detected even when their effective live
    /// allocation is unity. A plain unity Preserve Partition stays transparent.
    ///
    /// Work is one header/record visit, charged before that visit. The single
    /// plan header covers access to its immutable inventories. A layout header
    /// and each node are charged separately; a timing-map entry is charged before
    /// its layout. Duplicate layouts may be counted twice rather than requiring
    /// an allocated seen set. maximum_spans is validated but no spans are built.
    /// WorkLimit carries all spent work; it must never be treated as NotRequired
    /// or silently discarded by an optional caller. Invalid limits fail before
    /// any visit. Cancellation/deadlines remain the caller's responsibility,
    /// as with existing bounded Plan queries.
    ///
    /// This result proves no source validity, permission, resource bound, cache
    /// hit or future batch admission. It never authorizes caller-created
    /// operands or replaces the ordinary readers' independent admission. A cold
    /// Audio walker must still refuse actual ProjectedStage/Projected root or
    /// signal inputs under an intrinsic-free eligibility policy, including after
    /// NotRequired: this scan is not a claim about every possible projected PCM
    /// allocation or caller-supplied processing handle.
    pub fn audio_intrinsic_preparation(
        &self,
        limits: AudioQueryLimits,
    ) -> Result<AudioIntrinsicPreparation, PlanError> {
        limits.validate()?;
        let mut visits = Visits {
            maximum: limits.maximum_work,
            spent: 0,
        };
        if !visits.next() {
            return Ok(visits.limited());
        }
        for node in &self.nodes {
            if !visits.next() {
                return Ok(visits.limited());
            }
            if node.preserve_input.is_some()
                || matches!(node.kind, CompiledKind::Retime { scale, pitch, .. }
                    if pitch.processes(scale == ExactRatio::ONE))
            {
                return Ok(visits.required());
            }
        }
        if let Some(layout) = &self.sound_clock_layout
            && let Some(result) = visits.layout(layout)
        {
            return Ok(result);
        }
        for layout in self.audio_bindings.timings().values() {
            if !visits.next() {
                return Ok(visits.limited());
            }
            if let Some(result) = visits.layout(layout) {
                return Ok(result);
            }
        }
        Ok(AudioIntrinsicPreparation::NotRequired { work: visits.spent })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::*;
    use std::collections::BTreeMap;

    #[test]
    fn derived_only_historical_processing_is_detected_without_weakening_admission() {
        let id = |name: &str| NodeId::new(name).unwrap();
        let revision = |name: &str| RevisionId::new(name).unwrap();
        let frames = |count| FrameDuration::new(count).unwrap();
        let mut wire = serde_json::to_value(
            ProjectDocument::new(
                ProjectId::new("derived-only-discovery").unwrap(),
                revision("before"),
                PresentationBasis {
                    width: 16,
                    height: 16,
                    frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                    color_policy: ColorPolicy::SdrRec709,
                },
                id("root"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut view = BeatNode::sequence("View", Vec::new());
        view.kind = NodeKind::Retime {
            child: id("leaf"),
            duration: frames(4),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        };
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (id("root"), BeatNode::sequence("Root", vec![id("view")])),
            (id("view"), view),
            (
                id("leaf"),
                BeatNode::hold(
                    "Leaf",
                    HoldRecipe {
                        duration: frames(8),
                        video: HoldVideo::Background,
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                ),
            ),
        ]))
        .unwrap();
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let transparent = FrozenAudioLayout::capture(&document).unwrap();
        assert!(!transparent.retime_processes(&id("view"), &transparent.nodes()[&id("view")]));
        let mut visits = Visits {
            maximum: 65_536,
            spent: 0,
        };
        assert_eq!(visits.layout(&transparent), None);
        assert_eq!(visits.spent, 4);

        // The frozen grammar admits timing-only derived clocks without an
        // owned Preserve input. Only the effective clock changes here: the
        // integral mapping remains 4 -> 4 and therefore cannot trip the
        // integral processing branch of retime_processes.
        let mut layout_wire = serde_json::to_value(&transparent).unwrap();
        layout_wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([(
            id("view"),
            FrozenRetimeClock {
                extent: ExactRatio::integer(4),
                child_start: ExactRatio::ZERO,
                child_end: ExactRatio::integer(2),
            },
        )]))
        .unwrap();
        let derived = FrozenAudioLayout::from_json(&layout_wire.to_string()).unwrap();
        let node = &derived.nodes()[&id("view")];
        let FrozenAudioKind::Retime { mapping, pitch, .. } = &node.kind else {
            unreachable!()
        };
        assert_eq!(mapping.duration(), node.duration);
        assert!(!pitch.processes(true));
        assert!(derived.retime_processes(&id("view"), node));
        let mut visits = Visits {
            maximum: 65_536,
            spent: 0,
        };
        let result = visits.layout(&derived).unwrap();
        assert_eq!(result, AudioIntrinsicPreparation::MayBeRequired { work: 4 });
        assert!(!result.is_intrinsic_free());

        let binding = |layout: FrozenAudioLayout| {
            let timing = AudioTimingId {
                allocation: revision("clock"),
                ordinal: 0,
            };
            AudioBindingState::new(
                vec![AudioTimingRecord {
                    id: timing.clone(),
                    layout,
                }],
                BTreeMap::from([(
                    id("leaf"),
                    OwnedAudioBinding {
                        lattice: AudioPlacementTemplate {
                            reference_local_offset: ExactRatio::ZERO,
                            gap_after: None,
                            reference: AudioReferenceClock {
                                recipe: AudioRecipeKind::Node,
                                timing,
                                root: AudioClockRoot::ProjectRootRoundEven,
                                physical: id("leaf"),
                            },
                            arguments: Vec::new(),
                            births: Vec::new(),
                        },
                        reanchors: Vec::new(),
                        resume: None,
                    },
                )]),
            )
        };
        assert!(binding(transparent).is_ok());
        let error = binding(derived).unwrap_err();
        assert!(
            error
                .message
                .contains("retained exact Preserve clocks require an owned output route")
        );

        // Do not invent a valid public current Source binding for this table:
        // current binding and retained-context admission deliberately refuse it.
        // The helper test above exercises the exact future-safe discovery path.
        let context = FrozenAudioContext::capture(&document).unwrap();
        let mut context_wire = serde_json::to_value(&context).unwrap();
        context_wire["layout"] = layout_wire;
        assert!(FrozenAudioContext::from_json(&context_wire.to_string()).is_err());
    }
}
