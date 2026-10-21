//! Resolve a dimensionless speed against the captured beat's exact input span.

use super::Workspace;
use deadpan_core::{ExactRatio, FrameDuration, NodeId, NodeKind, PitchPolicy, RetimePurpose};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub input: ExactRatio,
    pub before: FrameDuration,
    pub after: FrameDuration,
    pub update: bool,
}

pub fn resolve(
    workspace: &Workspace,
    node: &NodeId,
    speed: ExactRatio,
    pitch: PitchPolicy,
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
            purpose: RetimePurpose::Edit,
            ..
        } if !wrap => (
            workspace
                .document
                .retime_update_input(node, pitch)
                .map_err(|error| error.to_string())?,
            true,
        ),
        _ => (ExactRatio::integer(before.frames()), false),
    };
    if input == ExactRatio::ZERO {
        return Err("An empty beat cannot be retimed.".into());
    }
    let frames = input
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
    // Preview and the inspector must be able to report the actual rounded
    // speed before a request can be submitted.
    input
        .checked_div(ExactRatio::integer(after.frames()))
        .map_err(|error| error.to_string())?;
    if update {
        workspace
            .document
            .validate_retime_update(node, after, pitch)
            .map_err(|error| error.to_string())?;
    }
    Ok(Resolution {
        input,
        before,
        after,
        update,
    })
}

impl Resolution {
    pub fn describe(self, pitch: deadpan_core::PitchPolicy) -> String {
        let speed = self
            .input
            .checked_div(ExactRatio::integer(self.after.frames()))
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
