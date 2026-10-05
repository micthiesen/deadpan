//! Pure, transactional editing of complete node audio-treatment recipes.
//! Key positions stay in exact owner-output frames, including hidden keys.

use deadpan_core::{
    AudioTreatments, ClipGain, ExactRatio, GainCurve, GainDb, GainEnvelope, GainRange, GainSegment,
    MAX_GAIN_ENVELOPES, MAX_GAIN_MUTE_RANGES, MAX_GAIN_SEGMENTS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainEdit {
    recipe: AudioTreatments,
}

impl GainEdit {
    pub(crate) fn new(recipe: AudioTreatments) -> Self {
        Self { recipe }
    }

    pub(crate) fn recipe(&self) -> &AudioTreatments {
        &self.recipe
    }

    pub(crate) fn clip(&self) -> Option<&ClipGain> {
        self.recipe.clip_gain()
    }

    pub(crate) fn trim(&self) -> GainDb {
        self.clip().map_or(GainDb::UNITY, ClipGain::trim)
    }

    pub(crate) fn muted(&self) -> bool {
        self.clip().is_some_and(ClipGain::muted)
    }

    pub(crate) fn envelopes(&self) -> &[GainEnvelope] {
        self.clip().map_or(&[], ClipGain::envelopes)
    }

    pub(crate) fn mute_ranges(&self) -> &[GainRange] {
        self.clip().map_or(&[], ClipGain::mute_ranges)
    }

    pub(crate) fn set_trim(&mut self, trim: GainDb) -> Result<(), String> {
        let clip = self.clip().cloned().unwrap_or_default().with_trim(trim);
        self.install(clip)
    }

    pub(crate) fn adjust_trim(&mut self, millidecibels: i32) -> Result<(), String> {
        let trim = self.trim().adjusted(millidecibels).map_err(message)?;
        self.set_trim(trim)
    }

    pub(crate) fn set_muted(&mut self, muted: bool) -> Result<(), String> {
        let clip = ClipGain::new(
            self.trim(),
            muted,
            self.envelopes().to_vec(),
            self.mute_ranges().to_vec(),
        )
        .map_err(message)?;
        self.install(clip)
    }

    pub(crate) fn add_envelope(&mut self, envelope: GainEnvelope) -> Result<usize, String> {
        if self.envelopes().len() == MAX_GAIN_ENVELOPES {
            return Err("A beat supports at most 16 gain envelopes.".into());
        }
        let mut envelopes = self.envelopes().to_vec();
        let index = envelopes.len();
        envelopes.push(envelope);
        self.install_envelopes(envelopes)?;
        Ok(index)
    }

    pub(crate) fn remove_envelope(&mut self, index: usize) -> Result<(), String> {
        self.envelope(index)?;
        let mut envelopes = self.envelopes().to_vec();
        envelopes.remove(index);
        self.install_envelopes(envelopes)
    }

    /// Move the two boundary keys explicitly. Interior keys, including keys
    /// outside the current visible owner duration, keep their exact positions.
    pub(crate) fn set_envelope_range(
        &mut self,
        index: usize,
        range: GainRange,
    ) -> Result<(), String> {
        let envelope = self.envelope(index)?;
        let mut segments = envelope.segments().to_vec();
        let last = segments.last_mut().ok_or_else(no_envelope)?;
        *last = GainSegment::new(range.end(), last.value(), last.curve()).map_err(message)?;
        let replacement = GainEnvelope::new(envelope.clock(), range, envelope.initial(), segments)
            .map_err(|_| {
                "The range must retain every interior key in its existing order.".to_owned()
            })?;
        self.replace_envelope(index, replacement)
    }

    /// Key zero is the initial value at range start and has no incoming curve.
    /// Every later key ends one segment and owns that segment's incoming curve.
    pub(crate) fn key(
        &self,
        envelope: usize,
        key: usize,
    ) -> Result<(ExactRatio, GainDb, Option<GainCurve>), String> {
        let envelope = self.envelope(envelope)?;
        if key == 0 {
            return Ok((envelope.range().start(), envelope.initial(), None));
        }
        let segment = envelope.segments().get(key - 1).ok_or_else(no_key)?;
        Ok((segment.end(), segment.value(), Some(segment.curve())))
    }

    pub(crate) fn set_key(
        &mut self,
        envelope: usize,
        key: usize,
        time: ExactRatio,
        value: GainDb,
        curve: Option<GainCurve>,
    ) -> Result<(), String> {
        let original = self.envelope(envelope)?;
        let mut range = original.range();
        let mut initial = original.initial();
        let mut segments = original.segments().to_vec();
        if key == 0 {
            if curve.is_some() {
                return Err("The initial key has no incoming curve.".into());
            }
            range = GainRange::new(time, range.end()).map_err(message)?;
            initial = value;
        } else {
            let last = key == segments.len();
            let segment = segments.get_mut(key - 1).ok_or_else(no_key)?;
            let curve = curve.ok_or_else(|| "An ending key needs an incoming curve.".to_owned())?;
            *segment = GainSegment::new(time, value, curve).map_err(message)?;
            if last {
                range = GainRange::new(range.start(), time).map_err(message)?;
            }
        }
        let replacement =
            GainEnvelope::new(original.clock(), range, initial, segments).map_err(message)?;
        self.replace_envelope(envelope, replacement)
    }

    /// Insert a distinct interior key. Its incoming curve and the original
    /// ending key's incoming curve split the containing segment. This explicit
    /// edit may reshape that segment, while all other segments remain unchanged.
    pub(crate) fn insert_key(
        &mut self,
        envelope: usize,
        time: ExactRatio,
        value: GainDb,
        curve: GainCurve,
    ) -> Result<usize, String> {
        let original = self.envelope(envelope)?;
        if original.segments().len() == MAX_GAIN_SEGMENTS {
            return Err("An envelope supports at most 64 segments.".into());
        }
        if time == original.range().start()
            || !original.range().contains(time)
            || original
                .segments()
                .iter()
                .any(|segment| segment.end() == time)
        {
            return Err("Insert a distinct key strictly inside the envelope range.".into());
        }
        // The range validator compares full-width exact ratios without an
        // overflowing subtraction or cross product. Both positions are positive.
        let index = original
            .segments()
            .iter()
            .position(|segment| GainRange::new(time, segment.end()).is_ok())
            .ok_or_else(no_key)?;
        let mut segments = original.segments().to_vec();
        segments.insert(
            index,
            GainSegment::new(time, value, curve).map_err(message)?,
        );
        let replacement = GainEnvelope::new(
            original.clock(),
            original.range(),
            original.initial(),
            segments,
        )
        .map_err(message)?;
        self.replace_envelope(envelope, replacement)?;
        Ok(index + 1)
    }

    /// Remove only an interior key. The following ending key keeps its complete
    /// incoming curve, including both cubic controls, across the merged segment.
    pub(crate) fn remove_key(&mut self, envelope: usize, key: usize) -> Result<(), String> {
        let original = self.envelope(envelope)?;
        if key == 0 || key >= original.segments().len() {
            return Err("Only interior keys can be removed; retain the range endpoints.".into());
        }
        let mut segments = original.segments().to_vec();
        segments.remove(key - 1);
        let replacement = GainEnvelope::new(
            original.clock(),
            original.range(),
            original.initial(),
            segments,
        )
        .map_err(message)?;
        self.replace_envelope(envelope, replacement)
    }

    pub(crate) fn add_mute_range(&mut self, range: GainRange) -> Result<usize, String> {
        if self.mute_ranges().len() == MAX_GAIN_MUTE_RANGES {
            return Err("A beat supports at most 64 mute ranges.".into());
        }
        let mut ranges = self.mute_ranges().to_vec();
        let index = ranges.len();
        ranges.push(range);
        self.install_mute_ranges(ranges)?;
        Ok(index)
    }

    pub(crate) fn set_mute_range(&mut self, index: usize, range: GainRange) -> Result<(), String> {
        let mut ranges = self.mute_ranges().to_vec();
        let selected = ranges.get_mut(index).ok_or_else(no_mute_range)?;
        *selected = range;
        self.install_mute_ranges(ranges)
    }

    pub(crate) fn remove_mute_range(&mut self, index: usize) -> Result<(), String> {
        if index >= self.mute_ranges().len() {
            return Err(no_mute_range());
        }
        let mut ranges = self.mute_ranges().to_vec();
        ranges.remove(index);
        self.install_mute_ranges(ranges)
    }

    fn envelope(&self, index: usize) -> Result<&GainEnvelope, String> {
        self.envelopes().get(index).ok_or_else(no_envelope)
    }

    fn replace_envelope(&mut self, index: usize, envelope: GainEnvelope) -> Result<(), String> {
        let mut envelopes = self.envelopes().to_vec();
        let selected = envelopes.get_mut(index).ok_or_else(no_envelope)?;
        *selected = envelope;
        self.install_envelopes(envelopes)
    }

    fn install_envelopes(&mut self, envelopes: Vec<GainEnvelope>) -> Result<(), String> {
        let clip = ClipGain::new(
            self.trim(),
            self.muted(),
            envelopes,
            self.mute_ranges().to_vec(),
        )
        .map_err(message)?;
        self.install(clip)
    }

    fn install_mute_ranges(&mut self, ranges: Vec<GainRange>) -> Result<(), String> {
        let clip = ClipGain::new(self.trim(), self.muted(), self.envelopes().to_vec(), ranges)
            .map_err(message)?;
        self.install(clip)
    }

    fn install(&mut self, clip: ClipGain) -> Result<(), String> {
        self.recipe = self.recipe.with_clip_gain(clip).map_err(message)?;
        Ok(())
    }

    pub(crate) fn saturation(&self) -> Option<deadpan_core::Saturation> {
        self.recipe.saturation()
    }

    /// Add, change or remove the saturation stage, keeping clip gain and the
    /// authored stage order.
    pub(crate) fn set_saturation(
        &mut self,
        saturation: Option<deadpan_core::Saturation>,
    ) -> Result<(), String> {
        self.recipe = self.recipe.with_saturation(saturation).map_err(message)?;
        Ok(())
    }
}

fn no_envelope() -> String {
    "Choose an existing gain envelope.".into()
}
fn no_key() -> String {
    "Choose an existing gain key.".into()
}
fn no_mute_range() -> String {
    "Choose an existing mute range.".into()
}
fn message(error: deadpan_core::GainError) -> String {
    error.to_string()
}

/// Parse exact dB using integer thousandths, with no rounding or float path.
pub(crate) fn parse_db(input: &str) -> Result<GainDb, String> {
    let invalid = || "Enter exact dB from -96 to 24 with at most three decimal places.".to_owned();
    if input.len() > 64 {
        return Err(invalid());
    }
    let input = input.trim();
    let (negative, magnitude) = match input.strip_prefix('-') {
        Some(magnitude) => (true, magnitude),
        None => (false, input.strip_prefix('+').unwrap_or(input)),
    };
    let (whole, fraction) = decimal_parts(magnitude).ok_or_else(invalid)?;
    if fraction.len() > 3 {
        return Err(invalid());
    }
    let scale = match fraction.len() {
        0 => 0,
        1 => 100,
        2 => 10,
        _ => 1,
    };
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i32>().map_err(|_| invalid())? * scale
    };
    let magnitude = whole
        .parse::<i32>()
        .ok()
        .and_then(|whole| whole.checked_mul(1000))
        .and_then(|whole| whole.checked_add(fractional))
        .ok_or_else(invalid)?;
    let value = if negative {
        magnitude.checked_neg().ok_or_else(invalid)?
    } else {
        magnitude
    };
    GainDb::new(value).map_err(|_| invalid())
}

pub(crate) fn format_db(gain: GainDb) -> String {
    let value = gain.millidecibels();
    let magnitude = value.unsigned_abs();
    let sign = if value < 0 { "-" } else { "" };
    let whole = magnitude / 1000;
    let fraction = magnitude % 1000;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        let fraction = format!("{fraction:03}");
        format!("{sign}{whole}.{}", fraction.trim_end_matches('0'))
    }
}

/// Parse a nonnegative whole, decimal or integer-ratio owner-frame coordinate.
/// Input length and every arithmetic operation are bounded before allocation.
pub(crate) fn parse_frames(input: &str) -> Result<ExactRatio, String> {
    let invalid = || {
        "Enter nonnegative owner frames as an exact number or ratio, for example 1.25 or 5/4."
            .to_owned()
    };
    if input.len() > 128 {
        return Err(invalid());
    }
    let input = input.trim();
    let input = input.strip_prefix('+').unwrap_or(input);
    if let Some((numerator, denominator)) = input.split_once('/') {
        if !digits(numerator) || !digits(denominator) {
            return Err(invalid());
        }
        let numerator = numerator.parse::<i128>().map_err(|_| invalid())?;
        let denominator = denominator.parse::<i128>().map_err(|_| invalid())?;
        return ExactRatio::new(numerator, denominator).map_err(|_| invalid());
    }
    let (whole, fraction) = decimal_parts(input).ok_or_else(invalid)?;
    let whole = whole.parse::<i128>().map_err(|_| invalid())?;
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        return ExactRatio::new(whole, 1).map_err(|_| invalid());
    }
    let exponent = u32::try_from(fraction.len()).map_err(|_| invalid())?;
    let denominator = 10i128.checked_pow(exponent).ok_or_else(invalid)?;
    let numerator = fraction.parse::<i128>().map_err(|_| invalid())?;
    let fractional = ExactRatio::new(numerator, denominator).map_err(|_| invalid())?;
    ExactRatio::new(whole, 1)
        .and_then(|whole| whole.checked_add(fractional))
        .map_err(|_| invalid())
}

pub(crate) fn format_frames(frames: ExactRatio) -> String {
    if frames.denominator() == 1 {
        frames.numerator().to_string()
    } else {
        format!("{}/{}", frames.numerator(), frames.denominator())
    }
}

fn decimal_parts(input: &str) -> Option<(&str, &str)> {
    let (whole, fraction) = match input.split_once('.') {
        Some((whole, fraction)) if digits(fraction) => (whole, fraction),
        Some(_) => return None,
        None => (input, ""),
    };
    digits(whole).then_some((whole, fraction))
}

fn digits(input: &str) -> bool {
    !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{AudioTreatmentStage, GainClock, Saturation};

    #[test]
    fn saturation_is_a_separate_ordered_stage_beside_clip_gain() {
        let mut edit = GainEdit::new(AudioTreatments::default());
        let drive = Saturation::new(GainDb::new(12_000).unwrap()).unwrap();
        edit.set_saturation(Some(drive)).unwrap();
        assert_eq!(edit.recipe().order(), &[AudioTreatmentStage::Saturation]);
        assert!(edit.clip().is_none());
        // Clip gain added later keeps its default place before saturation.
        edit.set_trim(GainDb::new(-3_000).unwrap()).unwrap();
        assert_eq!(
            edit.recipe().order(),
            &[
                AudioTreatmentStage::ClipGain,
                AudioTreatmentStage::Saturation
            ]
        );
        assert_eq!(edit.saturation(), Some(drive));
        edit.set_saturation(None).unwrap();
        assert_eq!(edit.recipe().order(), &[AudioTreatmentStage::ClipGain]);
        assert_eq!(edit.trim().millidecibels(), -3_000);
        assert!(Saturation::new(GainDb::new(24_000).unwrap()).is_ok());
        assert!(Saturation::new(GainDb::new(-1).unwrap()).is_err());
    }

    fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
        ExactRatio::new(numerator, denominator).unwrap()
    }
    fn db(value: i32) -> GainDb {
        GainDb::new(value).unwrap()
    }
    fn range(start: i64, end: i64) -> GainRange {
        GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap()
    }
    fn cubic() -> GainCurve {
        GainCurve::Cubic {
            control1: db(23_000),
            control2: db(-93_000),
        }
    }
    fn envelope() -> GainEnvelope {
        GainEnvelope::new(
            GainClock::OwnerOutput,
            range(1, 100),
            db(-12_000),
            vec![
                GainSegment::new(ratio(5, 2), db(3_000), GainCurve::Step).unwrap(),
                GainSegment::new(ratio(17, 3), db(6_000), cubic()).unwrap(),
                GainSegment::new(ExactRatio::integer(100), db(-6_000), GainCurve::Smoothstep)
                    .unwrap(),
            ],
        )
        .unwrap()
    }
    fn fixture() -> GainEdit {
        GainEdit::new(AudioTreatments::from_clip_gain(
            ClipGain::new(
                db(-3_000),
                true,
                vec![envelope(), envelope()],
                vec![range(4, 6), range(90, 99)],
            )
            .unwrap(),
        ))
    }

    #[test]
    fn configured_unity_and_absent_treatment_remain_distinct() {
        let mut empty = GainEdit::new(AudioTreatments::default());
        assert!(empty.recipe().is_empty());
        assert!(empty.clip().is_none());
        assert_eq!(empty.trim(), GainDb::UNITY);
        assert!(!empty.muted());
        empty.set_trim(GainDb::UNITY).unwrap();
        assert!(!empty.recipe().is_empty());
        assert_eq!(empty.recipe().order(), &[AudioTreatmentStage::ClipGain]);
        let configured = empty.recipe().clone();
        empty.add_envelope(envelope()).unwrap();
        empty.remove_envelope(0).unwrap();
        assert_eq!(empty.recipe(), &configured);
        let mut muted = GainEdit::new(AudioTreatments::default());
        muted.set_muted(true).unwrap();
        assert!(muted.muted());
        assert_eq!(muted.trim(), GainDb::UNITY);
        muted.set_muted(false).unwrap();
        assert_eq!(muted.recipe(), &configured);
    }

    #[test]
    fn trim_and_true_mute_preserve_all_envelopes_controls_and_ranges() {
        let mut edited = fixture();
        let before = edited.clone();
        edited.adjust_trim(3000).unwrap();
        edited.set_muted(false).unwrap();
        assert_eq!(edited.trim(), GainDb::UNITY);
        assert!(!edited.muted());
        assert_eq!(edited.envelopes(), before.envelopes());
        assert_eq!(edited.mute_ranges(), before.mute_ranges());
        edited.set_trim(db(-96_000)).unwrap();
        assert!(
            !edited.muted(),
            "finite minimum gain is not a mute sentinel"
        );
        let before = edited.clone();
        for delta in [-1, i32::MIN, i32::MAX] {
            assert!(edited.adjust_trim(delta).is_err());
            assert_eq!(edited, before);
        }
    }

    #[test]
    fn exact_keys_keep_incoming_curves_and_leave_other_envelopes_intact() {
        let mut edited = fixture();
        let other = edited.envelopes()[1].clone();
        assert_eq!(
            edited.key(0, 0).unwrap(),
            (ExactRatio::ONE, db(-12_000), None)
        );
        assert_eq!(
            edited.key(0, 2).unwrap(),
            (ratio(17, 3), db(6_000), Some(cubic()))
        );
        edited
            .set_key(0, 1, ratio(8, 3), db(9_000), Some(GainCurve::Linear))
            .unwrap();
        assert_eq!(
            edited.key(0, 2).unwrap(),
            (ratio(17, 3), db(6_000), Some(cubic()))
        );
        edited.set_key(0, 0, ratio(1, 3), db(-9_000), None).unwrap();
        edited
            .set_key(0, 3, ratio(601, 6), db(-3_000), Some(GainCurve::Smoothstep))
            .unwrap();
        assert_eq!(
            edited.envelopes()[0].range(),
            GainRange::new(ratio(1, 3), ratio(601, 6)).unwrap()
        );
        assert_eq!(edited.envelopes()[1], other);
        assert_eq!(edited.mute_ranges(), fixture().mute_ranges());
    }

    #[test]
    fn insert_and_remove_reshape_only_affected_segments_and_retain_cubic_controls() {
        let mut edited = fixture();
        let before = edited.envelopes()[0].clone();
        let inserted = edited
            .insert_key(0, ratio(7, 2), db(0), GainCurve::Linear)
            .unwrap();
        assert_eq!(inserted, 2);
        let after = &edited.envelopes()[0];
        assert_eq!(after.segments()[0], before.segments()[0]);
        assert_eq!(after.segments()[2], before.segments()[1]);
        assert_eq!(after.segments()[3], before.segments()[2]);
        assert_eq!(after.segments()[2].curve(), cubic());
        for position in [ratio(5, 4), ratio(7, 1), ratio(90, 1)] {
            assert_eq!(
                after.evaluate(position).unwrap(),
                before.evaluate(position).unwrap()
            );
        }
        edited.remove_key(0, inserted).unwrap();
        assert_eq!(edited.envelopes()[0], before);
        assert_eq!(edited.envelopes()[1], fixture().envelopes()[1]);
    }

    #[test]
    fn invalid_edits_are_atomic_and_range_changes_do_not_drop_hidden_keys() {
        let mut edited = fixture();
        let before = edited.clone();
        assert!(edited.set_envelope_range(0, range(1, 5)).is_err());
        assert_eq!(edited, before);
        assert!(edited.set_envelope_range(0, range(3, 100)).is_err());
        assert_eq!(edited, before);
        assert!(
            edited
                .set_key(0, 1, ratio(17, 3), db(0), Some(GainCurve::Linear))
                .is_err()
        );
        assert_eq!(edited, before);
        assert!(
            edited
                .set_key(0, 0, ExactRatio::ONE, db(0), Some(cubic()))
                .is_err()
        );
        assert_eq!(edited, before);
        assert!(edited.set_key(0, 1, ratio(5, 2), db(0), None).is_err());
        assert_eq!(edited, before);
        for position in [
            ExactRatio::ONE,
            ratio(5, 2),
            ExactRatio::integer(100),
            ratio(-1, 2),
        ] {
            assert!(edited.insert_key(0, position, db(0), cubic()).is_err());
            assert_eq!(edited, before);
        }
        for key in [0, 3, usize::MAX] {
            assert!(edited.remove_key(0, key).is_err());
            assert_eq!(edited, before);
        }
        assert!(edited.remove_envelope(usize::MAX).is_err());
        assert!(edited.remove_mute_range(usize::MAX).is_err());
        assert!(edited.set_mute_range(usize::MAX, range(0, 1)).is_err());
        assert!(edited.key(usize::MAX, 0).is_err());
        assert!(edited.key(0, usize::MAX).is_err());
        assert_eq!(edited, before);
        edited.set_envelope_range(0, range(0, 120)).unwrap();
        assert_eq!(
            &edited.envelopes()[0].segments()[..2],
            &before.envelopes()[0].segments()[..2]
        );
    }

    #[test]
    fn mute_ranges_are_exact_independent_and_bounded() {
        let mut edited = fixture();
        let envelopes = edited.envelopes().to_vec();
        let precise = GainRange::new(ratio(1, 3000), ratio(7, 4800)).unwrap();
        let index = edited.add_mute_range(precise).unwrap();
        assert_eq!(index, 2);
        assert_eq!(edited.mute_ranges()[index], precise);
        edited.set_mute_range(0, precise).unwrap();
        assert_eq!(edited.mute_ranges()[1], range(90, 99));
        edited.remove_mute_range(index).unwrap();
        assert_eq!(edited.envelopes(), envelopes);
        while edited.mute_ranges().len() < MAX_GAIN_MUTE_RANGES {
            edited.add_mute_range(precise).unwrap();
        }
        let before = edited.clone();
        assert!(edited.add_mute_range(precise).is_err());
        assert_eq!(edited, before);
    }

    #[test]
    fn collection_limits_and_wide_key_comparisons_are_checked() {
        let mut edited = fixture();
        while edited.envelopes().len() < MAX_GAIN_ENVELOPES {
            edited.add_envelope(envelope()).unwrap();
        }
        let before = edited.clone();
        assert!(edited.add_envelope(envelope()).is_err());
        assert_eq!(edited, before);
        let mut edited = GainEdit::new(AudioTreatments::default());
        let envelope = GainEnvelope::new(
            GainClock::OwnerOutput,
            range(0, 64),
            db(0),
            (1..=64)
                .map(|end| {
                    GainSegment::new(ExactRatio::integer(end), db(0), GainCurve::Step).unwrap()
                })
                .collect(),
        )
        .unwrap();
        edited.add_envelope(envelope).unwrap();
        let before = edited.clone();
        assert!(
            edited
                .insert_key(0, ratio(1, 2), db(0), GainCurve::Step)
                .is_err()
        );
        assert_eq!(edited, before);
        let mut edited = GainEdit::new(AudioTreatments::default());
        let start = ratio(i128::MAX - 4, i128::MAX);
        let end = ratio(i128::MAX - 1, i128::MAX);
        edited
            .add_envelope(
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(start, end).unwrap(),
                    db(0),
                    vec![GainSegment::new(end, db(1000), cubic()).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        let middle = ratio(i128::MAX - 2, i128::MAX);
        assert_eq!(
            edited
                .insert_key(0, middle, db(500), GainCurve::Linear)
                .unwrap(),
            1
        );
        assert_eq!(edited.key(0, 2).unwrap(), (end, db(1000), Some(cubic())));
    }

    #[test]
    fn decimal_db_parser_has_no_rounding_and_formats_losslessly() {
        for (input, expected) in [
            ("-96", -96_000),
            ("+24.000", 24_000),
            ("-0.001", -1),
            ("3.14", 3140),
            (" 0.010 ", 10),
            ("-0", 0),
        ] {
            let parsed = parse_db(input).unwrap();
            assert_eq!(parsed, db(expected));
            assert_eq!(parse_db(&format_db(parsed)).unwrap(), parsed);
        }
        for input in [
            "",
            "24.001",
            "-96.001",
            "0.0001",
            "1e1",
            "NaN",
            "Infinity",
            "1/2",
            ".5",
            "1.",
            "--1",
            "1 2",
            "−3",
            "1.2.3",
            "2147483647",
        ] {
            assert!(parse_db(input).is_err(), "{input}");
        }
        assert!(parse_db(&"0".repeat(65)).is_err());
        assert_eq!(format_db(db(-30)), "-0.03");
    }

    #[test]
    fn frame_parser_is_exact_bounded_and_handles_full_width_ratios() {
        for (input, expected) in [
            ("1.25", ratio(5, 4)),
            ("+5/4", ratio(5, 4)),
            ("30000/48048000", ratio(30000, 48048000)),
            ("0.000", ExactRatio::ZERO),
            (" 12 ", ExactRatio::integer(12)),
            ("01/02", ratio(1, 2)),
        ] {
            let parsed = parse_frames(input).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parse_frames(&format_frames(parsed)).unwrap(), parsed);
        }
        let wide = ratio(i128::MAX - 1, i128::MAX);
        assert_eq!(parse_frames(&format_frames(wide)).unwrap(), wide);
        assert_eq!(
            parse_frames(&i128::MAX.to_string()).unwrap(),
            ratio(i128::MAX, 1)
        );
        for input in [
            "", "-0", "-1", "1/0", "1/-2", "1/+2", "1/2/3", ".5", "1.", "1e3", "NaN", "1 / 2",
            "1.2.3", "1.5/2",
        ] {
            assert!(parse_frames(input).is_err(), "{input}");
        }
        assert!(parse_frames(&format!("{}.1", i128::MAX)).is_err());
        assert!(parse_frames(&format!("0.{}1", "0".repeat(38))).is_err());
        assert!(parse_frames(&"0".repeat(129)).is_err());
    }
}
