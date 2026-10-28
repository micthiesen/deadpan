//! Private allocation/issuer mechanics shared by actual context constructors.
use crate::{AudioSoundContextAdmission, PlanError};
use std::alloc::Layout;

pub(crate) fn array<T>(count: usize) -> Result<Layout, PlanError> {
    Layout::array::<T>(count).map_err(|_| PlanError::AudioQueryLimit("sound metadata layout"))
}
pub(crate) fn arc<T>() -> Result<Layout, PlanError> {
    Layout::new::<[usize; 2]>()
        .extend(Layout::new::<T>())
        .map(|(layout, _)| layout.pad_to_align())
        .map_err(|_| PlanError::AudioQueryLimit("sound metadata layout"))
}
pub(crate) fn identity(text: &str) -> Result<Layout, PlanError> {
    Layout::new::<[usize; 2]>()
        .extend(Layout::for_value(text))
        .map(|(layout, _)| layout.pad_to_align())
        .map_err(|_| PlanError::AudioQueryLimit("sound metadata layout"))
}
pub(crate) fn validate<A: AudioSoundContextAdmission>(
    issuer: A::Issuer,
    admission: &A,
) -> Result<(), A::Error> {
    if issuer != admission.issuer() {
        return Err(PlanError::AudioMetadataIssuerMismatch.into());
    }
    Ok(())
}
pub(crate) fn reserve<A: AudioSoundContextAdmission>(
    target: &mut Option<A::Lease>,
    admission: &mut A,
    issuer: A::Issuer,
    layout: Layout,
) -> Result<(), A::Error> {
    validate(issuer, admission)?;
    if layout.size() == 0 {
        return Ok(());
    }
    let mut issued = admission.reserve(layout)?;
    validate(issuer, admission)?;
    if let Some(target) = target {
        // Both tokens were issued only after validating this same stable issuer.
        admission.merge(target, &mut issued);
        validate(issuer, admission)?;
    } else {
        *target = Some(issued);
    }
    Ok(())
}
