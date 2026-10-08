//! Pixel rejection checks for one-sided extensions. This is one part of output
//! qualification, not a Ready receipt or permission to accept generated media.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::{ExtensionGenerationPlan, HostMessage, MotionAmount};
use deadpan_media::CanonicalExtension;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::quality_input::Control;
use crate::{
    ExtensionConditioningReceipt, ExtensionContext, ExtensionEndpointReport,
    ExtensionGenerationBinding, ExtensionMotionReport, QualificationError,
    RetainedExtensionConditioning,
};

const PROFILE: &str = "deadpan-extension-pixels-1";

/// Retained pixel observations bound to private native/sampled objects, the
/// exact extension plan and the retained input receipt. No measured rejection
/// is not a guarantee of visual continuity or identity. Face, mouth and selected
/// region checks, worker provenance and durable candidate admission are separate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionPixelReport {
    schema_version: u32,
    profile: String,
    motion: ExtensionMotionReport,
    endpoints: ExtensionEndpointReport,
}

impl ExtensionPixelReport {
    pub fn motion(&self) -> &ExtensionMotionReport {
        &self.motion
    }

    pub fn endpoints(&self) -> &ExtensionEndpointReport {
        &self.endpoints
    }

    /// Check identity, complete coverage and recomputed rejection rules against
    /// the original retained inputs. This validates observations; it does not
    /// decode the media again or grant store/acceptance authority.
    pub fn validate_for(
        &self,
        media: &CanonicalExtension,
        conditioning: &RetainedExtensionConditioning,
        request: &HostMessage,
    ) -> Result<(), QualificationError> {
        validate_inputs(media, conditioning, request)?;
        let binding = ExtensionGenerationBinding::from_request(request)?;
        self.validate_bound(
            (&media.native().report().video, media.native().object()),
            (&media.sampled().report().video, media.sampled().object()),
            conditioning.context(),
            conditioning.receipt(),
            &binding,
        )
    }

    /// Validate saved observations and exact contracts without opening media.
    /// The caller owns admission of manifest and media bytes to these identities.
    pub(crate) fn validate_bound(
        &self,
        native: (&VideoContract, &GeneratedObjectRef),
        sampled: (&VideoContract, &GeneratedObjectRef),
        context: &ExtensionContext,
        receipt: &ExtensionConditioningReceipt,
        binding: &ExtensionGenerationBinding,
    ) -> Result<(), QualificationError> {
        receipt.validate_binding(context, binding)?;
        let plan = &binding.plan;
        let motion = binding.constraints.motion;
        if *native.0 != crate::extension_motion::native_contract(plan)
            || *sampled.0 != crate::extension_endpoints::sampled_contract(plan)?
        {
            return Err(invalid(
                "native/sampled contracts differ from the captured extension plan",
            ));
        }
        if self.schema_version != 1 || self.profile != PROFILE {
            return Err(invalid("unsupported extension pixel report"));
        }
        self.motion.validate(plan, native.1, motion)?;
        self.endpoints
            .validate(plan, sampled.1, context, receipt, motion)
    }
}

/// Inspect the private output of `canonicalize_extension` after the model
/// worker has been stopped and reaped. Capture conditioning before that worker
/// runs; never replace it with a fresh capture of its writable workspace.
///
/// Every generated-native adjacent pair is inspected, including pairs omitted
/// by downsampling. Actual sampled joins use the retained PNGs. The unconditioned
/// opposite join stays explicitly unconditioned and an absent join is unmeasured.
/// One deadline covers both stages; run outside UI/audio/database transactions.
pub fn inspect_extension_pixels(
    media: &CanonicalExtension,
    conditioning: &mut RetainedExtensionConditioning,
    request: &HostMessage,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ExtensionPixelReport, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    control.remaining()?;
    let (plan, motion) = validate_inputs(media, conditioning, request)?;
    // Join checks decode at most two sampled pictures and can reject before
    // the complete native-generated inspection needs to run.
    let endpoints = crate::extension_endpoints::measure(
        media.sampled(),
        conditioning,
        plan,
        motion,
        deadline,
        cancelled,
    )?;
    control.remaining()?;
    let motion =
        crate::extension_motion::measure(media.native(), plan, motion, deadline, cancelled)?;
    control.remaining()?;
    let report = ExtensionPixelReport {
        schema_version: 1,
        profile: PROFILE.into(),
        motion,
        endpoints,
    };
    report.validate_for(media, conditioning, request)?;
    control.remaining()?;
    Ok(report)
}

fn validate_inputs<'a>(
    media: &CanonicalExtension,
    conditioning: &RetainedExtensionConditioning,
    request: &'a HostMessage,
) -> Result<(&'a ExtensionGenerationPlan, MotionAmount), QualificationError> {
    request
        .validate()
        .map_err(|error| QualificationError::Request(error.to_string()))?;
    let HostMessage::GenerateExtension {
        plan, constraints, ..
    } = request
    else {
        return Err(invalid("pixel inspection requires an extension request"));
    };
    conditioning.validate_for(request)?;
    if media.sampling() != plan.sampling_map()
        || media.native().report().video != crate::extension_motion::native_contract(plan)
        || media.sampled().report().video != crate::extension_endpoints::sampled_contract(plan)?
    {
        return Err(invalid(
            "private native/sampled media differs from the captured extension plan",
        ));
    }
    Ok((plan, constraints.motion))
}

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(format!("{PROFILE}: {}", reason.to_string()))
}
