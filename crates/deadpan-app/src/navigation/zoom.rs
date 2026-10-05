//! `,z`, `,c`, `:zoom` and `:creep`: authored framing on the selected beat,
//! optionally centered on a saved attention target and limited to the Edit
//! range inside that beat (specification §6.4, §7.5 and §8.2).
//!
//! Parsing and framing construction are pure. The app resolves the target
//! and the current pose at the displayed picture, then commits the result as
//! one ordinary `SetFraming` edit.

use deadpan_core::{
    ExactRatio, Framing, FramingClock, FramingCurve, FramingEnvelope, FramingPose, FramingSegment,
    FramingValue, TargetId,
};

pub const ZOOM_USAGE: &str = "Use :zoom 1.35 [target=current|center|face:N|ID or label] [curve=step|linear|smoothstep], or :zoom off to return to the full picture.";
pub const CREEP_USAGE: &str = "Use :creep [from=1] [to=1.35] [target=current|center|face:N|ID or label] [curve=smoothstep|linear].";

/// The most faces a `face:N` proposal can name (the detector's bound).
pub const MAX_FACE_NUMBER: u8 = 64;

/// The default punch-in and creep scale of `,z` and `,c`.
pub fn default_scale() -> ExactRatio {
    ExactRatio::new(27, 20).expect("constant ratio")
}

/// Which point the new framing centers on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetChoice {
    /// Keep the current center.
    Keep,
    /// `,z`: the selected target when there is one, else the current center.
    Selected,
    /// `target=current`: the selected target, which must exist.
    Current,
    /// `target=center`: the center of the Original's picture.
    Center,
    /// A saved target by id or label.
    Named(String),
    /// `target=face:N`: the `N`th face (1-based, left to right) that face
    /// detection proposes in the displayed picture. Detection runs in the
    /// background; only this explicit command saves the face as a target.
    Face(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomCurve {
    Step,
    Linear,
    Smoothstep,
}

impl ZoomCurve {
    fn framing(self) -> FramingCurve {
        match self {
            Self::Step => FramingCurve::Step,
            Self::Linear => FramingCurve::Linear,
            Self::Smoothstep => FramingCurve::Smoothstep,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomKind {
    /// A step change to `scale` (smash zoom / punch-in).
    Smash { scale: ExactRatio },
    /// An eased change from the current (or `from`) scale to `to`.
    Creep {
        from: Option<ExactRatio>,
        to: ExactRatio,
        curve: ZoomCurve,
    },
    /// Remove framing: an abrupt return to the full picture.
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoomInput {
    pub kind: ZoomKind,
    pub target: TargetChoice,
}

impl ZoomInput {
    /// `,z`: punch in to 1.35× on the selected target.
    pub fn punch_in() -> Self {
        Self {
            kind: ZoomKind::Smash {
                scale: default_scale(),
            },
            target: TargetChoice::Selected,
        }
    }

    /// `,c`: creep from the current framing to 1.35×.
    pub fn creep() -> Self {
        Self {
            kind: ZoomKind::Creep {
                from: None,
                to: default_scale(),
                curve: ZoomCurve::Smoothstep,
            },
            target: TargetChoice::Keep,
        }
    }
}

/// Split `name=value` words, keeping double-quoted values with spaces.
fn words(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                current.push(chars.next().ok_or("Unfinished escape in a quoted value.")?)
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if quoted {
        return Err("Close the quoted value with \".".into());
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

fn scale(value: &str) -> Result<ExactRatio, String> {
    let value = value
        .strip_suffix('x')
        .or_else(|| value.strip_suffix('×'))
        .unwrap_or(value);
    let ratio = super::gag::decimal(value)?;
    FramingPose::new(
        ExactRatio::new(1, 2).expect("constant"),
        ExactRatio::new(1, 2).expect("constant"),
        ratio,
    )
    .map_err(|_| format!("Scale {value} is outside 1/64..64."))?;
    Ok(ratio)
}

fn target(value: &str) -> Result<TargetChoice, String> {
    Ok(match value {
        "" => {
            return Err("target= needs current, center, face:N, or a target id or label.".into());
        }
        "current" => TargetChoice::Current,
        "center" => TargetChoice::Center,
        other => match other.strip_prefix("face:") {
            Some(number) => TargetChoice::Face(
                number
                    .parse::<u8>()
                    .ok()
                    .filter(|number| (1..=MAX_FACE_NUMBER).contains(number))
                    .ok_or(format!(
                        "face:N numbers the faces in the picture from 1 to {MAX_FACE_NUMBER}, left to right."
                    ))?,
            ),
            None => TargetChoice::Named(other.to_owned()),
        },
    })
}

fn curve(value: &str) -> Result<ZoomCurve, String> {
    match value {
        "step" => Ok(ZoomCurve::Step),
        "linear" => Ok(ZoomCurve::Linear),
        "smoothstep" | "smooth" => Ok(ZoomCurve::Smoothstep),
        _ => Err("curve is step, linear or smoothstep.".into()),
    }
}

fn parameters(
    words: &[String],
    allowed: &[&str],
    usage: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut values = std::collections::BTreeMap::new();
    for word in words {
        let (key, value) = word.split_once('=').ok_or(usage)?;
        if !allowed.contains(&key) {
            return Err(format!("Unknown parameter {key}. {usage}"));
        }
        if values.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("{key} is given twice."));
        }
    }
    Ok(values)
}

/// `:zoom SCALE [target=…] [curve=…]` or `:zoom off`.
pub fn parse_zoom(text: &str) -> Result<ZoomInput, String> {
    let words = words(text)?;
    let (first, rest) = words.split_first().ok_or(ZOOM_USAGE)?;
    if matches!(first.as_str(), "off" | "none") {
        if !rest.is_empty() {
            return Err("Use :zoom off without parameters.".into());
        }
        return Ok(ZoomInput {
            kind: ZoomKind::Off,
            target: TargetChoice::Keep,
        });
    }
    if first.contains('=') {
        return Err(format!("Give the scale first. {ZOOM_USAGE}"));
    }
    let to = scale(first)?;
    let mut values = parameters(rest, &["target", "curve"], ZOOM_USAGE)?;
    let target = values
        .remove("target")
        .map_or(Ok(TargetChoice::Keep), |value| target(&value))?;
    let kind = match values
        .remove("curve")
        .map_or(Ok(ZoomCurve::Step), |value| curve(&value))?
    {
        ZoomCurve::Step => ZoomKind::Smash { scale: to },
        curve => ZoomKind::Creep {
            from: None,
            to,
            curve,
        },
    };
    Ok(ZoomInput { kind, target })
}

/// `:creep [from=S] [to=S] [target=…] [curve=smoothstep|linear]`.
pub fn parse_creep(text: &str) -> Result<ZoomInput, String> {
    let words = words(text)?;
    let mut values = parameters(&words, &["from", "to", "target", "curve"], CREEP_USAGE)?;
    let from = values
        .remove("from")
        .map(|value| scale(&value))
        .transpose()?;
    let to = values
        .remove("to")
        .map_or(Ok(default_scale()), |value| scale(&value))?;
    let curve = values
        .remove("curve")
        .map_or(Ok(ZoomCurve::Smoothstep), |value| curve(&value))?;
    if curve == ZoomCurve::Step {
        return Err("A creep eases; use :zoom for a step change.".into());
    }
    let target = values
        .remove("target")
        .map_or(Ok(TargetChoice::Keep), |value| target(&value))?;
    Ok(ZoomInput {
        kind: ZoomKind::Creep { from, to, curve },
        target,
    })
}

/// What a target choice names once resolved against the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    Keep,
    Center,
    Target(TargetId),
}

/// Resolve `choice` given the target the beat follows and the saved targets
/// covering the displayed picture (id and label, in picker order). The second
/// value explains a fallback for the status line.
pub fn resolve(
    choice: &TargetChoice,
    followed: Option<&TargetId>,
    shown: &[(TargetId, String)],
    named: impl Fn(&str) -> Result<TargetId, String>,
) -> Result<(Resolved, Option<String>), String> {
    Ok(match choice {
        TargetChoice::Keep => (Resolved::Keep, None),
        TargetChoice::Center => (Resolved::Center, None),
        TargetChoice::Named(text) => (Resolved::Target(named(text)?), None),
        TargetChoice::Face(number) => {
            return Err(format!(
                "face:{number} names a detected face, which needs face detection first."
            ));
        }
        TargetChoice::Current | TargetChoice::Selected => {
            if let Some(id) = followed {
                return Ok((Resolved::Target(id.clone()), None));
            }
            match shown {
                [(id, _)] => (Resolved::Target(id.clone()), None),
                [] if *choice == TargetChoice::Selected => (
                    Resolved::Keep,
                    Some("No saved target covers this picture, so the punch-in keeps the current center. Draw one in Camera (,f then n).".to_owned()),
                ),
                [] => {
                    return Err("No saved target covers this picture. Name one with target=, or draw one in Camera (,f then n).".into());
                }
                several => {
                    let labels: Vec<&str> = several.iter().map(|(_, label)| label.as_str()).collect();
                    return Err(format!(
                        "Several targets cover this picture ({}). Name one, for example :zoom 1.35 target={}.",
                        labels.join(", "),
                        several[0].0.as_str()
                    ));
                }
            }
        }
    })
}

/// The beat-local frame whose picture fixes a target's center for `kind`:
/// the first frame of a step (where it lands) or the last frame of a creep
/// (where it arrives). A whole-beat step follows live and needs none.
pub fn sample_frame(kind: ZoomKind, range: Option<(u64, u64)>, frames: u64) -> Option<u64> {
    match kind {
        ZoomKind::Off => None,
        ZoomKind::Smash { .. } => range.map(|(start, _)| start),
        ZoomKind::Creep { .. } => Some(range.map_or(frames, |(_, end)| end).saturating_sub(1)),
    }
}

/// A resolved center for the new framing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Center {
    /// Keep the current evaluated center.
    Keep,
    /// A fixed point in the operation's input canvas.
    Point([ExactRatio; 2]),
    /// A saved target. `fixed` is its center at the [`sample_frame`] picture,
    /// for eased and ranged framing; a whole-beat step follows it live.
    Target {
        id: TargetId,
        fixed: Option<[ExactRatio; 2]>,
    },
}

/// The beat being framed, as Camera sees it at the displayed picture.
pub struct Shot<'a> {
    /// The beat's authored framing before the edit.
    pub entry: Option<&'a Framing>,
    /// The beat's evaluated pose at the displayed picture.
    pub current: FramingPose,
    /// The beat's output duration in frames.
    pub frames: u64,
    /// The Edit range inside the beat, in its local frames.
    pub range: Option<(u64, u64)>,
    /// The command named its target explicitly (`target=`), which may replace
    /// an existing camera path on the whole beat.
    pub explicit: bool,
}

fn pose(center: [ExactRatio; 2], scale: ExactRatio) -> Result<FramingPose, String> {
    FramingPose::new(center[0], center[1], scale).map_err(|error| error.to_string())
}

fn progress(frame: u64, frames: u64) -> Result<ExactRatio, String> {
    ExactRatio::new(i128::from(frame), i128::from(frames)).map_err(|error| error.to_string())
}

const PATH_REFUSAL: &str = "This beat has a camera path, and this would replace it. Use :zoom off first, or name the target (target=current, center or an id) to replace it explicitly.";
const FOLLOW_CREEP_REFUSAL: &str = "This beat follows a target; a creep with a fixed center would stop following it. Use :zoom S to change the follow's scale, name target= to creep to a fixed point, or :zoom off first.";
const RANGED_FOLLOW_REFUSAL: &str = "This beat follows a target; a range would stop following it outside the range. Apply to the whole beat (clear the Visual range) or :zoom off first.";
const RANGED_PATH_REFUSAL: &str = "This beat has a camera path; a range would replace it outside the range. Apply to the whole beat (clear the Visual range) or :zoom off first.";

/// Build the framing `kind` describes, or `None` for the full picture.
///
/// Nothing here flattens an existing path implicitly. On the whole beat, a
/// follow keeps following when only its scale changes, and a camera path is
/// replaced only by an explicit `target=` or `:zoom off`. A ranged edit keeps
/// the beat's framing outside the range, so it needs no framing or one static
/// pose (on any clock, since a static pose is constant).
pub fn build(kind: ZoomKind, center: &Center, shot: &Shot<'_>) -> Result<Option<Framing>, String> {
    let range = shot
        .range
        .filter(|(start, end)| !(*start == 0 && *end == shot.frames));
    if let Some((start, end)) = range
        && (start >= end || end > shot.frames)
    {
        return Err("Select a nonempty range inside the selected beat.".into());
    }
    let fixed = |current: [ExactRatio; 2]| -> Result<[ExactRatio; 2], String> {
        Ok(match center {
            Center::Keep => current,
            Center::Point(point) => *point,
            Center::Target { fixed, .. } => fixed.ok_or(
                "That target is not visible at the frame this framing samples, so it has no center there.",
            )?,
        })
    };
    let current = [shot.current.center_x, shot.current.center_y];
    let entry = shot.entry.map(|framing| &framing.value);
    let keep = *center == Center::Keep;
    let Some((start, end)) = range else {
        if !matches!(kind, ZoomKind::Off)
            && matches!(entry, Some(FramingValue::Envelope { .. }))
            && (keep || !shot.explicit)
        {
            return Err(PATH_REFUSAL.into());
        }
        return match kind {
            ZoomKind::Off => Ok(None),
            ZoomKind::Smash { scale } => {
                let (target, fallback, clock) = match (center, shot.entry) {
                    (Center::Target { id, .. }, _) => {
                        (id.clone(), pose(current, scale)?, FramingClock::OwnerOutput)
                    }
                    // Keep following; only the scale changes.
                    (
                        Center::Keep,
                        Some(Framing {
                            clock,
                            value:
                                FramingValue::Follow {
                                    target, fallback, ..
                                },
                        }),
                    ) => (
                        target.clone(),
                        pose([fallback.center_x, fallback.center_y], scale)?,
                        *clock,
                    ),
                    _ => {
                        return Framing::static_pose(pose(fixed(current)?, scale)?)
                            .map(Some)
                            .map_err(|error| error.to_string());
                    }
                };
                let framing = Framing {
                    clock,
                    value: FramingValue::Follow {
                        target,
                        scale,
                        fallback,
                    },
                };
                framing.validate().map_err(|error| error.to_string())?;
                Ok(Some(framing))
            }
            ZoomKind::Creep { from, to, curve } => {
                if keep && matches!(entry, Some(FramingValue::Follow { .. })) {
                    return Err(FOLLOW_CREEP_REFUSAL.into());
                }
                Framing::creep(
                    pose(current, from.unwrap_or(shot.current.scale))?,
                    pose(fixed(current)?, to)?,
                    curve.framing(),
                )
                .map(Some)
                .map_err(|error| error.to_string())
            }
        };
    };
    // Outside the range the beat keeps what it shows now.
    let before = match entry {
        None => FramingPose::identity(),
        Some(FramingValue::Static { pose }) => *pose,
        Some(FramingValue::Follow { .. }) => return Err(RANGED_FOLLOW_REFUSAL.into()),
        Some(FramingValue::Envelope { .. }) => return Err(RANGED_PATH_REFUSAL.into()),
    };
    let before_center = [before.center_x, before.center_y];
    let (inside_from, inside_to, curve, after) = match kind {
        ZoomKind::Off => {
            let identity = FramingPose::identity();
            (identity, identity, FramingCurve::Step, before)
        }
        ZoomKind::Smash { scale } => {
            let inside = pose(fixed(before_center)?, scale)?;
            (inside, inside, FramingCurve::Step, before)
        }
        ZoomKind::Creep { from, to, curve } => {
            let target = pose(fixed(before_center)?, to)?;
            (
                pose(before_center, from.unwrap_or(before.scale))?,
                target,
                curve.framing(),
                target,
            )
        }
    };
    if inside_from == before && inside_to == before {
        // Nothing changes inside the range.
        return Ok(shot.entry.cloned());
    }
    // A Step segment holds the previous pose until its end; its own pose
    // applies from that end onward. Pictures sample frame centers, so each
    // frame of [start, end) sees the range's pose.
    let mut segments = Vec::new();
    let initial = if start == 0 {
        inside_from
    } else {
        segments.push(FramingSegment {
            end: progress(start, shot.frames)?,
            pose: inside_from,
            curve: FramingCurve::Step,
        });
        before
    };
    let stepped = curve == FramingCurve::Step;
    segments.push(FramingSegment {
        end: progress(end, shot.frames)?,
        pose: if stepped && end != shot.frames {
            after
        } else {
            inside_to
        },
        curve,
    });
    if end != shot.frames {
        segments.push(FramingSegment {
            end: ExactRatio::ONE,
            pose: after,
            curve: FramingCurve::Step,
        });
    }
    let framing = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Envelope {
            envelope: FramingEnvelope { initial, segments },
        },
    };
    framing
        .validate()
        .map_err(|_| "This range cannot be expressed on the beat's framing clock (at most one million frames).".to_owned())?;
    Ok(Some(framing))
}

fn scale_label(scale: ExactRatio) -> String {
    format!(
        "{:.3}×",
        scale.numerator() as f64 / scale.denominator() as f64
    )
}

/// A short account of committed framing for the status line.
pub fn describe(framing: Option<&Framing>, label: &dyn Fn(&TargetId) -> String) -> String {
    match framing.map(|framing| &framing.value) {
        None => "full picture, no framing".into(),
        Some(FramingValue::Static { pose }) => format!("{} static", scale_label(pose.scale)),
        Some(FramingValue::Follow { target, scale, .. }) => {
            format!("{} following {}", scale_label(*scale), label(target))
        }
        Some(FramingValue::Envelope { envelope }) => {
            let last = envelope
                .segments
                .last()
                .map_or(envelope.initial, |segment| segment.pose);
            let scales: Vec<ExactRatio> = std::iter::once(envelope.initial.scale)
                .chain(envelope.segments.iter().map(|segment| segment.pose.scale))
                .collect();
            let peak = scales
                .iter()
                .copied()
                .max_by(|a, b| a.compare(*b))
                .unwrap_or(last.scale);
            format!(
                "camera path {} → {} (peak {}, {} segment{})",
                scale_label(envelope.initial.scale),
                scale_label(last.scale),
                scale_label(peak),
                envelope.segments.len(),
                if envelope.segments.len() == 1 {
                    ""
                } else {
                    "s"
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }

    fn shot(range: Option<(u64, u64)>) -> Shot<'static> {
        Shot {
            entry: None,
            current: FramingPose::identity(),
            frames: 10,
            range,
            explicit: false,
        }
    }

    fn approx(value: ExactRatio) -> f64 {
        value.numerator() as f64 / value.denominator() as f64
    }

    fn close(value: ExactRatio, expected: f64) -> bool {
        (approx(value) - expected).abs() < 1e-8
    }

    fn evaluate(framing: &Option<Framing>, frame: u64) -> FramingPose {
        framing.as_ref().map_or(FramingPose::identity(), |framing| {
            framing
                .evaluate(
                    ratio(2 * i128::from(frame) + 1, 2),
                    deadpan_core::FrameDuration::new(10).unwrap(),
                )
                .unwrap()
        })
    }

    #[test]
    fn commands_parse_with_defaults_targets_and_curves() {
        assert_eq!(
            parse_zoom("1.35 target=face:2 curve=step").unwrap(),
            ZoomInput {
                kind: ZoomKind::Smash {
                    scale: ratio(27, 20)
                },
                target: TargetChoice::Face(2),
            }
        );
        assert_eq!(
            parse_creep("to=1.5 target=face:1").unwrap().target,
            TargetChoice::Face(1)
        );
        assert_eq!(
            parse_zoom("2 target=\"face 2\"").unwrap().target,
            TargetChoice::Named("face 2".into())
        );
        assert_eq!(
            parse_zoom("2 curve=linear target=\"Target 1\"").unwrap(),
            ZoomInput {
                kind: ZoomKind::Creep {
                    from: None,
                    to: ratio(2, 1),
                    curve: ZoomCurve::Linear
                },
                target: TargetChoice::Named("Target 1".into()),
            }
        );
        assert_eq!(parse_zoom("off").unwrap().kind, ZoomKind::Off);
        assert_eq!(
            parse_creep("from=1 to=1.4 target=current").unwrap(),
            ZoomInput {
                kind: ZoomKind::Creep {
                    from: Some(ExactRatio::ONE),
                    to: ratio(7, 5),
                    curve: ZoomCurve::Smoothstep
                },
                target: TargetChoice::Current,
            }
        );
        assert_eq!(parse_creep("").unwrap(), ZoomInput::creep());
        for bad in [
            "",
            "target=center",
            "1.35 curve=wobble",
            "1.35 1.4",
            "0",
            "65",
            "off 2",
            "1.35 target=",
            "1.35 scale=2",
            "1.35 target=a target=b",
            "1.35 target=\"open",
            "1.35 target=face:0",
            "1.35 target=face:65",
            "1.35 target=face:",
            "1.35 target=face:two",
            "1.35 target=face:-1",
        ] {
            assert!(parse_zoom(bad).is_err(), "{bad}");
        }
        for bad in ["curve=step", "to=0", "speed=2", "to=1.2 to=1.3"] {
            assert!(parse_creep(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn whole_beat_smash_follows_a_target_and_otherwise_keeps_a_static_pose() {
        let id = TargetId::new("target-1").unwrap();
        let current = FramingPose::new(ratio(2, 5), ratio(1, 2), ExactRatio::ONE).unwrap();
        let shot = Shot {
            current,
            ..shot(None)
        };
        let followed = build(
            ZoomKind::Smash {
                scale: ratio(27, 20),
            },
            &Center::Target {
                id: id.clone(),
                fixed: None,
            },
            &shot,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            followed.value,
            FramingValue::Follow {
                target: id,
                scale: ratio(27, 20),
                fallback: FramingPose {
                    scale: ratio(27, 20),
                    ..current
                },
            }
        );
        let fixed = build(
            ZoomKind::Smash { scale: ratio(2, 1) },
            &Center::Point([ratio(1, 4), ratio(3, 4)]),
            &shot,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            fixed.value,
            FramingValue::Static {
                pose: FramingPose::new(ratio(1, 4), ratio(3, 4), ratio(2, 1)).unwrap()
            }
        );
        assert_eq!(build(ZoomKind::Off, &Center::Keep, &shot).unwrap(), None);
    }

    #[test]
    fn creeps_ease_from_the_current_pose_toward_a_target_center() {
        let framing = build(
            ZoomKind::Creep {
                from: Some(ExactRatio::ONE),
                to: ratio(7, 5),
                curve: ZoomCurve::Linear,
            },
            &Center::Target {
                id: TargetId::new("target-1").unwrap(),
                fixed: Some([ratio(3, 4), ratio(1, 2)]),
            },
            &shot(None),
        )
        .unwrap();
        assert!(close(evaluate(&framing, 0).scale, 1.02));
        assert!(close(evaluate(&framing, 9).scale, 1.38));
        assert!(close(evaluate(&framing, 9).center_x, 0.7375));
        // A target that is not visible here has no fixed center to creep to.
        assert!(
            build(
                ZoomKind::Creep {
                    from: None,
                    to: ratio(7, 5),
                    curve: ZoomCurve::Linear
                },
                &Center::Target {
                    id: TargetId::new("target-1").unwrap(),
                    fixed: None
                },
                &shot(None),
            )
            .is_err()
        );
    }

    #[test]
    fn ranged_framing_keeps_the_beat_outside_the_range() {
        let scale = ratio(27, 20);
        let smash = build(
            ZoomKind::Smash { scale },
            &Center::Keep,
            &shot(Some((3, 6))),
        )
        .unwrap();
        let scales: Vec<_> = (0..10).map(|frame| evaluate(&smash, frame).scale).collect();
        assert_eq!(
            scales,
            [1, 1, 1, 0, 0, 0, 1, 1, 1, 1].map(|unit| if unit == 1 {
                ExactRatio::ONE
            } else {
                scale
            })
        );
        // A creep over 4..8 holds before, eases, then holds its target.
        let creep = build(
            ZoomKind::Creep {
                from: None,
                to: ratio(2, 1),
                curve: ZoomCurve::Linear,
            },
            &Center::Keep,
            &shot(Some((4, 8))),
        )
        .unwrap();
        assert_eq!(evaluate(&creep, 3).scale, ExactRatio::ONE);
        assert!(close(evaluate(&creep, 4).scale, 1.125));
        assert!(close(evaluate(&creep, 7).scale, 1.875));
        assert_eq!(evaluate(&creep, 9).scale, ratio(2, 1));
        // A range that reaches the beat end needs no trailing segment.
        let tail = build(
            ZoomKind::Smash { scale },
            &Center::Keep,
            &shot(Some((5, 10))),
        )
        .unwrap();
        assert_eq!(evaluate(&tail, 4).scale, ExactRatio::ONE);
        assert_eq!(evaluate(&tail, 9).scale, scale);
        // A whole-beat range is the whole beat.
        assert_eq!(
            build(
                ZoomKind::Smash { scale },
                &Center::Keep,
                &shot(Some((0, 10)))
            )
            .unwrap(),
            build(ZoomKind::Smash { scale }, &Center::Keep, &shot(None)).unwrap(),
        );
    }

    #[test]
    fn abrupt_return_steps_out_over_a_range_and_never_flattens_a_path() {
        let zoomed =
            Framing::static_pose(FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap())
                .unwrap();
        let shot = Shot {
            entry: Some(&zoomed),
            current: FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap(),
            frames: 10,
            range: Some((6, 10)),
            explicit: false,
        };
        let returned = build(ZoomKind::Off, &Center::Keep, &shot).unwrap();
        assert_eq!(evaluate(&returned, 5).scale, ratio(3, 2));
        assert_eq!(evaluate(&returned, 6), FramingPose::identity());
        assert_eq!(evaluate(&returned, 9), FramingPose::identity());
        let path = Framing::creep(
            FramingPose::identity(),
            FramingPose {
                scale: ratio(2, 1),
                ..FramingPose::identity()
            },
            FramingCurve::Smoothstep,
        )
        .unwrap();
        let shot = Shot {
            entry: Some(&path),
            ..shot
        };
        assert!(build(ZoomKind::Off, &Center::Keep, &shot).is_err());
        assert!(build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).is_err());
        // An unframed beat stepping out over a range is unchanged.
        let shot = Shot {
            entry: None,
            current: FramingPose::identity(),
            ..shot
        };
        assert_eq!(build(ZoomKind::Off, &Center::Keep, &shot).unwrap(), None);
    }

    #[test]
    fn whole_beat_keep_retains_a_follow_and_refuses_to_flatten_a_path() {
        let id = TargetId::new("target-1").unwrap();
        let fallback = FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(11, 10)).unwrap();
        let follow = Framing {
            clock: FramingClock::RetainedOutput {
                offset: ExactRatio::ONE,
                duration: ExactRatio::integer(12),
            },
            value: FramingValue::Follow {
                target: id.clone(),
                scale: ratio(11, 10),
                fallback,
            },
        };
        // The displayed pose is the target's center, not the fallback.
        let current = FramingPose::new(ratio(3, 5), ratio(1, 2), ratio(11, 10)).unwrap();
        let shot_on = |entry| Shot {
            entry: Some(entry),
            current,
            ..shot(None)
        };
        let smash = build(
            ZoomKind::Smash { scale: ratio(2, 1) },
            &Center::Keep,
            &shot_on(&follow),
        )
        .unwrap()
        .unwrap();
        assert_eq!(smash.clock, follow.clock);
        assert_eq!(
            smash.value,
            FramingValue::Follow {
                target: id,
                scale: ratio(2, 1),
                fallback: FramingPose {
                    scale: ratio(2, 1),
                    ..fallback
                },
            }
        );
        let creep = ZoomKind::Creep {
            from: None,
            to: ratio(2, 1),
            curve: ZoomCurve::Smoothstep,
        };
        assert_eq!(
            build(creep, &Center::Keep, &shot_on(&follow)).unwrap_err(),
            FOLLOW_CREEP_REFUSAL
        );
        // An explicit fixed center replaces the follow on request.
        assert!(matches!(
            build(
                creep,
                &Center::Point([ratio(1, 2), ratio(1, 2)]),
                &Shot {
                    explicit: true,
                    ..shot_on(&follow)
                }
            ),
            Ok(Some(Framing {
                value: FramingValue::Envelope { .. },
                ..
            }))
        ));
        let path = Framing::creep(
            FramingPose::identity(),
            FramingPose {
                scale: ratio(2, 1),
                ..FramingPose::identity()
            },
            FramingCurve::Smoothstep,
        )
        .unwrap();
        for kind in [ZoomKind::Smash { scale: ratio(3, 2) }, creep] {
            assert_eq!(
                build(kind, &Center::Keep, &shot_on(&path)).unwrap_err(),
                PATH_REFUSAL
            );
            // ,z resolving the selected target is not an explicit replacement.
            let target = Center::Target {
                id: TargetId::new("target-1").unwrap(),
                fixed: Some([ratio(1, 2), ratio(1, 2)]),
            };
            assert_eq!(
                build(kind, &target, &shot_on(&path)).unwrap_err(),
                PATH_REFUSAL
            );
            assert!(
                build(
                    kind,
                    &target,
                    &Shot {
                        explicit: true,
                        ..shot_on(&path)
                    }
                )
                .is_ok()
            );
        }
        assert_eq!(
            build(ZoomKind::Off, &Center::Keep, &shot_on(&path)).unwrap(),
            None
        );
    }

    #[test]
    fn retained_clock_static_poses_are_ranged_like_any_static_pose() {
        let zoomed = FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap();
        let retained = Framing {
            clock: FramingClock::RetainedOutput {
                offset: ratio(-3, 1),
                duration: ExactRatio::integer(7),
            },
            value: FramingValue::Static { pose: zoomed },
        };
        let shot = Shot {
            entry: Some(&retained),
            current: zoomed,
            frames: 10,
            range: Some((2, 5)),
            explicit: false,
        };
        let returned = build(ZoomKind::Off, &Center::Keep, &shot).unwrap();
        let scales: Vec<_> = (0..10)
            .map(|frame| evaluate(&returned, frame).scale)
            .collect();
        assert_eq!(scales[1], ratio(3, 2));
        assert_eq!(scales[2], ExactRatio::ONE);
        assert_eq!(scales[4], ExactRatio::ONE);
        assert_eq!(scales[5], ratio(3, 2));
        // Ranged edits on follows and paths name what they would replace.
        let follow = Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Follow {
                target: TargetId::new("target-1").unwrap(),
                scale: ratio(3, 2),
                fallback: zoomed,
            },
        };
        assert_eq!(
            build(
                ZoomKind::Off,
                &Center::Keep,
                &Shot {
                    entry: Some(&follow),
                    ..shot
                }
            )
            .unwrap_err(),
            RANGED_FOLLOW_REFUSAL
        );
    }

    #[test]
    fn ranges_beyond_the_progress_grid_refuse_with_the_frame_limit() {
        let shot = Shot {
            entry: None,
            current: FramingPose::identity(),
            frames: 1_000_003,
            range: Some((1, 2)),
            explicit: false,
        };
        let error =
            build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).unwrap_err();
        assert!(error.contains("one million frames"), "{error}");
        // Reducible progress within the grid is admitted.
        let shot = Shot {
            frames: 2_000_000,
            range: Some((500_000, 1_000_000)),
            ..shot
        };
        assert!(build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).is_ok());
    }

    #[test]
    fn target_resolution_prefers_the_follow_then_a_single_visible_target() {
        let one = TargetId::new("target-1").unwrap();
        let two = TargetId::new("target-2").unwrap();
        let shown = [
            (one.clone(), "Target 1".to_owned()),
            (two.clone(), "Target 2".to_owned()),
        ];
        let named = |text: &str| TargetId::new(text).map_err(|error| error.to_string());
        let error = resolve(&TargetChoice::Selected, None, &shown, named).unwrap_err();
        assert!(
            error.contains("Several targets cover this picture (Target 1, Target 2)"),
            "{error}"
        );
        assert!(resolve(&TargetChoice::Current, None, &shown, named).is_err());
        assert_eq!(
            resolve(&TargetChoice::Selected, Some(&two), &shown, named).unwrap(),
            (Resolved::Target(two.clone()), None)
        );
        assert_eq!(
            resolve(&TargetChoice::Current, None, &shown[..1], named).unwrap(),
            (Resolved::Target(one), None)
        );
        let (resolved, note) = resolve(&TargetChoice::Selected, None, &[], named).unwrap();
        assert_eq!(resolved, Resolved::Keep);
        assert!(note.is_some());
        assert!(resolve(&TargetChoice::Current, None, &[], named).is_err());
        assert_eq!(
            resolve(&TargetChoice::Named("target-2".into()), None, &[], named).unwrap(),
            (Resolved::Target(two), None)
        );
        assert_eq!(
            resolve(&TargetChoice::Center, None, &[], named).unwrap().0,
            Resolved::Center
        );
    }

    #[test]
    fn targets_are_sampled_where_a_step_lands_or_a_creep_arrives() {
        let smash = ZoomKind::Smash { scale: ratio(2, 1) };
        let creep = ZoomKind::Creep {
            from: None,
            to: ratio(2, 1),
            curve: ZoomCurve::Linear,
        };
        assert_eq!(sample_frame(smash, None, 10), None);
        assert_eq!(sample_frame(smash, Some((3, 6)), 10), Some(3));
        assert_eq!(sample_frame(creep, Some((3, 6)), 10), Some(5));
        assert_eq!(sample_frame(creep, None, 10), Some(9));
        assert_eq!(sample_frame(ZoomKind::Off, Some((3, 6)), 10), None);
    }
}
