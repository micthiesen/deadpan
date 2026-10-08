//! Prepared pixels retain the operation that selected their capture policy.

use deadpan_jobs::{GenerationPlan, HoldConstraints, Sha256};

use super::{BridgeInputs, ExtensionInputs};

#[derive(Debug, Clone)]
pub enum PreparedInputs {
    Bridge(Box<BridgeInputs>),
    Extension(Box<ExtensionInputs>),
}

impl From<BridgeInputs> for PreparedInputs {
    fn from(value: BridgeInputs) -> Self {
        Self::Bridge(Box::new(value))
    }
}

impl From<ExtensionInputs> for PreparedInputs {
    fn from(value: ExtensionInputs) -> Self {
        Self::Extension(Box::new(value))
    }
}

impl PreparedInputs {
    pub fn plan(&self) -> GenerationPlan {
        match self {
            Self::Bridge(inputs) => GenerationPlan::Bridge(inputs.plan.clone()),
            Self::Extension(inputs) => GenerationPlan::Extension(inputs.plan.clone()),
        }
    }

    pub fn constraints(&self) -> &HoldConstraints {
        match self {
            Self::Bridge(inputs) => &inputs.constraints,
            Self::Extension(inputs) => &inputs.constraints,
        }
    }

    pub fn manifest(&self) -> &[u8] {
        match self {
            Self::Bridge(inputs) => &inputs.manifest,
            Self::Extension(inputs) => &inputs.manifest,
        }
    }

    pub fn manifest_sha256(&self) -> &Sha256 {
        match self {
            Self::Bridge(inputs) => &inputs.manifest_sha256,
            Self::Extension(inputs) => &inputs.manifest_sha256,
        }
    }
}
