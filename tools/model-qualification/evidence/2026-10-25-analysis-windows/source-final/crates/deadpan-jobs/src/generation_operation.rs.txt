//! Pure operation choice and a tagged wrapper around distinct generation plans.
//!
//! Endpoint presence and provider support are supplied by the host. These types
//! neither infer capabilities nor load models, and never change authored time.

use deadpan_core::{ExtensionDirection, FrameDuration, FrameRate};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    BridgeGenerationPlan, ConditioningMode, ExtensionGenerationPlan, NativeDimensions,
    ProtocolVersion,
};

/// Captured user preference. Automatic resolves once against actual endpoints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationModePreference {
    #[default]
    Automatic,
    Bridge,
    ExtendFromLeft,
    ExtendFromRight,
}

impl<'de> Deserialize<'de> for GenerationModePreference {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "automatic" => Ok(Self::Automatic),
            "bridge" => Ok(Self::Bridge),
            "extend_from_left" => Ok(Self::ExtendFromLeft),
            "extend_from_right" => Ok(Self::ExtendFromRight),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "automatic",
                    "bridge",
                    "extend_from_left",
                    "extend_from_right",
                ],
            )),
        }
    }
}

/// The provider's declared operation support. Default denies every operation.
/// This is not a measured capability or permission to advertise one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditioningSupport {
    pub bridge: bool,
    pub extend_from_left: bool,
    pub extend_from_right: bool,
}

impl ConditioningSupport {
    pub const fn supports(self, conditioning: ConditioningMode) -> bool {
        match conditioning {
            ConditioningMode::Bridge => self.bridge,
            ConditioningMode::ExtendFromLeft => self.extend_from_left,
            ConditioningMode::ExtendFromRight => self.extend_from_right,
        }
    }
}

impl GenerationModePreference {
    /// `true` means a picture exists in the definition, including authored
    /// black. A saved freeze recipe does not make an absent endpoint present.
    /// Automatic chooses by presence first; unsupported choices fail without
    /// falling back to a different conditioning operation.
    pub fn resolve(
        self,
        left_present: bool,
        right_present: bool,
        support: ConditioningSupport,
    ) -> Result<ConditioningMode, GenerationModeError> {
        let conditioning = match self {
            Self::Automatic => match (left_present, right_present) {
                (true, true) => ConditioningMode::Bridge,
                (true, false) => ConditioningMode::ExtendFromLeft,
                (false, true) => ConditioningMode::ExtendFromRight,
                (false, false) => return Err(GenerationModeError::NoEndpoints),
            },
            Self::Bridge => ConditioningMode::Bridge,
            Self::ExtendFromLeft => ConditioningMode::ExtendFromLeft,
            Self::ExtendFromRight => ConditioningMode::ExtendFromRight,
        };
        if !left_present
            && matches!(
                conditioning,
                ConditioningMode::Bridge | ConditioningMode::ExtendFromLeft
            )
        {
            return Err(GenerationModeError::MissingLeft { conditioning });
        }
        if !right_present
            && matches!(
                conditioning,
                ConditioningMode::Bridge | ConditioningMode::ExtendFromRight
            )
        {
            return Err(GenerationModeError::MissingRight { conditioning });
        }
        if !support.supports(conditioning) {
            return Err(GenerationModeError::Unsupported { conditioning });
        }
        Ok(conditioning)
    }

    /// Whether this exact preference can resolve with these endpoints/support.
    pub fn supports(
        self,
        left_present: bool,
        right_present: bool,
        support: ConditioningSupport,
    ) -> bool {
        self.resolve(left_present, right_present, support).is_ok()
    }

    /// Check preference compatibility with an already resolved operation.
    /// Automatic accepts the captured operation; this does not re-prove its
    /// endpoint availability or provider support. Initial admission uses resolve.
    pub fn validate_resolved(self, resolved: ConditioningMode) -> Result<(), GenerationModeError> {
        if self == Self::Automatic || self == Self::from(resolved) {
            Ok(())
        } else {
            Err(GenerationModeError::ResolvedMismatch {
                preference: self,
                resolved,
            })
        }
    }
}

impl From<ConditioningMode> for GenerationModePreference {
    fn from(value: ConditioningMode) -> Self {
        match value {
            ConditioningMode::Bridge => Self::Bridge,
            ConditioningMode::ExtendFromLeft => Self::ExtendFromLeft,
            ConditioningMode::ExtendFromRight => Self::ExtendFromRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GenerationModeError {
    #[error("AI generation has no conditioning picture in this definition")]
    NoEndpoints,
    #[error("{conditioning:?} requires a picture before the Hold in its definition")]
    MissingLeft { conditioning: ConditioningMode },
    #[error("{conditioning:?} requires a picture after the Hold in its definition")]
    MissingRight { conditioning: ConditioningMode },
    #[error("the provider does not declare support for {conditioning:?}")]
    Unsupported { conditioning: ConditioningMode },
    #[error("captured preference {preference:?} disagrees with resolved conditioning {resolved:?}")]
    ResolvedMismatch {
        preference: GenerationModePreference,
        resolved: ConditioningMode,
    },
}

/// Operation-tagged metadata. Each inner plan keeps its own unchanged wire
/// grammar, arithmetic and capability validation; this wrapper adds no defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "plan",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GenerationPlan {
    Bridge(BridgeGenerationPlan),
    Extension(ExtensionGenerationPlan),
}

impl From<BridgeGenerationPlan> for GenerationPlan {
    fn from(plan: BridgeGenerationPlan) -> Self {
        Self::Bridge(plan)
    }
}

impl From<ExtensionGenerationPlan> for GenerationPlan {
    fn from(plan: ExtensionGenerationPlan) -> Self {
        Self::Extension(plan)
    }
}

impl GenerationPlan {
    pub const fn project_frames(&self) -> FrameDuration {
        match self {
            Self::Bridge(plan) => plan.project_frames(),
            Self::Extension(plan) => plan.project_frames(),
        }
    }
    pub const fn project_frame_rate(&self) -> FrameRate {
        match self {
            Self::Bridge(plan) => plan.project_frame_rate(),
            Self::Extension(plan) => plan.project_frame_rate(),
        }
    }
    pub const fn native_frame_rate(&self) -> FrameRate {
        match self {
            Self::Bridge(plan) => plan.native_frame_rate(),
            Self::Extension(plan) => plan.native_frame_rate(),
        }
    }
    pub const fn native_dimensions(&self) -> NativeDimensions {
        match self {
            Self::Bridge(plan) => plan.native_dimensions(),
            Self::Extension(plan) => plan.native_dimensions(),
        }
    }
    pub const fn conditioning(&self) -> ConditioningMode {
        match self {
            Self::Bridge(_) => ConditioningMode::Bridge,
            Self::Extension(plan) => match plan.direction() {
                ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
                ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
            },
        }
    }
    pub const fn protocol(&self) -> ProtocolVersion {
        match self {
            Self::Bridge(_) => ProtocolVersion::V2,
            Self::Extension(_) => ProtocolVersion::V3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AxisLimits, BridgeCapability, DimensionLimits, ExtensionCapability, FrameCountFormula,
    };

    const ALL: ConditioningSupport = ConditioningSupport {
        bridge: true,
        extend_from_left: true,
        extend_from_right: true,
    };

    #[test]
    fn automatic_uses_real_presence_and_never_substitutes_an_unsupported_operation() {
        for (left, right, expected) in [
            (true, true, ConditioningMode::Bridge),
            (true, false, ConditioningMode::ExtendFromLeft),
            (false, true, ConditioningMode::ExtendFromRight),
        ] {
            assert_eq!(
                GenerationModePreference::Automatic.resolve(left, right, ALL),
                Ok(expected)
            );
            assert!(GenerationModePreference::Automatic.supports(left, right, ALL));
            let mut unsupported = ALL;
            match expected {
                ConditioningMode::Bridge => unsupported.bridge = false,
                ConditioningMode::ExtendFromLeft => unsupported.extend_from_left = false,
                ConditioningMode::ExtendFromRight => unsupported.extend_from_right = false,
            }
            assert_eq!(
                GenerationModePreference::Automatic.resolve(left, right, unsupported),
                Err(GenerationModeError::Unsupported {
                    conditioning: expected
                })
            );
        }
        assert_eq!(
            GenerationModePreference::Automatic.resolve(false, false, ALL),
            Err(GenerationModeError::NoEndpoints)
        );
        assert!(!GenerationModePreference::Automatic.supports(false, false, ALL));
        // Availability describes existence, not brightness or provider kind.
        let authored_black = Some(());
        let absent: Option<()> = None;
        assert_eq!(
            GenerationModePreference::Automatic.resolve(
                authored_black.is_some(),
                absent.is_some(),
                ALL
            ),
            Ok(ConditioningMode::ExtendFromLeft)
        );
        assert_eq!(
            GenerationModePreference::Automatic.resolve(
                authored_black.is_some(),
                authored_black.is_some(),
                ALL
            ),
            Ok(ConditioningMode::Bridge)
        );
    }

    #[test]
    fn explicit_modes_require_their_anchors_and_do_not_change_direction() {
        for mode in [
            ConditioningMode::Bridge,
            ConditioningMode::ExtendFromLeft,
            ConditioningMode::ExtendFromRight,
        ] {
            let preference = GenerationModePreference::from(mode);
            assert_eq!(preference.resolve(true, true, ALL), Ok(mode));
            assert_eq!(
                preference.resolve(true, true, ConditioningSupport::default()),
                Err(GenerationModeError::Unsupported { conditioning: mode })
            );
            for resolved in [
                ConditioningMode::Bridge,
                ConditioningMode::ExtendFromLeft,
                ConditioningMode::ExtendFromRight,
            ] {
                assert_eq!(
                    preference.validate_resolved(resolved).is_ok(),
                    mode == resolved
                );
                assert!(
                    GenerationModePreference::Automatic
                        .validate_resolved(resolved)
                        .is_ok()
                );
            }
        }
        assert_eq!(
            GenerationModePreference::ExtendFromLeft.resolve(false, true, ALL),
            Err(GenerationModeError::MissingLeft {
                conditioning: ConditioningMode::ExtendFromLeft
            })
        );
        assert_eq!(
            GenerationModePreference::ExtendFromRight.resolve(true, false, ALL),
            Err(GenerationModeError::MissingRight {
                conditioning: ConditioningMode::ExtendFromRight
            })
        );
        assert_eq!(
            GenerationModePreference::Bridge.resolve(true, false, ALL),
            Err(GenerationModeError::MissingRight {
                conditioning: ConditioningMode::Bridge
            })
        );
        assert_eq!(
            GenerationModePreference::Bridge.resolve(false, true, ALL),
            Err(GenerationModeError::MissingLeft {
                conditioning: ConditioningMode::Bridge
            })
        );
        assert_eq!(
            GenerationModePreference::ExtendFromLeft.resolve(true, false, ALL),
            Ok(ConditioningMode::ExtendFromLeft)
        );
        assert_eq!(
            GenerationModePreference::ExtendFromRight.resolve(false, true, ALL),
            Ok(ConditioningMode::ExtendFromRight)
        );
    }

    #[test]
    fn mode_and_declared_support_have_closed_wire_shapes() {
        for (mode, name) in [
            (GenerationModePreference::Automatic, "automatic"),
            (GenerationModePreference::Bridge, "bridge"),
            (GenerationModePreference::ExtendFromLeft, "extend_from_left"),
            (
                GenerationModePreference::ExtendFromRight,
                "extend_from_right",
            ),
        ] {
            let wire = serde_json::to_value(mode).unwrap();
            assert_eq!(wire, name);
            assert_eq!(
                serde_json::from_value::<GenerationModePreference>(wire).unwrap(),
                mode
            );
        }
        for invalid in [
            serde_json::json!("auto"),
            serde_json::json!("extend"),
            serde_json::json!(null),
            serde_json::json!({"automatic":null}),
            serde_json::json!(["bridge"]),
        ] {
            assert!(serde_json::from_value::<GenerationModePreference>(invalid).is_err());
        }
        assert_eq!(
            serde_json::from_value::<ConditioningSupport>(serde_json::to_value(ALL).unwrap())
                .unwrap(),
            ALL
        );
        assert!(
            serde_json::from_value::<ConditioningSupport>(serde_json::json!({"bridge":true}))
                .is_err()
        );
        let mut unknown = serde_json::to_value(ALL).unwrap();
        unknown["image_to_video"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ConditioningSupport>(unknown).is_err());
    }

    fn plans() -> Vec<GenerationPlan> {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let native = FrameRate::new(24, 1).unwrap();
        let dimensions = DimensionLimits::new(
            AxisLimits::new(768, 768, 64).unwrap(),
            AxisLimits::new(320, 320, 64).unwrap(),
        );
        let raster = NativeDimensions::new(768, 320).unwrap();
        let frames = FrameDuration::new(8).unwrap();
        let mut plans = vec![GenerationPlan::Bridge(
            BridgeGenerationPlan::new(
                FrameDuration::new(12).unwrap(),
                rate,
                &BridgeCapability::new(
                    true,
                    native,
                    FrameCountFormula::new(8, 1, 9, 97).unwrap(),
                    dimensions,
                ),
                raster,
            )
            .unwrap(),
        )];
        let extension = ExtensionCapability::new(
            native,
            9,
            FrameCountFormula::new(8, 0, 8, 8).unwrap(),
            dimensions,
            FrameDuration::new(9).unwrap(),
        )
        .unwrap();
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            plans.push(GenerationPlan::Extension(
                ExtensionGenerationPlan::new(direction, frames, rate, &extension, raster).unwrap(),
            ));
        }
        plans
    }

    #[test]
    fn tagged_plan_preserves_each_inner_wire_and_getter_semantics() {
        for plan in plans() {
            let wire = serde_json::to_value(&plan).unwrap();
            assert_eq!(
                serde_json::from_value::<GenerationPlan>(wire.clone()).unwrap(),
                plan
            );
            match &plan {
                GenerationPlan::Bridge(inner) => {
                    assert_eq!(wire["operation"], "bridge");
                    assert_eq!(wire["plan"], serde_json::to_value(inner).unwrap());
                    assert_eq!(plan.project_frames(), inner.project_frames());
                    assert_eq!(plan.project_frame_rate(), inner.project_frame_rate());
                    assert_eq!(plan.native_frame_rate(), inner.native_frame_rate());
                    assert_eq!(plan.native_dimensions(), inner.native_dimensions());
                    assert_eq!(plan.conditioning(), ConditioningMode::Bridge);
                    assert_eq!(plan.protocol(), ProtocolVersion::V2);
                }
                GenerationPlan::Extension(inner) => {
                    assert_eq!(wire["operation"], "extension");
                    assert_eq!(wire["plan"], serde_json::to_value(inner).unwrap());
                    assert_eq!(plan.project_frames(), inner.project_frames());
                    assert_eq!(plan.project_frame_rate(), inner.project_frame_rate());
                    assert_eq!(plan.native_frame_rate(), inner.native_frame_rate());
                    assert_eq!(plan.native_dimensions(), inner.native_dimensions());
                    assert_eq!(
                        plan.conditioning(),
                        match inner.direction() {
                            ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
                            ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
                        }
                    );
                    assert_eq!(plan.protocol(), ProtocolVersion::V3);
                }
            }
            for field in ["operation", "plan"] {
                let mut missing = wire.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<GenerationPlan>(missing).is_err());
            }
            let mut unknown = wire.clone();
            unknown["operation"] = serde_json::json!("image_to_video");
            assert!(serde_json::from_value::<GenerationPlan>(unknown).is_err());
            let mut extra = wire.clone();
            extra["unused"] = serde_json::json!(null);
            assert!(serde_json::from_value::<GenerationPlan>(extra).is_err());
            let mut invalid_inner = wire.clone();
            invalid_inner["plan"]["schema_version"] = serde_json::json!(99);
            assert!(
                serde_json::from_value::<GenerationPlan>(invalid_inner).is_err(),
                "the wrapper must retain inner plan validation"
            );
            let mut wrong = wire;
            wrong["operation"] = serde_json::json!(match plan {
                GenerationPlan::Bridge(_) => "extension",
                GenerationPlan::Extension(_) => "bridge",
            });
            assert!(serde_json::from_value::<GenerationPlan>(wrong).is_err());
        }
    }
}
