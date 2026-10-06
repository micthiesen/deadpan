//! Which tier serves each playback picture: the exact Original picture when
//! its decoder can deliver it within the picture's budget, otherwise the
//! reduced preview tier, while the Original decoder repositions ahead of the
//! heard clock in bounded steps between pictures.
//!
//! The choice changes only which pixels a picture shows. Audio, the heard
//! clock, the requested picture identity and the presentation timing are the
//! caller's and never depend on it. Costs are measured on the serving thread
//! and kept per Original decoder; nothing here decodes or waits.

use std::time::Duration;

use deadpan_core::{SourceFrameId, SourceFrameIndex};

use crate::source_session::DecodePlan;

/// Fraction of a picture period an exact decode may take. The rest belongs
/// to conversion upload, GPU composition and the viewer's own frame.
pub const EXACT_BUDGET: f64 = 0.7;
/// Fraction of the time left between reduced pictures that repositioning may
/// count on, for scheduling noise and other work on the machine.
pub const REPOSITION_SHARE: f64 = 0.8;
/// Repositioning further ahead than this is pointless: the reduced tier
/// would show for longer than a wait for the next keyframe usually lasts.
pub const MAX_REPOSITION_LEAD: Duration = Duration::from_secs(4);
/// Assumed reduced-tier picture cost before one is measured (a 1080p
/// intra picture measured 8.2–8.6 ms including Metal; proxy record).
const DEFAULT_REDUCED_MS: f64 = 10.0;
/// Weight of the newest observation in each moving average.
const WEIGHT: f64 = 0.25;
/// Seeks and reposition steps shorter than this measure fixed per-call costs
/// (a keyframe picture, conversion) rather than preroll, and are ignored.
pub const MIN_MEASURED_ORDINALS: u64 = 4;
/// Before any seek is measured, preroll is assumed to cost this fraction of
/// a forward picture per ordinal: preroll skips non-reference pictures and
/// converts nothing. Measured 4K long-GOP preroll is about 0.7 ms per ordinal
/// against about 9 ms per forward picture.
const ASSUMED_PREROLL_SHARE: f64 = 0.25;

/// Which pixels serve one playback picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureSource {
    /// The exact decoded Original picture.
    Exact,
    /// The reduced preview tier for the same Original picture.
    Reduced,
}

/// One picture's timing context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureClock {
    /// Time between requested pictures: the project picture period for an
    /// edit, the Original picture's duration for the Original itself.
    pub picture_period: Duration,
    /// Time in which unity-speed playback advances one Original ordinal.
    pub ordinal_period: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
struct Average(Option<f64>);

impl Average {
    fn record(&mut self, value: f64) {
        if !value.is_finite() || value < 0.0 {
            return;
        }
        self.0 = Some(match self.0 {
            None => value,
            Some(previous) => previous + WEIGHT * (value - previous),
        });
    }
}

/// Measured decode costs of one Original decoder and its reduced tier.
#[derive(Clone, Debug, Default)]
pub struct PlaybackPictures {
    /// Milliseconds per picture decoded forward, conversion included.
    forward_ms: Average,
    /// Milliseconds per ordinal of a keyframe seek or reposition.
    seek_ms: Average,
    /// Milliseconds per reduced-tier picture, conversion included.
    reduced_ms: Average,
}

impl PlaybackPictures {
    /// Predicted milliseconds of an exact picture by `plan`; None when no
    /// measurement covers it yet.
    pub fn predicted_ms(&self, plan: DecodePlan) -> Option<f64> {
        match plan {
            DecodePlan::Current => Some(self.forward_ms.0.unwrap_or(0.0) * 0.25),
            DecodePlan::Forward { pictures } => self.forward_ms.0.map(|ms| ms * pictures as f64),
            DecodePlan::Resume { remaining, after } => {
                let preroll = if remaining == 0 {
                    0.0
                } else {
                    self.seek_ms.0? * remaining as f64
                };
                Some(preroll + self.forward_ms.0? * after as f64)
            }
            DecodePlan::Seek { ordinals } => self.seek_ms.0.map(|ms| ms * ordinals as f64),
        }
    }

    /// The tier for a picture whose exact decode follows `plan`. Without a
    /// reduced tier the exact picture is always decoded (and may be late).
    ///
    /// Unmeasured costs: a forward step or the current picture is assumed
    /// affordable and measured; a seek of more than one ordinal is assumed
    /// not to be, since only long-GOP Originals above 1080p have a reduced
    /// tier and their seeks are known to exceed a picture period.
    pub fn choose(&self, plan: DecodePlan, clock: PictureClock, reduced: bool) -> PictureSource {
        if !reduced {
            return PictureSource::Exact;
        }
        let budget = clock.picture_period.as_secs_f64() * 1000.0 * EXACT_BUDGET;
        match self.predicted_ms(plan) {
            Some(ms) if ms <= budget => PictureSource::Exact,
            Some(_) => PictureSource::Reduced,
            None => match plan {
                DecodePlan::Current | DecodePlan::Forward { pictures: 1 } => PictureSource::Exact,
                DecodePlan::Seek { ordinals: 1 } => PictureSource::Exact,
                _ => PictureSource::Reduced,
            },
        }
    }

    /// Record an exact picture's elapsed decode and conversion. A seek of
    /// fewer than [`MIN_MEASURED_ORDINALS`] ordinals measures a keyframe
    /// picture's decode and conversion, not preroll, so it is not recorded.
    pub fn record_exact(&mut self, plan: DecodePlan, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        match plan {
            DecodePlan::Current | DecodePlan::Resume { .. } => {}
            DecodePlan::Forward { pictures } => self.forward_ms.record(ms / pictures as f64),
            DecodePlan::Seek { ordinals } if ordinals >= MIN_MEASURED_ORDINALS => {
                self.seek_ms.record(ms / ordinals as f64);
            }
            DecodePlan::Seek { .. } => {}
        }
    }

    /// Record a reduced-tier picture's elapsed decode and conversion.
    pub fn record_reduced(&mut self, elapsed: Duration) {
        self.reduced_ms.record(elapsed.as_secs_f64() * 1000.0);
    }

    /// Record a reposition step that passed `ordinals` ordinals.
    pub fn record_reposition(&mut self, ordinals: u64, elapsed: Duration) {
        if ordinals >= MIN_MEASURED_ORDINALS {
            self.seek_ms
                .record(elapsed.as_secs_f64() * 1000.0 / ordinals as f64);
        }
    }

    /// The picture ahead of a reduced picture of ordinal `shown` at which
    /// the Original decoder already stands (`current`) or toward which it is
    /// repositioning: playback reaches it without a new reposition.
    pub fn ahead(
        shown: SourceFrameId,
        current: Option<SourceFrameId>,
        repositioning: Option<SourceFrameId>,
    ) -> Option<SourceFrameId> {
        repositioning.or(current).filter(|ahead| ahead.0 > shown.0)
    }

    /// Where the Original decoder should reposition after a reduced picture
    /// of ordinal `shown`, so that exact pictures resume as soon as possible:
    /// a target the decoder can reach, using the time left between reduced
    /// pictures, before playback does; or the next keyframe when that comes
    /// sooner or nothing in the current group of pictures can be reached.
    /// None when even forward decoding exceeds the picture budget (exact
    /// pictures could not keep up after repositioning), no target lies
    /// within [`MAX_REPOSITION_LEAD`], or the decoder already stands or moves
    /// at `ahead` (see [`Self::ahead`]), which playback reaches no later.
    pub fn reposition_target(
        &self,
        index: &SourceFrameIndex,
        shown: SourceFrameId,
        clock: PictureClock,
        ahead: Option<SourceFrameId>,
    ) -> Option<SourceFrameId> {
        let target = self.earliest_reachable(index, shown, clock);
        match (target, ahead) {
            (Some(target), Some(ahead)) if ahead.0 <= target.0 => None,
            (target, _) => target,
        }
    }

    fn earliest_reachable(
        &self,
        index: &SourceFrameIndex,
        shown: SourceFrameId,
        clock: PictureClock,
    ) -> Option<SourceFrameId> {
        let frames = index.frames();
        let last = u64::try_from(frames.len()).ok()?.checked_sub(1)?;
        let picture_ms = clock.picture_period.as_secs_f64() * 1000.0;
        let ordinal_ms = clock.ordinal_period.as_secs_f64() * 1000.0;
        if picture_ms <= 0.0 || ordinal_ms <= 0.0 || shown.0 >= last {
            return None;
        }
        // Unmeasured forward decoding is tried (and measured) after the
        // reposition; measured forward decoding over budget cannot keep up.
        let forward = self.forward_ms.0;
        if forward.is_some_and(|forward| forward > picture_ms * EXACT_BUDGET) {
            return None;
        }
        let lead_limit = (MAX_REPOSITION_LEAD.as_secs_f64() * 1000.0 / ordinal_ms).ceil() as u64;
        let horizon = shown.0.saturating_add(lead_limit.max(1)).min(last);
        // The next keyframe after the shown picture costs about one picture.
        let keyframe = ((shown.0 + 1)..=horizon)
            .find(|ordinal| frames[*ordinal as usize].seek_from == Some(SourceFrameId(*ordinal)));
        let reduced = self.reduced_ms.0.unwrap_or(DEFAULT_REDUCED_MS);
        let useful_per_ordinal =
            ordinal_ms * (1.0 - (reduced / picture_ms).min(1.0)) * REPOSITION_SHARE;
        let seek = self
            .seek_ms
            .0
            .or(forward.map(|forward| forward * ASSUMED_PREROLL_SHARE));
        let in_group = seek.and_then(|seek| {
            if useful_per_ordinal <= seek {
                return None;
            }
            let anchor = frames[shown.0 as usize]
                .seek_from
                .unwrap_or(SourceFrameId(0));
            let behind = (shown.0 + 1).saturating_sub(anchor.0) as f64;
            let lead = (behind * seek / (useful_per_ordinal - seek)).ceil() + 2.0;
            // A lead past the horizon (or not finite) is out of reach.
            if !lead.is_finite() || lead > (horizon - shown.0) as f64 {
                return None;
            }
            Some(shown.0 + lead as u64)
        });
        match (in_group, keyframe) {
            (Some(target), Some(keyframe)) => Some(SourceFrameId(target.min(keyframe))),
            (Some(target), None) => Some(SourceFrameId(target)),
            (None, Some(keyframe)) => Some(SourceFrameId(keyframe)),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use deadpan_core::{AssetId, IndexedSourceFrame, SourceTimeBase, TerminalProvenance};

    use super::*;

    const PERIOD: Duration = Duration::from_micros(33_333);

    fn clock() -> PictureClock {
        PictureClock {
            picture_period: PERIOD,
            ordinal_period: PERIOD,
        }
    }

    /// `count` pictures at 30 fps with a keyframe every `gop`.
    fn index(count: u64, gop: u64) -> SourceFrameIndex {
        let frames = (0..count)
            .map(|ordinal| IndexedSourceFrame {
                identity: SourceFrameId(ordinal),
                pts: i64::try_from(ordinal).unwrap() * 1000,
                reported_duration: Some(1000),
                keyframe: ordinal % gop == 0,
                seek_from: Some(SourceFrameId(ordinal - ordinal % gop)),
                decode_timestamp: Some(i64::try_from(ordinal).unwrap() * 1000),
            })
            .collect();
        SourceFrameIndex::new(
            AssetId::new("a").unwrap(),
            SourceTimeBase::new(1, 30_000).unwrap(),
            frames,
            i64::try_from(count).unwrap() * 1000,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap()
    }

    fn measured(forward: f64, seek: f64, reduced: f64) -> PlaybackPictures {
        let mut costs = PlaybackPictures::default();
        costs.record_exact(
            DecodePlan::Forward { pictures: 1 },
            Duration::from_secs_f64(forward / 1000.0),
        );
        costs.record_exact(
            DecodePlan::Seek { ordinals: 100 },
            Duration::from_secs_f64(seek * 100.0 / 1000.0),
        );
        costs.record_reduced(Duration::from_secs_f64(reduced / 1000.0));
        costs
    }

    #[test]
    fn without_a_reduced_tier_every_picture_is_exact() {
        let costs = measured(40.0, 1.0, 8.0);
        assert_eq!(
            costs.choose(DecodePlan::Seek { ordinals: 200 }, clock(), false),
            PictureSource::Exact
        );
    }

    #[test]
    fn affordable_steps_are_exact_and_long_seeks_are_reduced() {
        let costs = measured(15.0, 0.7, 8.0);
        assert_eq!(
            costs.choose(DecodePlan::Forward { pictures: 1 }, clock(), true),
            PictureSource::Exact
        );
        assert_eq!(
            costs.choose(DecodePlan::Current, clock(), true),
            PictureSource::Exact
        );
        // 30 ordinals at 0.7 ms fit a 23.3 ms budget; 125 do not.
        assert_eq!(
            costs.choose(DecodePlan::Seek { ordinals: 30 }, clock(), true),
            PictureSource::Exact
        );
        assert_eq!(
            costs.choose(DecodePlan::Seek { ordinals: 125 }, clock(), true),
            PictureSource::Reduced
        );
        assert_eq!(
            costs.choose(DecodePlan::Forward { pictures: 3 }, clock(), true),
            PictureSource::Reduced
        );
    }

    #[test]
    fn unmeasured_seeks_are_reduced_and_unmeasured_steps_exact() {
        let costs = PlaybackPictures::default();
        assert_eq!(
            costs.choose(DecodePlan::Forward { pictures: 1 }, clock(), true),
            PictureSource::Exact
        );
        assert_eq!(
            costs.choose(DecodePlan::Seek { ordinals: 1 }, clock(), true),
            PictureSource::Exact
        );
        assert_eq!(
            costs.choose(DecodePlan::Seek { ordinals: 2 }, clock(), true),
            PictureSource::Reduced
        );
    }

    #[test]
    fn a_sequential_original_that_cannot_keep_up_stays_reduced() {
        let costs = measured(30.0, 0.7, 8.0);
        assert_eq!(
            costs.choose(DecodePlan::Forward { pictures: 1 }, clock(), true),
            PictureSource::Reduced
        );
        assert_eq!(
            costs.reposition_target(&index(600, 250), SourceFrameId(120), clock(), None),
            None
        );
    }

    #[test]
    fn reposition_targets_the_earliest_reachable_picture() {
        let costs = measured(15.0, 0.7, 8.0);
        let index = index(600, 250);
        // Shown 121 ordinals after the keyframe: useful time per ordinal is
        // 33.3 × (1 − 8/33.3) × 0.8 ≈ 20.3 ms, so the lead is
        // ceil(121 × 0.7 / 19.6) + 2 = 7.
        let target = costs
            .reposition_target(&index, SourceFrameId(120), clock(), None)
            .unwrap();
        assert_eq!(target, SourceFrameId(127));
        // A keyframe sooner than that is preferred.
        let target = costs
            .reposition_target(&index, SourceFrameId(247), clock(), None)
            .unwrap();
        assert_eq!(target, SourceFrameId(250));
    }

    #[test]
    fn slow_seeks_wait_for_the_next_keyframe_within_the_lead_limit() {
        let costs = measured(15.0, 25.0, 8.0);
        let index = index(600, 250);
        assert_eq!(
            costs.reposition_target(&index, SourceFrameId(150), clock(), None),
            Some(SourceFrameId(250))
        );
        // No keyframe within four seconds (120 ordinals): nothing to do.
        assert_eq!(
            costs.reposition_target(&index, SourceFrameId(10), clock(), None),
            None
        );
    }

    #[test]
    fn a_decoder_close_ahead_is_kept_and_a_far_one_repositioned() {
        let shown = SourceFrameId(100);
        let ahead = |current, repositioning| PlaybackPictures::ahead(shown, current, repositioning);
        assert_eq!(
            ahead(Some(SourceFrameId(110)), None),
            Some(SourceFrameId(110))
        );
        assert_eq!(
            ahead(None, Some(SourceFrameId(101))),
            Some(SourceFrameId(101))
        );
        assert_eq!(ahead(Some(SourceFrameId(100)), None), None);
        assert_eq!(ahead(Some(SourceFrameId(40)), None), None);
        let costs = measured(15.0, 0.7, 8.0);
        let index = index(600, 250);
        // The earliest reachable picture after 120 is 127 (see below).
        assert_eq!(
            costs.reposition_target(
                &index,
                SourceFrameId(120),
                clock(),
                Some(SourceFrameId(125))
            ),
            None,
            "a decoder at 125 is reached sooner than a reposition"
        );
        assert_eq!(
            costs.reposition_target(
                &index,
                SourceFrameId(120),
                clock(),
                Some(SourceFrameId(144))
            ),
            Some(SourceFrameId(127)),
            "a decoder 24 pictures ahead (a Repeat restart) is repositioned"
        );
    }

    #[test]
    fn short_seeks_do_not_set_the_preroll_cost_and_a_prior_applies_until_measured() {
        let mut costs = PlaybackPictures::default();
        costs.record_exact(
            DecodePlan::Forward { pictures: 1 },
            Duration::from_millis(12),
        );
        // A cold keyframe decode is not a per-ordinal preroll cost.
        costs.record_exact(DecodePlan::Seek { ordinals: 1 }, Duration::from_millis(150));
        assert_eq!(costs.predicted_ms(DecodePlan::Seek { ordinals: 10 }), None);
        // Prior: 3 ms per ordinal (a quarter of a forward picture); useful
        // time per ordinal (33.3 − 10) × 0.8 ≈ 18.7 ms; 21 behind the anchor:
        // ceil(21 × 3 / 15.7) + 2 = 7.
        let target = costs
            .reposition_target(&index(600, 250), SourceFrameId(270), clock(), None)
            .unwrap();
        assert_eq!(target, SourceFrameId(277));
    }

    #[test]
    fn unmeasured_forward_decoding_still_repositions() {
        let mut costs = PlaybackPictures::default();
        costs.record_exact(
            DecodePlan::Seek { ordinals: 100 },
            Duration::from_millis(70),
        );
        // Only the seek is measured (stopped seeks): reposition anyway, so the
        // exact forward pictures after it are tried and measured.
        assert_eq!(
            costs.reposition_target(&index(600, 250), SourceFrameId(120), clock(), None),
            Some(SourceFrameId(127))
        );
        // Nothing measured at all: the next keyframe.
        assert_eq!(
            PlaybackPictures::default().reposition_target(
                &index(600, 250),
                SourceFrameId(200),
                clock(),
                None
            ),
            Some(SourceFrameId(250))
        );
    }

    #[test]
    fn resume_cost_combines_preroll_and_forward_pictures() {
        let costs = measured(15.0, 0.7, 8.0);
        let predicted = costs
            .predicted_ms(DecodePlan::Resume {
                remaining: 10,
                after: 1,
            })
            .unwrap();
        assert!((predicted - 22.0).abs() < 1e-9);
        assert_eq!(
            costs.choose(
                DecodePlan::Resume {
                    remaining: 10,
                    after: 1
                },
                clock(),
                true
            ),
            PictureSource::Exact
        );
    }
}
