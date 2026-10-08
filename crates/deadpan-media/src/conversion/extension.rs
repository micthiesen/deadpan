use super::*;
use crate::protocol::ExtensionConversionRequest;
use deadpan_core::ExtensionSamplingMap;

/// Private verified complete native movie and generated-only sampled master.
/// Context handles remain in `native` for provenance, never in `sampled`.
pub struct CanonicalExtension {
    native: CanonicalMedia,
    sampled: CanonicalMedia,
    sampling: ExtensionSamplingMap,
    source_identity: InputIdentity,
}

impl CanonicalExtension {
    pub fn native(&self) -> &CanonicalMedia {
        &self.native
    }

    pub fn sampled(&self) -> &CanonicalMedia {
        &self.sampled
    }

    pub fn sampling(&self) -> &ExtensionSamplingMap {
        &self.sampling
    }

    pub fn source_identity(&self) -> InputIdentity {
        self.source_identity
    }

    pub fn into_parts(self) -> (CanonicalMedia, CanonicalMedia, ExtensionSamplingMap) {
        (self.native, self.sampled, self.sampling)
    }
}

/// Sample only the generated interval into exactly the authored output count.
/// Shares [`canonicalize`]'s private snapshot, process ownership, verification
/// and deadline guarantees. The extension map must be retained on acceptance.
pub fn sample_extension(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &ExtensionConversionRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalMedia, ConversionError> {
    convert(
        executable,
        source,
        identity,
        &WorkerRequest::Extension(request.clone()),
        cancelled,
    )
}

/// Preserve the complete native movie and produce the generated-only master.
/// One immutable input snapshot and one deadline cover both helper processes,
/// their independent decode checks and the final object hashes. Byte budgets
/// apply per file, and native RGB scratch is used sequentially.
pub fn canonicalize_extension(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &ExtensionConversionRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalExtension, ConversionError> {
    request.validate()?;
    let deadline = Deadline {
        end: Instant::now() + Duration::from_millis(request.limits.timeout_ms),
        cancelled,
    };
    deadline.check()?;
    let mut input = snapshot(source, identity, request.input_byte_length, &deadline)?;
    input.rewind()?;
    let native_request = WorkerRequest::Convert(ConversionRequest {
        protocol: PROTOCOL_VERSION,
        video: request.native,
        input_byte_length: request.input_byte_length,
        limits: request.limits,
    });
    let native = convert_snapshot(executable, input.try_clone()?, &native_request, &deadline)?;
    input.rewind()?;
    let sampled = convert_snapshot(
        executable,
        input,
        &WorkerRequest::Extension(request.clone()),
        &deadline,
    )?;
    if native.report.output_rgb_sha256 != sampled.report.input_rgb_sha256 {
        return Err(ConversionError::Protocol(
            "native and sampled masters decoded different source pixels".into(),
        ));
    }
    deadline.check()?;
    Ok(CanonicalExtension {
        native,
        sampled,
        sampling: request.sampling.clone(),
        source_identity: identity,
    })
}
