//! Resolve a dimensionless speed against the captured beat's exact input span.

use super::Workspace;
use deadpan_core::{ExactRatio, FrameDuration, NodeId, NodeKind, RetimePurpose};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub input: FrameDuration,
    pub before: FrameDuration,
    pub after: FrameDuration,
    pub update: bool,
}

pub fn resolve(
    workspace: &Workspace,
    node: &NodeId,
    speed: ExactRatio,
    wrap: bool,
) -> Result<Resolution, String> {
    if !speed.compare_integer(0).is_gt() {
        return Err("Retime speed must be positive.".into());
    }
    let beat = workspace
        .document
        .nodes()
        .get(node)
        .ok_or("The selected beat no longer exists.")?;
    let before = workspace
        .plan
        .node_duration(node)
        .ok_or("The selected beat has no resolved duration.")?;
    let (input, update) = match &beat.kind {
        NodeKind::Retime {
            mapping,
            purpose: RetimePurpose::Edit,
            ..
        } if !wrap => (mapping.duration(), true),
        _ => (before, false),
    };
    if input == FrameDuration::ZERO {
        return Err("An empty beat cannot be retimed.".into());
    }
    let frames = ExactRatio::integer(input.frames())
        .checked_div(speed)
        .and_then(ExactRatio::round_even)
        .map_err(|error| error.to_string())?;
    let frames =
        i64::try_from(frames).map_err(|_| "Retime duration exceeds the supported frame range.")?;
    if frames == 0 {
        return Err(
            "This speed resolves to 0 frames. Choose a slower speed; no edit was made.".into(),
        );
    }
    let after = FrameDuration::new(frames).map_err(|error| error.to_string())?;
    Ok(Resolution {
        input,
        before,
        after,
        update,
    })
}

impl Resolution {
    pub fn describe(self, pitch: deadpan_core::PitchPolicy) -> String {
        let speed = ExactRatio::new(
            i128::from(self.input.frames()),
            i128::from(self.after.frames()),
        )
        .expect("positive resolved frame durations");
        let pitch = match pitch {
            deadpan_core::PitchPolicy::Preserve => "preserve pitch".to_owned(),
            deadpan_core::PitchPolicy::FollowSpeed => "tape pitch".to_owned(),
            deadpan_core::PitchPolicy::Shift { semitones } => {
                format!("pitch {semitones:+} semitones")
            }
        };
        format!(
            "{} to {} f · {}/{}× · {pitch} · {}",
            self.before.frames(),
            self.after.frames(),
            speed.numerator(),
            speed.denominator(),
            if self.update {
                "update this Retime"
            } else {
                "wrap selected beat"
            }
        )
    }
}
