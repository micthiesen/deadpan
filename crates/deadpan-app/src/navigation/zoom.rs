//! `,z`, `,c`, `:zoom` and `:creep`: authored framing on the selected beat,
//! optionally centered on a saved attention target and limited to the Edit
//! range inside that beat (specification §6.4, §7.5 and §8.2).
//!
//! Parsing and framing construction are pure. The app resolves the target
//! and the current pose at the displayed picture, then commits the result as
//! one ordinary `SetFraming` edit.

pub use deadpan_core::framing_zoom::{
    Center, Shot, ZoomCurve, ZoomKind, build, default_scale, sample_frame,
};
use deadpan_core::{ExactRatio, Framing, FramingPose, FramingValue, TargetId};

pub const ZOOM_USAGE: &str = "Use :zoom 1.35 [target=current|center|face:N|ID or label] [curve=step|linear|smoothstep], or :zoom off to return to the full picture.";
pub const CREEP_USAGE: &str = "Use :creep [from=1] [to=1.35] [target=current|center|face:N|ID or label] [curve=smoothstep|linear].";

/// The most faces a `face:N` proposal can name (the detector's bound).
pub const MAX_FACE_NUMBER: u8 = 64;

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
}
