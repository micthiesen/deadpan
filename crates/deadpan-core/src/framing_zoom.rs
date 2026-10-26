//! Pure punch-in, creep and ranged framing construction shared by native and
//! semantic editing. Target resolution and picture geometry belong to the host.

use serde::{Deserialize, Serialize};

use crate::{
    ExactRatio, Framing, FramingClock, FramingCurve, FramingEnvelope, FramingPose, FramingSegment,
    FramingValue, TargetId,
};

/// The default punch-in and creep scale of `,z` and `,c`.
pub fn default_scale() -> ExactRatio {
    ExactRatio::new(27, 20).expect("constant ratio")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
    #[serde(deserialize_with = "deserialize_empty")]
    Off,
}

impl ZoomKind {
    /// Validate bounded authored scales and the eased curve of a creep before
    /// accepting semantic wire input. This does not resolve a host picture.
    pub fn validate(self) -> Result<(), String> {
        let scale = |scale| {
            let center = FramingPose::identity();
            FramingPose::new(center.center_x, center.center_y, scale)
                .map(|_| ())
                .map_err(|error| error.to_string())
        };
        match self {
            Self::Off => Ok(()),
            Self::Smash { scale: value } => scale(value),
            Self::Creep { from, to, curve } => {
                if let Some(from) = from {
                    scale(from)?;
                }
                scale(to)?;
                if curve == ZoomCurve::Step {
                    return Err("A creep eases; use :zoom for a step change.".into());
                }
                Ok(())
            }
        }
    }
}

fn deserialize_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}

    Empty::deserialize(deserializer).map(|_| ())
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

/// Resolved owner eligible to create a Keep Follow transition. Range admission
/// remains the host's responsibility and never grants occurrence authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeFollowOwner {
    Source,
    Hold,
    Sequence,
    Repeat,
    Retime,
}

/// The beat being framed, as Camera sees it at the displayed picture.
pub struct Shot<'a> {
    /// Resolved owner after the host validates ordinary or explicit-play
    /// context. This admits transition construction, never raw payload validity.
    pub whole_owner: Option<WholeFollowOwner>,
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

pub const PATH_REFUSAL: &str = "This beat has a camera path, and this would replace it. Use :zoom off first, or name the target (target=current, center or an id) to replace it explicitly.";
pub const FOLLOW_CREEP_REFUSAL: &str = "Following creep requires one admitted owner. Choose a contained Edit range and This play at every enclosing Repeat, or clear the range.";
pub const RANGED_FOLLOW_REFUSAL: &str = "This beat follows a target; a range would stop following it outside the range. Apply to the whole beat (clear the Visual range) or :zoom off first.";
pub const RANGED_PATH_REFUSAL: &str = "This beat has a camera path; a range would replace it outside the range. Apply to the whole beat (clear the Visual range) or :zoom off first.";
pub const FOLLOW_RETUNE_RANGE_REFUSAL: &str = "Clear the Edit range to retune this Follow's existing scale animation; its authored clock and range are retained.";

/// Build the framing `kind` describes, or `None` for the full picture.
///
/// Nothing here flattens an existing path implicitly. On the whole beat, a
/// follow keeps following when only its scale changes, and a camera path is
/// replaced only by an explicit `target=` or `:zoom off`. A ranged edit keeps
/// the beat's framing outside the range. Keep creep retunes an animated Follow
/// on its existing clock and window when no range is selected. An omitted
/// `from` retains both bases and the ranged start; an explicit `from` changes
/// the animation's start, preserving a ranged animation's prefix. Keep creep
/// can also retain a fixed Follow, including its prefix scale and fallback.
/// Other ranged forms require no framing or one static pose (on any clock,
/// since a static pose is constant).
pub fn build(kind: ZoomKind, center: &Center, shot: &Shot<'_>) -> Result<Option<Framing>, String> {
    kind.validate()?;
    if let Some((start, end)) = shot.range
        && (start >= end || end > shot.frames)
    {
        return Err("Select a nonempty range inside the selected beat.".into());
    }
    let selected = shot
        .range
        .map(|(start, end)| {
            Ok::<_, String>((progress(start, shot.frames)?, progress(end, shot.frames)?))
        })
        .transpose()?;
    build_at(kind, center, shot, selected)
}

/// Build over an exact normalized owner-output range supplied by a validated
/// host occurrence map. No local frame rounding is permitted. `shot.range`
/// must be absent so an integral recipe range cannot conflict with this clock.
pub fn build_normalized_range(
    kind: ZoomKind,
    center: &Center,
    shot: &Shot<'_>,
    selected: (ExactRatio, ExactRatio),
) -> Result<Option<Framing>, String> {
    kind.validate()?;
    if shot.range.is_some() {
        return Err("A framing range must use one declared clock.".into());
    }
    let (start, end) = selected;
    if start.compare(ExactRatio::ZERO).is_lt()
        || !start.compare(end).is_lt()
        || end.compare(ExactRatio::ONE).is_gt()
    {
        return Err("Select a nonempty range inside the framed owner's output.".into());
    }
    build_at(kind, center, shot, Some(selected))
}

/// Retune an existing Follow scale animation without changing its target,
/// normalization clock or window. `to` is the live branch's absolute arrival
/// scale. An omitted `from` preserves both bases and the ranged start; an
/// explicit `from` changes the start of the ramp while keeping a ranged
/// animation's prefix. The fallback retains its center and relative scale.
///
/// This data transformation grants no owner or occurrence authority. Callers
/// must capture and validate their edit scope before applying it. An explicit
/// Step animation is valid here; [`build`] separately requires an eased creep.
pub fn retune_follow(
    entry: &Framing,
    from: Option<ExactRatio>,
    to: ExactRatio,
    curve: ZoomCurve,
) -> Result<Framing, String> {
    let FramingValue::Follow {
        target,
        scale,
        fallback,
        scale_transition: Some(transition),
    } = &entry.value
    else {
        return Err("The selected framing has no Follow scale animation to retune.".into());
    };
    let (scale, fallback, range) = if let Some(mut range) = transition.range {
        if let Some(from) = from {
            range.from_factor = from
                .checked_div(*scale)
                .map_err(|error| error.to_string())?;
        }
        (*scale, *fallback, Some(range))
    } else {
        let from = from.unwrap_or(*scale);
        let fallback = FramingPose {
            scale: fallback
                .scale
                .checked_mul(
                    from.checked_div(*scale)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            ..*fallback
        };
        (from, fallback, None)
    };
    let framing = Framing {
        clock: entry.clock,
        value: FramingValue::Follow {
            target: target.clone(),
            scale,
            fallback,
            scale_transition: Some(crate::FollowScaleTransition {
                factor: to.checked_div(scale).map_err(|error| error.to_string())?,
                curve,
                range,
            }),
        },
    };
    framing.validate().map_err(|error| error.to_string())?;
    Ok(framing)
}

fn build_at(
    kind: ZoomKind,
    center: &Center,
    shot: &Shot<'_>,
    selected: Option<(ExactRatio, ExactRatio)>,
) -> Result<Option<Framing>, String> {
    let range =
        selected.filter(|(start, end)| !(*start == ExactRatio::ZERO && *end == ExactRatio::ONE));
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
    if keep
        && let ZoomKind::Creep { from, to, curve } = kind
        && matches!(
            entry,
            Some(FramingValue::Follow {
                scale_transition: Some(_),
                ..
            })
        )
    {
        if selected.is_some() {
            return Err(FOLLOW_RETUNE_RANGE_REFUSAL.into());
        }
        if shot.whole_owner.is_none() {
            return Err(FOLLOW_CREEP_REFUSAL.into());
        }
        return retune_follow(shot.entry.expect("Follow entry checked"), from, to, curve).map(Some);
    }
    if keep
        && let ZoomKind::Creep { from, to, curve } = kind
        && let Some((start, end)) = selected
        && let Some(FramingValue::Follow {
            target,
            scale,
            fallback,
            scale_transition: None,
        }) = entry
    {
        if shot.whole_owner.is_none() {
            return Err(FOLLOW_CREEP_REFUSAL.into());
        }
        let framing = Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Follow {
                target: target.clone(),
                scale: *scale,
                fallback: *fallback,
                scale_transition: Some(crate::FollowScaleTransition {
                    factor: to.checked_div(*scale).map_err(|error| error.to_string())?,
                    curve,
                    range: Some(crate::FollowScaleRange {
                        start,
                        end,
                        from_factor: from
                            .unwrap_or(*scale)
                            .checked_div(*scale)
                            .map_err(|error| error.to_string())?,
                    }),
                }),
            },
        };
        framing.validate().map_err(|error| error.to_string())?;
        return Ok(Some(framing));
    }
    let Some((start, end)) = range else {
        if !matches!(kind, ZoomKind::Off)
            && matches!(
                entry,
                Some(
                    FramingValue::Envelope { .. }
                        | FramingValue::Follow {
                            scale_transition: Some(_),
                            ..
                        }
                )
            )
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
                        scale_transition: None,
                    },
                };
                framing.validate().map_err(|error| error.to_string())?;
                Ok(Some(framing))
            }
            ZoomKind::Creep { from, to, curve } => {
                if keep
                    && let Some(FramingValue::Follow {
                        target,
                        scale,
                        fallback,
                        ..
                    }) = entry
                {
                    if shot.whole_owner.is_none() || selected.is_some() {
                        return Err(FOLLOW_CREEP_REFUSAL.into());
                    }
                    let from = from.unwrap_or(*scale);
                    let framing = Framing {
                        clock: FramingClock::OwnerOutput,
                        value: FramingValue::Follow {
                            target: target.clone(),
                            scale: from,
                            fallback: FramingPose {
                                scale: fallback
                                    .scale
                                    .checked_mul(
                                        from.checked_div(*scale)
                                            .map_err(|error| error.to_string())?,
                                    )
                                    .map_err(|error| error.to_string())?,
                                ..*fallback
                            },
                            scale_transition: Some(crate::FollowScaleTransition {
                                factor: to.checked_div(from).map_err(|error| error.to_string())?,
                                curve,
                                range: None,
                            }),
                        },
                    };
                    framing.validate().map_err(|error| error.to_string())?;
                    return Ok(Some(framing));
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
    let initial = if start == ExactRatio::ZERO {
        inside_from
    } else {
        segments.push(FramingSegment {
            end: start,
            pose: inside_from,
            curve: FramingCurve::Step,
        });
        before
    };
    let stepped = curve == FramingCurve::Step;
    segments.push(FramingSegment {
        end,
        pose: if stepped && end != ExactRatio::ONE {
            after
        } else {
            inside_to
        },
        curve,
    });
    if end != ExactRatio::ONE {
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

#[cfg(test)]
mod tests;
