use deadpan_core::FrameRate;
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, GenerationPlanError,
    ProviderSelection,
};
use serde::{Deserialize, Serialize};

/// Host-selected provider contract, separate from the worker's request/claims.
/// A future installed-pack manager must supply and attest this selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProviderWire", into = "ProviderWire")]
pub struct SelectedBridgeProvider {
    selection: ProviderSelection,
    capability: BridgeCapability,
}

impl SelectedBridgeProvider {
    pub const fn new(selection: ProviderSelection, capability: BridgeCapability) -> Self {
        Self {
            selection,
            capability,
        }
    }
    pub fn selection(&self) -> &ProviderSelection {
        &self.selection
    }
    pub const fn capability(&self) -> &BridgeCapability {
        &self.capability
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderWire {
    selection: ProviderSelection,
    supported: bool,
    native_frame_rate: FrameRate,
    frame_counts: CountsWire,
    width: AxisWire,
    height: AxisWire,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountsWire {
    step: u32,
    offset: u32,
    minimum: u32,
    maximum: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AxisWire {
    minimum: u32,
    maximum: u32,
    multiple: u32,
}

impl From<AxisLimits> for AxisWire {
    fn from(axis: AxisLimits) -> Self {
        Self {
            minimum: axis.minimum(),
            maximum: axis.maximum(),
            multiple: axis.multiple(),
        }
    }
}

impl TryFrom<ProviderWire> for SelectedBridgeProvider {
    type Error = GenerationPlanError;
    fn try_from(wire: ProviderWire) -> Result<Self, Self::Error> {
        Ok(Self::new(
            wire.selection,
            BridgeCapability::new(
                wire.supported,
                wire.native_frame_rate,
                FrameCountFormula::new(
                    wire.frame_counts.step,
                    wire.frame_counts.offset,
                    wire.frame_counts.minimum,
                    wire.frame_counts.maximum,
                )?,
                DimensionLimits::new(
                    AxisLimits::new(wire.width.minimum, wire.width.maximum, wire.width.multiple)?,
                    AxisLimits::new(
                        wire.height.minimum,
                        wire.height.maximum,
                        wire.height.multiple,
                    )?,
                ),
            ),
        ))
    }
}

impl From<SelectedBridgeProvider> for ProviderWire {
    fn from(value: SelectedBridgeProvider) -> Self {
        let capability = value.capability;
        let counts = capability.frame_counts();
        Self {
            selection: value.selection,
            supported: capability.supported(),
            native_frame_rate: capability.native_frame_rate(),
            frame_counts: CountsWire {
                step: counts.step(),
                offset: counts.offset(),
                minimum: counts.minimum(),
                maximum: counts.maximum(),
            },
            width: capability.dimensions().width().into(),
            height: capability.dimensions().height().into(),
        }
    }
}
