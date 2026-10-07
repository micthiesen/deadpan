//! Bounded user intent for a local AI pause. Timing and provider identity stay
//! in the request's existing constraints and binding.

use deadpan_core::TargetId;
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
#[serde(
    tag = "selection",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GenerationTarget {
    #[default]
    Inherit,
    None,
    Saved(TargetId),
}

impl GenerationTarget {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value == "none" {
            Ok(Self::None)
        } else {
            TargetId::new(value)
                .map(Self::Saved)
                .map_err(|error| error.to_string())
        }
    }

    pub fn resolve(&self, previous: Option<&TargetId>) -> Option<TargetId> {
        match self {
            Self::Inherit => previous.cloned(),
            Self::None => None,
            Self::Saved(target) => Some(target.clone()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationOptions {
    #[serde(default)]
    pub motion: MotionAmount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<HoldInstructions>,
    #[serde(default)]
    pub region_target: GenerationTarget,
}

impl GenerationOptions {
    pub fn from_constraints(constraints: &HoldConstraints) -> Self {
        Self {
            motion: constraints.motion,
            instructions: constraints.instructions.clone(),
            region_target: constraints
                .region_target
                .clone()
                .map_or(GenerationTarget::None, GenerationTarget::Saved),
        }
    }

    /// Set the controls on prepared worker input without changing its timing.
    pub fn apply_to(&self, constraints: &mut HoldConstraints) {
        constraints.motion = self.motion;
        constraints.instructions.clone_from(&self.instructions);
        constraints.region_target = self
            .region_target
            .resolve(constraints.region_target.as_ref());
    }

    /// Resolve omission once, before asynchronous conditioning begins.
    pub fn resolve_target(&mut self, previous: Option<&TargetId>) {
        self.region_target = self
            .region_target
            .resolve(previous)
            .map_or(GenerationTarget::None, GenerationTarget::Saved);
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

    #[test]
    fn resolving_omission_captures_presence_or_absence_once() {
        let target = TargetId::new("subject").unwrap();
        let later = TargetId::new("later").unwrap();
        let mut inherited = GenerationOptions::default();
        inherited.resolve_target(Some(&target));
        assert_eq!(
            inherited.region_target,
            GenerationTarget::Saved(target.clone())
        );
        inherited.resolve_target(Some(&later));
        assert_eq!(inherited.region_target, GenerationTarget::Saved(target));
        let mut absent = GenerationOptions::default();
        absent.resolve_target(None);
        absent.resolve_target(Some(&later));
        assert_eq!(absent.region_target, GenerationTarget::None);
        let mut cleared = GenerationOptions {
            region_target: GenerationTarget::None,
            ..GenerationOptions::default()
        };
        cleared.resolve_target(Some(&later));
        assert_eq!(cleared.region_target, GenerationTarget::None);
        assert!(GenerationTarget::parse("").is_err());
    }
}
