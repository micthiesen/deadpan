//! Sample allocation and PCM lookup are different clocks. A structural edge is
//! rounded on its owning grid; the sample map retains an exact local phase and
//! rate independently of that edge and of any particular read request.
use std::marker::PhantomData;

use deadpan_core::{AudioSample, ExactRatio, InsertionBias, TimeError};
use serde::Serialize;

use crate::{PlanError, SignalSample};

/// Root output encloses edits using ties-to-even endpoints. Prepared signals
/// contain sample points and therefore use ceil endpoints. Neither rule rounds
/// a sampling coordinate or derives a rate from an allocated sample count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBoundaryRule {
    RoundEven,
    PointCeil,
}

/// The origin, spacing and boundary rule of one sampling domain. The containing
/// span/stage supplies its project, revision and occurrence identity. Equal grid
/// values alone do not identify equivalent media or DSP preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioSampleGrid<S> {
    frame_origin: ExactRatio,
    frames_per_sample: ExactRatio,
    boundary_rule: AudioBoundaryRule,
    #[serde(skip)]
    sample_domain: PhantomData<S>,
}

impl<S> AudioSampleGrid<S> {
    pub fn new(
        frame_origin: ExactRatio,
        frames_per_sample: ExactRatio,
        boundary_rule: AudioBoundaryRule,
    ) -> Result<Self, PlanError> {
        if frames_per_sample.compare_integer(0).is_le() {
            return Err(PlanError::InvalidPlan(
                "sample grid spacing must be positive",
            ));
        }
        Ok(Self {
            frame_origin,
            frames_per_sample,
            boundary_rule,
            sample_domain: PhantomData,
        })
    }

    pub fn frame_origin(&self) -> ExactRatio {
        self.frame_origin
    }

    pub fn frames_per_sample(&self) -> ExactRatio {
        self.frames_per_sample
    }

    pub fn boundary_rule(&self) -> AudioBoundaryRule {
        self.boundary_rule
    }

    fn boundary_index(&self, frame: ExactRatio) -> Result<i64, TimeError> {
        let exact = frame
            .checked_sub(self.frame_origin)?
            .checked_div(self.frames_per_sample)?;
        let rounded = match self.boundary_rule {
            AudioBoundaryRule::RoundEven => exact.round_even()?,
            AudioBoundaryRule::PointCeil => exact.ceil()?,
        };
        i64::try_from(rounded).map_err(|_| TimeError::Overflow)
    }

    fn frame_at(&self, sample: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.frame_origin
            .checked_add(sample.checked_mul(self.frames_per_sample)?)
    }

    fn probe_index(&self, sample: i64) -> Result<(ExactRatio, InsertionBias), TimeError> {
        let (position, bias) = match self.boundary_rule {
            AudioBoundaryRule::PointCeil => (ExactRatio::integer(sample), InsertionBias::Right),
            AudioBoundaryRule::RoundEven => (
                ExactRatio::new(i128::from(sample) * 2 + 1, 2)?,
                if sample % 2 == 0 {
                    InsertionBias::Right
                } else {
                    InsertionBias::Left
                },
            ),
        };
        Ok((self.frame_at(position)?, bias))
    }
}

/// Exact PCM lookup in the leaf's local frame clock, or in the full intrinsic
/// output clock when the span stops at a Preserve/RoomTone preparation domain.
/// This is independent of structural placement, filter support and envelopes.
/// Anchors may lie outside the current crop; queries must not restart phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioSampleMap<S> {
    anchor: S,
    local_at_anchor: ExactRatio,
    local_frames_per_sample: ExactRatio,
}

impl<S: Copy> AudioSampleMap<S> {
    pub fn new(
        anchor: S,
        local_at_anchor: ExactRatio,
        local_frames_per_sample: ExactRatio,
    ) -> Result<Self, PlanError> {
        if local_frames_per_sample.compare_integer(0).is_le() {
            return Err(PlanError::InvalidPlan("sample map rate must be positive"));
        }
        Ok(Self {
            anchor,
            local_at_anchor,
            local_frames_per_sample,
        })
    }

    pub fn anchor(&self) -> S {
        self.anchor
    }

    pub fn local_at_anchor(&self) -> ExactRatio {
        self.local_at_anchor
    }

    pub fn local_frames_per_sample(&self) -> ExactRatio {
        self.local_frames_per_sample
    }

    fn local_at_offset(&self, offset: i128) -> Result<ExactRatio, TimeError> {
        self.local_at_anchor
            .checked_add(ExactRatio::new(offset, 1)?.checked_mul(self.local_frames_per_sample)?)
    }
}

// Keep the root and preparation sample types distinct without exposing a
// general numeric conversion trait to consumers.
macro_rules! sample_domain {
    ($sample:ident) => {
        impl AudioSampleGrid<$sample> {
            pub fn boundary(&self, frame: ExactRatio) -> Result<$sample, TimeError> {
                Ok($sample(self.boundary_index(frame)?))
            }

            pub fn at(&self, sample: $sample) -> Result<ExactRatio, TimeError> {
                self.frame_at(ExactRatio::integer(sample.0))
            }

            /// Search probe for the structural interval allocated to this sample.
            /// This is not the sample position used for PCM lookup.
            pub fn probe(&self, sample: $sample) -> Result<(ExactRatio, InsertionBias), TimeError> {
                self.probe_index(sample.0)
            }
        }

        impl AudioSampleMap<$sample> {
            pub fn local_at(&self, sample: $sample) -> Result<ExactRatio, TimeError> {
                self.local_at_offset(i128::from(sample.0) - i128::from(self.anchor.0))
            }

            /// Continue the current map's exact phase at a new sample boundary.
            /// Repeated insertion must compose this map, not recover a phase
            /// from original picture coordinates or change its authored step.
            pub fn resume(&self, old_cut: $sample, new_anchor: $sample) -> Result<Self, PlanError> {
                Self::new(
                    new_anchor,
                    self.local_at(old_cut)?,
                    self.local_frames_per_sample,
                )
            }
        }
    };
}

sample_domain!(AudioSample);
sample_domain!(SignalSample);

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }

    #[test]
    fn resume_composes_current_ntsc_phase_without_rescaling_the_suffix() {
        let grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            ratio(5, 8008),
            AudioBoundaryRule::RoundEven,
        )
        .unwrap();
        let original =
            AudioSampleMap::new(AudioSample(0), ExactRatio::ZERO, ratio(147, 160)).unwrap();
        let boundary = |frame| grid.boundary(ExactRatio::integer(frame)).unwrap();
        assert_eq!(boundary(1), AudioSample(1602));
        assert_eq!(boundary(2), AudioSample(3203));
        let first = original.resume(boundary(1), boundary(2)).unwrap();
        assert_eq!(first.local_at(boundary(2)).unwrap(), ratio(117747, 80));
        let second = first.resume(boundary(3), boundary(4)).unwrap();
        assert_eq!(second.local_at(boundary(4)).unwrap(), ratio(117747, 40));
        assert_ne!(
            second.local_at(boundary(4)).unwrap(),
            original.local_at(boundary(2)).unwrap()
        );
        for n in [0, 1, 2, 96, 1601, 8197] {
            assert_eq!(
                second.local_at(AudioSample(boundary(4).0 + n)).unwrap(),
                first.local_at(AudioSample(boundary(3).0 + n)).unwrap(),
            );
        }
        assert_eq!(second.local_frames_per_sample(), ratio(147, 160));
        assert_eq!(boundary(3).0 - boundary(2).0, 1602);
        assert_eq!(boundary(2).0 - boundary(1).0, 1601);
    }

    #[test]
    fn root_rounding_and_prepared_point_grids_have_explicit_different_endpoints() {
        let root = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            ratio(1, 2000),
            AudioBoundaryRule::RoundEven,
        )
        .unwrap();
        let signal = AudioSampleGrid::<SignalSample>::new(
            ExactRatio::ZERO,
            ratio(1, 2000),
            AudioBoundaryRule::PointCeil,
        )
        .unwrap();
        assert_eq!(root.boundary(ratio(2, 3)).unwrap(), AudioSample(1333));
        assert_eq!(signal.boundary(ratio(2, 3)).unwrap(), SignalSample(1334));
        let shifted = AudioSampleGrid::<SignalSample>::new(
            ratio(-2, 3),
            ratio(1, 2000),
            AudioBoundaryRule::PointCeil,
        )
        .unwrap();
        assert_eq!(shifted.at(SignalSample(0)).unwrap(), ratio(-2, 3));
        assert_eq!(
            shifted.boundary(ExactRatio::ZERO).unwrap(),
            SignalSample(1334)
        );
        for n in -8..8 {
            let half = ratio(i128::from(n) * 2 + 1, 4000);
            let (probe, bias) = root.probe(AudioSample(n)).unwrap();
            assert_eq!(probe, half);
            assert_eq!(
                bias,
                if n % 2 == 0 {
                    InsertionBias::Right
                } else {
                    InsertionBias::Left
                }
            );
            assert_eq!(
                root.boundary(half).unwrap(),
                AudioSample(if n % 2 == 0 { n } else { n + 1 })
            );
            assert_eq!(
                signal.probe(SignalSample(n)).unwrap(),
                (ratio(i128::from(n), 2000), InsertionBias::Right)
            );
        }
    }

    #[test]
    fn map_keeps_out_of_crop_anchors_and_checks_signed_arithmetic() {
        let map =
            AudioSampleMap::new(AudioSample(i64::MIN), ratio(-1, 3), ExactRatio::ONE).unwrap();
        assert_eq!(
            map.local_at(AudioSample(i64::MAX)).unwrap(),
            ratio((i128::from(i64::MAX) - i128::from(i64::MIN)) * 3 - 1, 3)
        );
        let signal = AudioSampleMap::new(SignalSample(2000), ratio(2, 3), ratio(1, 2000)).unwrap();
        assert_eq!(signal.local_at(SignalSample(0)).unwrap(), ratio(-1, 3));
        assert_eq!(
            signal
                .resume(SignalSample(0), SignalSample(50))
                .unwrap()
                .local_at(SignalSample(49))
                .unwrap(),
            ratio(-2003, 6000)
        );
        for invalid in [ExactRatio::ZERO, ratio(-1, 2)] {
            assert!(AudioSampleMap::new(AudioSample(0), ExactRatio::ZERO, invalid).is_err());
            assert!(
                AudioSampleGrid::<AudioSample>::new(
                    ExactRatio::ZERO,
                    invalid,
                    AudioBoundaryRule::RoundEven
                )
                .is_err()
            );
        }
        let huge =
            AudioSampleMap::new(AudioSample(0), ExactRatio::ZERO, ratio(i128::MAX, 1)).unwrap();
        assert_eq!(huge.local_at(AudioSample(2)), Err(TimeError::Overflow));
    }
}
