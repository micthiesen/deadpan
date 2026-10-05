//! `:repeat [N] gap=120ms [gap-step=-40ms] gain-step=3dB zoom-step=0.08
//! [progression=multiply]`: change a selected Repeat's plays, gaps and
//! per-play escalation together, or wrap a plain beat first.

use deadpan_core::{ExactRatio, FrameRate, GainDb, PauseLength, ZoomProgression, ZoomStep};

use super::duration::DurationInput;

pub const USAGE: &str = "Use :repeat 3 gap=120ms gain-step=3dB zoom-step=0.08 on a selected beat or Repeat. gap= sets every gap (gap=0 removes them all); gap-step=-40ms makes each later gap 40 ms shorter. progression=multiply compounds the zoom; gain-step=0dB or zoom-step=0 removes that step.";

/// The most gaps one `gap-step` ladder authors, matching the core bound.
const MAX_GAPS: u32 = deadpan_core::MAX_SEMANTIC_REPEAT_GAPS as u32;

/// Requested gaps: the first gap, and how much each later gap adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GapInput {
    pub first: DurationInput,
    /// `(shorter, amount)`: each later gap is `amount` shorter or longer.
    pub step: Option<(bool, DurationInput)>,
}

impl GapInput {
    /// Exact gap lengths for a Repeat of `plays`, each rounded once. A zero
    /// first gap without a step removes the gaps.
    pub fn lengths(self, plays: u32, rate: FrameRate) -> Result<Vec<PauseLength>, String> {
        let zero = match self.first {
            DurationInput::Frames(frames) => frames == deadpan_core::FrameDuration::ZERO,
            DurationInput::Seconds(seconds) => seconds == ExactRatio::ZERO,
        };
        let Some((shorter, step)) = self.step else {
            return if zero {
                Ok(Vec::new())
            } else {
                Ok(vec![self.first.pause_length(rate)?])
            };
        };
        let count = plays.saturating_sub(1).max(1);
        if count > MAX_GAPS {
            return Err(format!(
                "A gap-step ladder sets at most {MAX_GAPS} gaps ({} plays); use fewer plays or a single gap= without gap-step.",
                MAX_GAPS + 1
            ));
        }
        (0..count)
            .map(|index| {
                let value = match (self.first, step) {
                    (DurationInput::Frames(first), DurationInput::Frames(step)) => {
                        let offset = i64::from(index)
                            .checked_mul(step.frames())
                            .ok_or("gap-step overflows")?;
                        let frames = if shorter {
                            first.frames().checked_sub(offset)
                        } else {
                            first.frames().checked_add(offset)
                        }
                        .filter(|frames| *frames > 0)
                        .ok_or("Every gap must stay positive; use a smaller gap-step.")?;
                        DurationInput::Frames(
                            deadpan_core::FrameDuration::new(frames).map_err(|e| e.to_string())?,
                        )
                    }
                    (DurationInput::Seconds(first), DurationInput::Seconds(step)) => {
                        let offset = step
                            .checked_mul(ExactRatio::integer(i64::from(index)))
                            .map_err(|e| e.to_string())?;
                        let seconds = if shorter {
                            first.checked_sub(offset)
                        } else {
                            first.checked_add(offset)
                        }
                        .map_err(|e| e.to_string())?;
                        if seconds.compare_integer(0).is_le() {
                            return Err(
                                "Every gap must stay positive; use a smaller gap-step.".into()
                            );
                        }
                        DurationInput::Seconds(seconds)
                    }
                    _ => return Err("Give gap and gap-step in the same unit.".into()),
                };
                value.pause_length(rate)
            })
            .collect()
    }
}

/// Requested Repeat change. Unnamed parameters keep their current values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EscalationInput {
    /// Total plays: a new count for a Repeat, or the plays of a new wrapper.
    pub plays: Option<u32>,
    pub gap: Option<GapInput>,
    pub gain_step: Option<GainDb>,
    /// A grid-rounded zoom step; `Some(None)` removes the zoom progression.
    pub zoom_step: Option<Option<ExactRatio>>,
    pub progression: Option<ZoomProgression>,
}

impl EscalationInput {
    /// Whether the command names any escalation parameter.
    pub fn changes_escalation(&self) -> bool {
        self.gain_step.is_some() || self.zoom_step.is_some() || self.progression.is_some()
    }

    /// Merge into a Repeat's current escalation; `None` when nothing remains.
    pub fn apply(
        self,
        current: Option<deadpan_core::RepeatEscalation>,
    ) -> Result<Option<deadpan_core::RepeatEscalation>, String> {
        let current = current.unwrap_or_default();
        let step = match self.zoom_step {
            Some(step) => step,
            None => current.zoom.map(|zoom| zoom.step),
        };
        let progression = self
            .progression
            .or(current.zoom.map(|zoom| zoom.progression))
            .unwrap_or_default();
        let zoom = match (step, self.progression) {
            (Some(step), _) => Some(ZoomStep { step, progression }),
            (None, Some(_)) => {
                return Err("progression needs a zoom-step; this Repeat has none.".into());
            }
            (None, None) => None,
        };
        let result = deadpan_core::RepeatEscalation {
            gain_step: self.gain_step.unwrap_or(current.gain_step),
            zoom,
        };
        Ok((result.gain_step != GainDb::UNITY || result.zoom.is_some()).then_some(result))
    }

    /// Inspector rows: the gain and zoom added per play, or no escalation.
    pub fn fields(
        escalation: Option<&deadpan_core::RepeatEscalation>,
    ) -> Vec<(&'static str, String)> {
        let Some(escalation) = escalation else {
            return vec![("Escalation", "None".into())];
        };
        let mut fields = Vec::new();
        if escalation.gain_step != GainDb::UNITY {
            fields.push((
                "Gain per play",
                format!(
                    "{:+} dB",
                    f64::from(escalation.gain_step.millidecibels()) / 1000.0
                ),
            ));
        }
        if let Some(zoom) = escalation.zoom {
            let step = zoom.step.numerator() as f64 / zoom.step.denominator() as f64;
            fields.push((
                "Zoom per play",
                match zoom.progression {
                    ZoomProgression::Add => format!("{step:+.3}"),
                    ZoomProgression::Multiply => format!("×{step:.3}"),
                },
            ));
        }
        fields
    }
}

/// Parse the words after `repeat`. Returns `None` for a plain count, which
/// keeps its ordinary wrap-or-set meaning.
pub fn parse(arguments: &[&str]) -> Result<Option<EscalationInput>, String> {
    let (plays, parameters) = match arguments.split_first() {
        Some((first, rest)) if first.bytes().all(|byte| byte.is_ascii_digit()) => {
            let plays = first
                .parse::<u32>()
                .ok()
                .filter(|plays| *plays > 0)
                .ok_or("Total plays must be a positive integer.")?;
            (Some(plays), rest)
        }
        _ => (None, arguments),
    };
    if parameters.is_empty() {
        return Ok(None);
    }
    let mut input = EscalationInput {
        plays,
        gap: None,
        gain_step: None,
        zoom_step: None,
        progression: None,
    };
    let mut zoom_step = None;
    let mut progression = None;
    let mut gap = None;
    let mut gap_step = None;
    for parameter in parameters {
        let (key, value) = parameter.split_once('=').ok_or(USAGE)?;
        match key {
            "gain-step" if input.gain_step.is_none() => {
                input.gain_step = Some(gain(value)?);
            }
            "zoom-step" if zoom_step.is_none() => zoom_step = Some(decimal(value)?),
            "progression" if progression.is_none() => {
                progression = Some(match value {
                    "add" => ZoomProgression::Add,
                    "multiply" => ZoomProgression::Multiply,
                    _ => return Err("progression is add or multiply.".into()),
                });
            }
            "gap" if gap.is_none() => {
                gap = Some(if value == "0" {
                    DurationInput::Frames(deadpan_core::FrameDuration::ZERO)
                } else {
                    DurationInput::parse(value)?
                });
            }
            "gap-step" if gap_step.is_none() => {
                let (shorter, amount) = match value.strip_prefix('-') {
                    Some(amount) => (true, amount),
                    None => (false, value.strip_prefix('+').unwrap_or(value)),
                };
                gap_step = Some((shorter, DurationInput::parse(amount)?));
            }
            "gain-step" | "zoom-step" | "progression" | "gap" | "gap-step" => {
                return Err(format!("{key} is given twice."));
            }
            _ => return Err(format!("Unknown :repeat parameter {key}. {USAGE}")),
        }
    }
    input.progression = progression;
    input.gap = match (gap, gap_step) {
        (Some(first), step) => Some(GapInput { first, step }),
        (None, Some(_)) => {
            return Err(
                "gap-step needs a first gap, for example gap=500ms gap-step=-200ms.".into(),
            );
        }
        (None, None) => None,
    };
    if let Some(step) = zoom_step {
        // Zero (or ×1 when multiplying) removes the zoom progression.
        let unchanged = match progression.unwrap_or_default() {
            ZoomProgression::Add => step == ExactRatio::ZERO,
            ZoomProgression::Multiply => step == ExactRatio::ONE,
        };
        input.zoom_step = Some(if unchanged {
            None
        } else {
            let rounded = deadpan_core::quantize_zoom_step(step).map_err(|e| e.to_string())?;
            if rounded == ExactRatio::ZERO {
                return Err("zoom-step is too small to change the picture.".into());
            }
            Some(rounded)
        });
    }
    Ok(Some(input))
}

/// `3dB`, `-1.5dB` or `0dB`, at most three decimals, as exact millidecibels.
fn gain(value: &str) -> Result<GainDb, String> {
    let number = value
        .len()
        .checked_sub(2)
        .filter(|split| {
            value.is_char_boundary(*split) && value[*split..].eq_ignore_ascii_case("db")
        })
        .map(|split| &value[..split])
        .ok_or("Write gain-step in decibels, for example gain-step=3dB.")?;
    let millidecibels = decimal(number)?
        .checked_mul(ExactRatio::integer(1000))
        .map_err(|error| error.to_string())?;
    if millidecibels.denominator() != 1 {
        return Err("gain-step allows at most three decimals.".into());
    }
    i32::try_from(millidecibels.numerator())
        .ok()
        .and_then(|value| GainDb::new(value).ok())
        .ok_or_else(|| "gain-step must lie between -96 dB and +24 dB.".into())
}

/// A plain signed decimal such as `0.08`, `-0.25` or `1.5`.
fn decimal(value: &str) -> Result<ExactRatio, String> {
    let invalid = || format!("{value} is not a decimal number.");
    let (negative, digits) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || whole.len() + fraction.len() > 18
    {
        return Err(invalid());
    }
    let numerator: i128 = format!("{whole}{fraction}")
        .parse()
        .map_err(|_| invalid())?;
    let denominator = 10_i128.pow(fraction.len() as u32);
    ExactRatio::new(if negative { -numerator } else { numerator }, denominator)
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worked_example_parses_and_plain_counts_stay_ordinary() {
        let input = parse(&["3", "gain-step=3dB", "zoom-step=0.08"])
            .unwrap()
            .unwrap();
        assert_eq!(input.plays, Some(3));
        assert_eq!(input.gain_step, Some(GainDb::new(3000).unwrap()));
        assert_eq!(input.progression, None);
        assert_eq!(
            input.zoom_step,
            Some(Some(
                deadpan_core::quantize_zoom_step(ExactRatio::new(8, 100).unwrap()).unwrap()
            ))
        );
        assert_eq!(
            parse(&["gain-step=2DB"]).unwrap().unwrap().gain_step,
            Some(GainDb::new(2000).unwrap())
        );
        assert_eq!(parse(&["3"]).unwrap(), None);
        assert_eq!(
            parse(&["gain-step=-1.5dB"]).unwrap().unwrap().gain_step,
            Some(GainDb::new(-1500).unwrap())
        );
    }

    #[test]
    fn zero_steps_clear_and_bad_input_explains() {
        let cleared = parse(&["zoom-step=0", "gain-step=0dB"]).unwrap().unwrap();
        assert_eq!(cleared.zoom_step, Some(None));
        assert_eq!(cleared.apply(None), Ok(None));
        let multiply = parse(&["zoom-step=1.5", "progression=multiply"])
            .unwrap()
            .unwrap()
            .apply(None)
            .unwrap()
            .unwrap();
        assert_eq!(
            multiply.zoom.unwrap().progression,
            ZoomProgression::Multiply
        );
        // progression alone changes an existing zoom step's progression.
        let switched = parse(&["progression=add"])
            .unwrap()
            .unwrap()
            .apply(Some(multiply))
            .unwrap()
            .unwrap();
        assert_eq!(switched.zoom.unwrap().progression, ZoomProgression::Add);
        assert!(
            parse(&["progression=multiply"])
                .unwrap()
                .unwrap()
                .apply(None)
                .is_err()
        );
        for bad in [
            vec!["gap=120"],
            vec!["gap=1ms", "gap=2ms"],
            vec!["gap-step=-1f"],
            vec!["gain-step=3"],
            vec!["gain-step=0.0005dB"],
            vec!["gain-step=30dB"],
            vec!["zoom-step=0.0000000000001"],
            vec!["zoom-step=abc"],
            vec!["volume=3"],
            vec!["0", "gain-step=3dB"],
            vec!["gain-step=1dB", "gain-step=2dB"],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn unnamed_parameters_keep_their_current_values() {
        let current = parse(&["gain-step=3dB", "zoom-step=0.25"])
            .unwrap()
            .unwrap()
            .apply(None)
            .unwrap();
        let louder = parse(&["gain-step=6dB"])
            .unwrap()
            .unwrap()
            .apply(current)
            .unwrap();
        assert_eq!(louder.unwrap().gain_step, GainDb::new(6000).unwrap());
        assert_eq!(louder.unwrap().zoom, current.unwrap().zoom);
        assert_eq!(
            EscalationInput::fields(louder.as_ref()),
            [
                ("Gain per play", "+6 dB".to_owned()),
                ("Zoom per play", "+0.250".to_owned())
            ]
        );
        assert_eq!(
            EscalationInput::fields(None),
            [("Escalation", "None".to_owned())]
        );
    }
}

#[cfg(test)]
mod gap_tests {
    use super::*;

    fn rate() -> FrameRate {
        FrameRate::new(30, 1).unwrap()
    }

    fn milliseconds(value: u32) -> PauseLength {
        PauseLength::Milliseconds {
            milliseconds: std::num::NonZeroU32::new(value).unwrap(),
        }
    }

    #[test]
    fn gaps_parse_with_steps_and_resolve_each_length_once() {
        let input = parse(&["3", "gap=120ms", "gain-step=3dB", "zoom-step=0.08"])
            .unwrap()
            .unwrap();
        assert_eq!(input.plays, Some(3));
        assert_eq!(
            input.gap.unwrap().lengths(3, rate()).unwrap(),
            [milliseconds(120)]
        );
        let ladder = parse(&["gap=500ms", "gap-step=-200ms"]).unwrap().unwrap();
        assert!(!ladder.changes_escalation());
        assert_eq!(
            ladder.gap.unwrap().lengths(3, rate()).unwrap(),
            [milliseconds(500), milliseconds(300)]
        );
        assert!(ladder.gap.unwrap().lengths(4, rate()).is_ok());
        assert!(
            ladder.gap.unwrap().lengths(5, rate()).is_err(),
            "a gap may not shrink to nothing"
        );
        let frames = parse(&["gap=6f", "gap-step=+2f"]).unwrap().unwrap();
        assert_eq!(
            frames.gap.unwrap().lengths(3, rate()).unwrap(),
            [
                PauseLength::Frames {
                    frames: std::num::NonZeroU32::new(6).unwrap()
                },
                PauseLength::Frames {
                    frames: std::num::NonZeroU32::new(8).unwrap()
                }
            ]
        );
        for zero in ["gap=0f", "gap=0", "gap=0ms"] {
            let removed = parse(&[zero]).unwrap().unwrap();
            assert_eq!(
                removed.gap.unwrap().lengths(3, rate()).unwrap(),
                [],
                "{zero}"
            );
        }
        // A ladder longer than the bound refuses instead of truncating.
        let long = parse(&["gap=500f", "gap-step=-1f"]).unwrap().unwrap();
        assert!(long.gap.unwrap().lengths(65, rate()).is_ok());
        assert!(
            long.gap
                .unwrap()
                .lengths(66, rate())
                .unwrap_err()
                .contains("at most 64")
        );
        let mixed = parse(&["gap=6f", "gap-step=-10ms"]).unwrap().unwrap();
        assert!(mixed.gap.unwrap().lengths(3, rate()).is_err());
    }
}
