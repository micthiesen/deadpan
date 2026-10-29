//! Persistent caller stop keys and their actual repeat-array admission.
use super::*;

/// The persistent stop key and its actual repeat-array charge move together.
/// Read-only dereferencing preserves existing borrowed renderer consumers.
pub(in crate::stages) struct AdmittedStop<I, L> {
    value: Boundary,
    lease: Option<L>,
    issuer: I,
}

impl<I, L> std::ops::Deref for AdmittedStop<I, L> {
    type Target = Boundary;
    fn deref(&self) -> &Boundary {
        &self.value
    }
}

impl<I: Copy + Eq, L> AdmittedStop<I, L> {
    pub(in crate::stages) fn issuer(&self) -> I {
        self.issuer
    }
    pub(in crate::stages) fn lease(&self) -> Option<&L> {
        self.lease.as_ref()
    }
    pub(in crate::stages) fn with_lease_mut(&mut self, commit: impl FnOnce(Option<&mut L>)) {
        commit(self.lease.as_mut());
    }
}

impl<I: Copy + Eq, L> AdmittedStop<I, L> {
    /// Copy an exact borrowed boundary into an admitted tile-local snapshot.
    /// The array lease stays with the copied value and drops after it.
    pub(in crate::stages) fn from_boundary<A>(
        boundary: &Boundary,
        admission: &mut A,
    ) -> Result<Self, StageAudioError>
    where
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError, Issuer = I, Lease = L>,
    {
        let issuer = admission.issuer();
        if !A::METERED {
            return Ok(Self {
                value: boundary.clone(),
                lease: None,
                issuer,
            });
        }

        let spend = |admission: &mut A| -> Result<(), StageAudioError> {
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            admission.spend(1)?;
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            Ok(())
        };

        // This snapshot also reads a caller-supplied before-stage scalar,
        // unlike `new`, where that value was already computed by the caller.
        // Account for the scalar and repeat-length inspections separately.
        spend(admission)?;
        let mut result = Self {
            value: Boundary {
                owner: OwnerKey {
                    instance: InstancePath {
                        node: boundary.owner.instance.node.clone(),
                        repeats: Vec::new(),
                    },
                    gap: false,
                    after: None,
                },
                before: 0,
            },
            lease: None,
            issuer,
        };
        spend(admission)?;
        result.value.before = boundary.before;
        spend(admission)?;
        let repeats = &boundary.owner.instance.repeats;
        if !repeats.is_empty() {
            let layout = std::alloc::Layout::array::<deadpan_core::RepeatInstance>(repeats.len())
                .map_err(|_| StageAudioError::Range)?;
            result.lease = Some(admission.reserve(layout)?);
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            let copied = &mut result.value.owner.instance.repeats;
            *copied = Vec::with_capacity(repeats.len());
            assert_eq!(copied.capacity(), repeats.len());
            for value in repeats {
                spend(admission)?;
                copied.push(value.clone());
            }
        }
        spend(admission)?;
        result.value.owner.gap = boundary.owner.gap;
        spend(admission)?;
        result.value.owner.after = boundary.owner.after.clone();
        Ok(result)
    }
}

#[cfg(test)]
#[path = "caller_key_tests.rs"]
mod tests;

impl<I: Copy + Eq, L> AdmittedStop<I, L> {
    pub(in crate::stages) fn new<A>(
        owner: &AudioOwnerClock<'_>,
        before: usize,
        admission: &mut A,
    ) -> Result<Self, StageAudioError>
    where
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError, Issuer = I, Lease = L>,
    {
        let issuer = admission.issuer();
        if !A::METERED {
            return Ok(Self {
                value: Boundary {
                    owner: OwnerKey::new(owner),
                    before,
                },
                lease: None,
                issuer,
            });
        }
        let spend = |admission: &mut A| -> Result<(), StageAudioError> {
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            admission.spend(1)?;
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            Ok(())
        };
        spend(admission)?;
        let mut result = Self {
            value: Boundary {
                owner: OwnerKey {
                    instance: InstancePath {
                        node: owner.instance().node.clone(),
                        repeats: Vec::new(),
                    },
                    gap: false,
                    after: None,
                },
                before,
            },
            lease: None,
            issuer,
        };
        spend(admission)?;
        let repeats = &owner.instance().repeats;
        if !repeats.is_empty() {
            let layout = std::alloc::Layout::array::<deadpan_core::RepeatInstance>(repeats.len())
                .map_err(|_| StageAudioError::Range)?;
            result.lease = Some(admission.reserve(layout)?);
            if admission.issuer() != issuer {
                return Err(PlanError::AudioMetadataIssuerMismatch.into());
            }
            let copied = &mut result.value.owner.instance.repeats;
            *copied = Vec::with_capacity(repeats.len());
            assert_eq!(copied.capacity(), repeats.len());
            for value in repeats {
                spend(admission)?;
                copied.push(value.clone());
            }
        }
        spend(admission)?;
        result.value.owner.gap = owner.kind() == deadpan_plan::AudioOwnerKind::DefaultGap;
        spend(admission)?;
        result.value.owner.after = owner.gap_after().cloned();
        Ok(result)
    }
}
