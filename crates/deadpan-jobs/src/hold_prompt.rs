//! Bounded user intent for a local AI pause. Timing and provider identity stay
//! in the request's existing constraints and binding.

use serde::{Deserialize, Serialize};

use crate::{HoldConstraints, MotionAmount, ValueError};

pub const MAX_HOLD_INSTRUCTION_BYTES: usize = 512;

impl MotionAmount {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Still => "still",
            Self::Subtle => "subtle",
            Self::Moderate => "moderate",
        }
    }
}

impl std::str::FromStr for MotionAmount {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "still" => Ok(Self::Still),
            "subtle" => Ok(Self::Subtle),
            "moderate" => Ok(Self::Moderate),
            _ => Err("Motion must be still, subtle or moderate."),
        }
    }
}

/// A short plain-language constraint added to the versioned pause prompt.
/// It is model input only, never a command, path or executable configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HoldInstructions(String);

impl HoldInstructions {
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        let value = value.into();
        if value.len() > MAX_HOLD_INSTRUCTION_BYTES
            || value.trim().is_empty()
            || value.chars().any(char::is_control)
        {
            return Err(ValueError::InvalidHoldInstructions);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for HoldInstructions {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<HoldInstructions> for String {
    fn from(value: HoldInstructions) -> Self {
        value.0
    }
}

/// Captured generation controls, independent of the pause's exact duration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationOptions {
    #[serde(default)]
    pub motion: MotionAmount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<HoldInstructions>,
}

impl GenerationOptions {
    pub fn from_constraints(constraints: &HoldConstraints) -> Self {
        Self {
            motion: constraints.motion,
            instructions: constraints.instructions.clone(),
        }
    }

    /// Set the controls on prepared worker input without changing its timing.
    pub fn apply_to(&self, constraints: &mut HoldConstraints) {
        constraints.motion = self.motion;
        constraints.instructions.clone_from(&self.instructions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_are_bounded_utf8_text_and_never_control_characters() {
        for text in [
            "Keep the hands still.",
            "目線をそのまま保つ。",
            "a".repeat(512).as_str(),
        ] {
            let value = HoldInstructions::new(text).unwrap();
            assert_eq!(value.as_str(), text);
            assert_eq!(
                serde_json::from_str::<HoldInstructions>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
        for text in [
            "",
            "  ",
            "\n",
            "a\0b",
            "a\tb",
            "a\u{0085}b",
            &"a".repeat(513),
            &"界".repeat(171),
        ] {
            assert!(HoldInstructions::new(text).is_err(), "{text:?}");
            assert!(serde_json::from_value::<HoldInstructions>(serde_json::json!(text)).is_err());
        }
        assert!(
            serde_json::from_value::<GenerationOptions>(
                serde_json::json!({"motion":"still","unknown":1})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<GenerationOptions>(serde_json::json!({"motion":"fast"}))
                .is_err()
        );
        assert_eq!(
            serde_json::from_value::<GenerationOptions>(serde_json::json!({})).unwrap(),
            GenerationOptions::default()
        );
    }
}
