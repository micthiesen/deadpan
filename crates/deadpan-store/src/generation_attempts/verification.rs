//! Bound the whole retained bundle check, including repeated temporal inputs.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{BundleValidationReceipt, attempt_error};
use crate::generated_media::GeneratedMediaLimits;
use crate::object_storage::ObjectControl;
use crate::{ProjectStore, StoreError};

const BUNDLE_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(60);

impl ProjectStore {
    pub(crate) fn verify_bundle_objects(
        &self,
        receipt: &BundleValidationReceipt,
        limits: GeneratedMediaLimits,
    ) -> Result<(), StoreError> {
        let cancelled = AtomicBool::new(false);
        let deadline = Instant::now() + BUNDLE_VERIFICATION_TIMEOUT;
        self.verify_bundle_objects_controlled(
            receipt,
            limits,
            ObjectControl::bounded(deadline, &cancelled).with_closed(&self.generated_read_closed),
        )
        .map(|_| ())
    }

    /// Returns the number of distinct immutable objects actually verified. One
    /// control governs every open/copy/hash; it never restarts for another input.
    fn verify_bundle_objects_controlled(
        &self,
        receipt: &BundleValidationReceipt,
        limits: GeneratedMediaLimits,
        control: ObjectControl<'_>,
    ) -> Result<usize, StoreError> {
        control.check()?;
        receipt
            .validate_shape()
            .map_err(|error| attempt_error(&error.to_string()))?;
        let mut verified = BTreeMap::new();
        let objects = [
            receipt.native_object(),
            receipt.sampled_object(),
            receipt.provenance_object(),
        ]
        .into_iter()
        .chain(
            receipt
                .admission()
                .into_iter()
                .flat_map(|evidence| evidence.inputs().objects()),
        );
        for object in objects {
            control.check()?;
            if let Some(length) = verified.get(object.content()) {
                if *length != object.byte_length() {
                    return Err(attempt_error(
                        "shared bundle object has conflicting lengths",
                    ));
                }
                continue;
            }
            // Drop each private copy immediately, keeping peak snapshot storage
            // bounded by one object even for the full temporal context.
            drop(
                self.generated_storage
                    .snapshot_controlled(object, limits, control)?,
            );
            verified.insert(object.content(), object.byte_length());
        }
        control.check()?;
        Ok(verified.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation_attempts::{
        BundleAdmissionEvidence, BundleInputObjects, ValidatorIdentity,
    };
    use deadpan_core::{
        ColorPolicy, GeneratedContentId, GeneratedObjectRef, NodeId, PresentationBasis,
        ProjectDocument, ProjectId, RevisionId, SourceSpan, SourceTimeBase, SourceTimestamp,
    };
    use deadpan_jobs::{ExtensionGenerationPlan, NativeCandidateManifest, Sha256, VideoSpec};
    use std::io::Cursor;

    fn fixture() -> (tempfile::TempDir, ProjectStore, BundleValidationReceipt) {
        let wire: serde_json::Value = serde_json::from_str(include_str!(
            "../../../deadpan-jobs/tests/fixtures/generate_extension_v3.json"
        ))
        .unwrap();
        let plan: ExtensionGenerationPlan = serde_json::from_value(wire["plan"].clone()).unwrap();
        let video: VideoSpec =
            serde_json::from_value(wire["constraints"]["video"].clone()).unwrap();
        let candidate_wire: serde_json::Value = serde_json::from_str(include_str!(
            "../../../deadpan-jobs/tests/fixtures/completed_extension_v3.json"
        ))
        .unwrap();
        let candidate: NativeCandidateManifest =
            serde_json::from_value(candidate_wire["candidate"].clone()).unwrap();
        let document = ProjectDocument::new(
            ProjectId::new("verification").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 512,
                height: 320,
                frame_rate: video.frame_rate(),
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let mut store =
            ProjectStore::create(&scratch.path().join("verification.deadpan"), &document).unwrap();
        let mut objects = Vec::new();
        for bytes in [
            b"native".as_slice(),
            b"sampled",
            b"provenance",
            b"manifest",
            b"picture",
            b"signatures",
        ] {
            let object = GeneratedObjectRef::new(
                GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
                bytes.len() as u64,
            )
            .unwrap();
            store
                .promote_generated_object(&mut Cursor::new(bytes), &object, limits())
                .unwrap();
            objects.push(object);
        }
        let span = |video: &VideoSpec| {
            let time_base = SourceTimeBase::new(
                video.frame_rate().denominator(),
                video.frame_rate().numerator(),
            )
            .unwrap();
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base,
                },
                SourceTimestamp {
                    ticks: video.frames().frames(),
                    time_base,
                },
            )
            .unwrap()
        };
        let receipt = BundleValidationReceipt::new_extension(
            &candidate,
            objects[0].clone(),
            objects[1].clone(),
            objects[2].clone(),
            video.clone(),
            plan.clone(),
            ValidatorIdentity::new("test", "1").unwrap(),
            BundleAdmissionEvidence::new(
                span(&candidate.video),
                span(&video),
                BundleInputObjects::new_extension(
                    Sha256::new("a".repeat(64)).unwrap(),
                    objects[3].clone(),
                    vec![objects[4].clone(); plan.context_frame_count() as usize],
                    None,
                    objects[5].clone(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        (scratch, store, receipt)
    }

    fn limits() -> GeneratedMediaLimits {
        GeneratedMediaLimits::new(1024).unwrap()
    }

    #[test]
    fn repeated_context_verifies_each_distinct_file_once() {
        let (_scratch, store, receipt) = fixture();
        assert_eq!(
            receipt
                .admission()
                .unwrap()
                .inputs()
                .context()
                .unwrap()
                .len(),
            9
        );
        let cancelled = AtomicBool::new(false);
        let control =
            ObjectControl::bounded(Instant::now() + BUNDLE_VERIFICATION_TIMEOUT, &cancelled);
        assert_eq!(
            store
                .verify_bundle_objects_controlled(&receipt, limits(), control)
                .unwrap(),
            6
        );
        // A corrupt input still fails after the outputs were verified, despite
        // the repeated input identities being coalesced.
        let picture = receipt.admission().unwrap().inputs().context().unwrap()[0].clone();
        let path = store
            .package
            .join("Media/Generated")
            .join(format!("blake3-{}", picture.content().digest()));
        std::fs::remove_file(path).unwrap();
        assert!(
            store
                .verify_bundle_objects_controlled(&receipt, limits(), control)
                .is_err()
        );
    }

    #[test]
    fn complete_bundle_checks_share_deadline_and_cancellation() {
        let (_scratch, store, receipt) = fixture();
        let cancelled = AtomicBool::new(false);
        let expired = ObjectControl::bounded(Instant::now(), &cancelled);
        assert!(matches!(
            store.verify_bundle_objects_controlled(&receipt, limits(), expired),
            Err(StoreError::GeneratedMedia(
                crate::generated_media::GeneratedMediaError::DeadlineExceeded
            ))
        ));
        cancelled.store(true, std::sync::atomic::Ordering::Release);
        let cancelled_control =
            ObjectControl::bounded(Instant::now() + BUNDLE_VERIFICATION_TIMEOUT, &cancelled);
        assert!(matches!(
            store.verify_bundle_objects_controlled(&receipt, limits(), cancelled_control),
            Err(StoreError::GeneratedMedia(
                crate::generated_media::GeneratedMediaError::Cancelled
            ))
        ));
    }
}
