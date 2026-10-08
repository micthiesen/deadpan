//! Actual canonicalization/Vision with synthetic worker loading claims. These
//! fixtures exercise candidate admission and never claim real-model quality.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use deadpan_core::{FrameDuration, GeneratedContentId, GeneratedObjectRef};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AxisLimits, CandidateDeclaration, DimensionLimits, ExtensionCapability, FrameCountFormula,
    NativeCandidateManifest, RequestVersion, Sha256, VideoSpec, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::protocol::ConversionLimits;
use deadpan_models::{
    ExtensionGenerationBinding, ExtensionQualification, QualificationLimits,
    QualifiedExtensionBundle, RetainedExtensionConditioning, SelectedExtensionProvider,
    StoredExtensionProvenance, ValidatedExtensionEvidence, qualify_extension,
};
use serde_json::{Value, json};
use sha2::Digest;

use super::*;

struct Prepared {
    directory: tempfile::TempDir,
    request: HostMessage,
    conditioning: RetainedExtensionConditioning,
    selected: SelectedExtensionProvider,
    declaration: CandidateDeclaration,
    context_bytes: Vec<u8>,
}

impl Prepared {
    fn from_fixture(fixture: Fixture) -> Self {
        let Fixture {
            directory,
            request,
            conditioning,
            native,
            ..
        } = fixture;
        Self::new(directory, request, conditioning, &native)
    }

    fn from_native(native: &[u8]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let (request, conditioning) = retained_inputs(
            directory.path(),
            &plan(ExtensionDirection::FromLeft, 1),
            true,
        );
        Self::new(directory, request, conditioning, native)
    }

    fn new(
        directory: tempfile::TempDir,
        request: HostMessage,
        conditioning: RetainedExtensionConditioning,
        native: &[u8],
    ) -> Self {
        let binding = ExtensionGenerationBinding::from_request(&request).unwrap();
        let plan = &binding.plan;
        let dimensions = plan.native_dimensions();
        let selected = SelectedExtensionProvider::new(
            binding.provider.clone(),
            ExtensionCapability::new(
                plan.native_frame_rate(),
                plan.context_frame_count(),
                FrameCountFormula::new(8, 0, GENERATED, GENERATED).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(dimensions.width(), dimensions.width(), 1).unwrap(),
                    AxisLimits::new(dimensions.height(), dimensions.height(), 1).unwrap(),
                ),
                FrameDuration::new(8).unwrap(),
            )
            .unwrap(),
        );
        let context_bytes =
            fs::read(directory.path().join(binding.input.manifest.as_str())).unwrap();
        fs::write(directory.path().join("outputs/native.mp4"), native).unwrap();
        let mut declaration = NativeCandidateManifest {
            native: artifact("outputs/native.mp4", native),
            provenance: artifact("outputs/provenance.json", b"temporary"),
            video: VideoSpec::new(
                FrameDuration::new(i64::from(plan.native_frame_count())).unwrap(),
                plan.native_frame_rate(),
                dimensions.width(),
                dimensions.height(),
            )
            .unwrap(),
            provider: binding.provider.clone(),
        };
        let report = synthetic_worker(&binding, conditioning.context(), &declaration);
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(directory.path().join("outputs/provenance.json"), &bytes).unwrap();
        declaration.provenance = artifact("outputs/provenance.json", &bytes);
        Self {
            directory,
            request,
            conditioning,
            selected,
            declaration: CandidateDeclaration::NativeExtensionV3(declaration),
            context_bytes,
        }
    }

    fn worker(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.directory.path().join("outputs/provenance.json")).unwrap(),
        )
        .unwrap()
    }

    fn replace_worker(&mut self, bytes: &[u8]) {
        fs::write(self.directory.path().join("outputs/provenance.json"), bytes).unwrap();
        let CandidateDeclaration::NativeExtensionV3(declaration) = &mut self.declaration else {
            unreachable!()
        };
        declaration.provenance = artifact("outputs/provenance.json", bytes);
    }

    fn qualify(
        self,
        codec: &Path,
        tracker: &Path,
        limits: QualificationLimits,
        cancelled: &AtomicBool,
    ) -> Result<QualifiedExtensionBundle, QualificationError> {
        let workspace = ArtifactWorkspace::open(self.directory.path()).unwrap();
        qualify_extension(
            codec,
            tracker,
            &workspace,
            ExtensionQualification {
                request: &self.request,
                declaration: &self.declaration,
                selected_provider: &self.selected,
                conditioning: self.conditioning,
            },
            limits,
            cancelled,
        )
    }
}

fn artifact(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

fn object(bytes: &[u8]) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

fn synthetic_worker(
    binding: &ExtensionGenerationBinding,
    context: &deadpan_models::ExtensionContext,
    declaration: &NativeCandidateManifest,
) -> Value {
    let plan = &binding.plan;
    let interval = plan.sampling_map().generated_interval();
    json!({
        "schema_version": 3, "operation": "extension", "direction": plan.direction(),
        "request_binding": binding, "context": context,
        "generated_interval": [interval.start, interval.end],
        "pack_id": binding.provider.pack_id, "pack_version": binding.provider.pack_version,
        "runtime_id": binding.provider.runtime_id, "runtime_version": binding.provider.runtime_version,
        "model_manifest_sha256": "a".repeat(64), "seed": binding.provider.seed,
        "native_sha256": declaration.native.sha256(), "native_bytes": declaration.native.byte_length(),
        "timing": {
            "requested_duration": plan.requested_duration(),
            "generated_duration": plan.generated_duration(),
            "native_movie_duration": plan.native_movie_duration(),
            "context_duration": plan.context_duration(), "context_anchor_span": plan.context_anchor_span(),
            "speed_conversion": plan.speed(), "retime_deviation": plan.retime_deviation(),
        },
        "runtime_commit": "a".repeat(40), "pack_revision": "b".repeat(40), "gemma_revision": "c".repeat(40),
        "adapter_sources_sha256": {"fixture.py": "d".repeat(64)},
        "loaded_ltx_sources_sha256": {"synthetic.py": "e".repeat(64)},
        "verified_assets": [{"repository":"synthetic-fixture", "path":"fixture.bin", "size":1, "sha256":"f".repeat(64)}],
        "prompt_version":"synthetic-admission-test-1", "prompt":"Synthetic fixture, no inference performed.",
        "configuration":{"synthetic":true},
        "model_color_interpretation":"full-range SDR sRGB RGB, BT.709 primaries",
        "temporal_interpolation":"frame centers clamped to generated interval; encoded sRGB RGB8",
        "conditioning_preprocessing":"retained host PNG fixture; no model was loaded",
        "test_scope":"Synthetic worker metadata; actual converter and Vision processes."
    })
}

fn limits() -> QualificationLimits {
    QualificationLimits {
        media: ConversionLimits {
            max_input_bytes: 16 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            max_scratch_bytes: 32 * 1024 * 1024,
            timeout_ms: 60_000,
        },
        maximum_worker_provenance_bytes: 4 * 1024 * 1024,
        maximum_host_provenance_bytes: 32 * 1024 * 1024,
    }
}

fn codec() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker"))
}

fn tracker() -> PathBuf {
    let path = codec().with_file_name("deadpan-track");
    assert!(
        path.is_file(),
        "build deadpan-track beside the media worker: {}",
        path.display()
    );
    path
}

fn fixture(direction: ExtensionDirection, output: u32) -> Fixture {
    Fixture::new(
        direction,
        output,
        [0; GENERATED as usize],
        [255; CONTEXT as usize],
        true,
    )
}

fn error(result: Result<QualifiedExtensionBundle, QualificationError>) -> QualificationError {
    match result {
        Ok(_) => panic!("candidate unexpectedly qualified"),
        Err(error) => error,
    }
}

fn reopen(
    bytes: &[u8],
    expected: &GeneratedObjectRef,
    binding: &ExtensionGenerationBinding,
    context: &[u8],
) -> Result<ValidatedExtensionEvidence, QualificationError> {
    StoredExtensionProvenance::from_bytes(bytes, expected)?.validate_for(binding, context)
}

#[test]
fn extension_qualification_retains_all_proofs_after_worker_inputs_and_outputs_disappear() {
    for direction in DIRECTIONS {
        for output in [1, 8] {
            let prepared = Prepared::from_fixture(fixture(direction, output));
            let context_bytes = prepared.context_bytes.clone();
            let worker_bytes =
                fs::read(prepared.directory.path().join("outputs/provenance.json")).unwrap();
            let binding = ExtensionGenerationBinding::from_request(&prepared.request).unwrap();
            // Mutation happens after capture. Neither joins nor geometry may
            // silently recapture these worker-writable input paths.
            fs::remove_dir_all(prepared.directory.path().join("inputs")).unwrap();
            let bundle = prepared
                .qualify(codec(), &tracker(), limits(), &AtomicBool::new(false))
                .unwrap();
            // Prepared's TempDir and all worker output paths are now gone.
            assert_eq!(bundle.binding(), &binding);
            assert_eq!(bundle.native().report().video.frames, CONTEXT + GENERATED);
            assert_eq!(bundle.sampled().report().video.frames, output);
            let (mut native, mut sampled, mut provenance, conditioning) = bundle.into_parts();
            for media in [&mut native, &mut sampled] {
                assert_eq!(media.stream_position().unwrap(), 0);
                let mut signature = [0; 4];
                media.read_exact(&mut signature).unwrap();
                assert_eq!(signature, [0x1a, 0x45, 0xdf, 0xa3]);
            }
            let provenance_object = provenance.object().clone();
            let mut bytes = Vec::new();
            provenance.read_to_end(&mut bytes).unwrap();
            let evidence = reopen(&bytes, &provenance_object, &binding, &context_bytes).unwrap();
            assert_eq!(evidence.native_object(), native.object());
            assert_eq!(evidence.sampled_object(), sampled.object());
            assert_eq!(evidence.conditioning(), conditioning.receipt());
            assert_eq!(evidence.conditioning().context().len(), CONTEXT as usize);
            assert!(evidence.conditioning().opposite().is_some());
            assert_eq!(
                evidence.pixels().motion().transitions().len(),
                GENERATED as usize - 1
            );
            assert!(evidence.geometry().geometry().geometry.rejection.is_none());
            assert!(evidence.geometry().geometry().mouth.rejection.is_none());
            assert_eq!(
                evidence.geometry().region_unavailable_reason(),
                Some("no selected region target")
            );
            let wire: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(wire["validation_profile"], "deadpan-ffv1-extension-1");
            for field in [
                "pixels",
                "geometry",
                "conditioning",
                "native_validation",
                "sampled_validation",
            ] {
                assert!(wire[field].is_object(), "{field}");
            }
            assert_eq!(
                wire["geometry"]["observations"]["landmarks"]["frames"]
                    .as_array()
                    .unwrap()
                    .len(),
                GENERATED as usize
            );
            assert_eq!(
                wire["worker_provenance_utf8"].as_str().unwrap().as_bytes(),
                worker_bytes
            );
            let (manifest, context, opposite, signatures) = conditioning.into_parts();
            let mut inputs = vec![manifest, signatures];
            inputs.extend(context);
            inputs.extend(opposite);
            for mut input in inputs {
                assert_eq!(input.stream_position().unwrap(), 0);
            }
        }
    }
}

#[test]
fn stored_extension_rejects_missing_reports_forged_metrics_bindings_and_manifest_bytes() {
    let prepared = Prepared::from_fixture(fixture(ExtensionDirection::FromRight, 1));
    let context = prepared.context_bytes.clone();
    let binding = ExtensionGenerationBinding::from_request(&prepared.request).unwrap();
    let bundle = prepared
        .qualify(codec(), &tracker(), limits(), &AtomicBool::new(false))
        .unwrap();
    let (_, _, mut provenance, _) = bundle.into_parts();
    let original_object = provenance.object().clone();
    let mut bytes = Vec::new();
    provenance.read_to_end(&mut bytes).unwrap();
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    for field in [
        "pixels",
        "geometry",
        "conditioning",
        "native_validation",
        "sampled_validation",
        "worker_provenance_utf8",
    ] {
        for omit in [true, false] {
            let mut changed = wire.clone();
            if omit {
                changed.as_object_mut().unwrap().remove(field);
            } else {
                changed[field] = Value::Null;
            }
            let bytes = serde_json::to_vec(&changed).unwrap();
            assert!(
                reopen(&bytes, &object(&bytes), &binding, &context).is_err(),
                "{field}, omit={omit}"
            );
        }
    }
    for (pointer, replacement) in [
        ("/schema_version", json!(2)),
        ("/validation_profile", json!("deadpan-ffv1-bridge-8")),
        ("/native_validation/video/frames", json!(GENERATED)),
        ("/pixels/motion/thresholds/abrupt_luma_shift", json!(255.0)),
        ("/pixels/endpoints/entry/role", json!("conditioned")),
        ("/geometry/geometry/thresholds/center_residual", json!(1.0)),
        ("/geometry/observations/landmarks/frames/0/pts", json!(-1)),
        (
            "/geometry/region_unavailable_reason",
            json!("inference succeeded"),
        ),
        ("/selected_provider/selection/seed", json!(99)),
    ] {
        let mut changed = wire.clone();
        *changed.pointer_mut(pointer).expect(pointer) = replacement;
        let bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            reopen(&bytes, &object(&bytes), &binding, &context).is_err(),
            "{pointer}"
        );
    }
    let mut changed_binding = binding.clone();
    changed_binding.target.request_version = RequestVersion::new(2).unwrap();
    assert!(reopen(&bytes, &original_object, &changed_binding, &context).is_err());
    let mut changed_context = context.clone();
    changed_context.push(b' ');
    assert!(reopen(&bytes, &original_object, &binding, &changed_context).is_err());
    let duplicated = format!(
        "{{\"schema_version\":1,{}",
        std::str::from_utf8(&bytes[1..]).unwrap()
    )
    .into_bytes();
    assert!(reopen(&duplicated, &object(&duplicated), &binding, &context).is_err());
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert!(reopen(&changed, &original_object, &binding, &context).is_err());
}

#[test]
fn extension_provenance_contradictions_fail_before_native_snapshot_or_decode() {
    let base = fixture(ExtensionDirection::FromLeft, 1);
    for mutation in [
        "malformed",
        "duplicate",
        "schema",
        "direction",
        "interval",
        "timing",
        "seed",
        "missing_context",
    ] {
        let mut prepared = Prepared::from_native(&base.native);
        let mut report = prepared.worker();
        match mutation {
            "schema" => report["schema_version"] = json!(2),
            "direction" => report["direction"] = json!("from_right"),
            "interval" => report["generated_interval"] = json!([0, GENERATED]),
            "timing" => {
                report["timing"]["context_anchor_span"] =
                    report["timing"]["context_duration"].clone()
            }
            "seed" => report["seed"] = json!(99),
            "missing_context" => {
                report.as_object_mut().unwrap().remove("context");
            }
            _ => {}
        }
        let bytes = match mutation {
            "malformed" => b"{".to_vec(),
            "duplicate" => format!(
                "{{\"schema_version\":3,{}",
                &serde_json::to_string(&report).unwrap()[1..]
            )
            .into_bytes(),
            _ => serde_json::to_vec(&report).unwrap(),
        };
        prepared.replace_worker(&bytes);
        fs::remove_file(prepared.directory.path().join("outputs/native.mp4")).unwrap();
        let failure = error(prepared.qualify(
            Path::new("/missing/media"),
            Path::new("/missing/tracker"),
            limits(),
            &AtomicBool::new(false),
        ));
        assert!(
            matches!(
                failure,
                QualificationError::Json(_) | QualificationError::Provenance(_)
            ),
            "{mutation}: {failure}"
        );
    }
}

#[test]
fn extension_qualification_rejects_wrong_variant_provider_and_bounded_control() {
    let base = fixture(ExtensionDirection::FromLeft, 1);
    for mode in [
        "bridge_variant",
        "provider",
        "cancel",
        "worker_bytes",
        "native_bytes",
        "invalid_limits",
    ] {
        let mut prepared = Prepared::from_native(&base.native);
        let mut bound = limits();
        let cancelled = AtomicBool::new(mode == "cancel");
        match mode {
            "bridge_variant" => {
                let CandidateDeclaration::NativeExtensionV3(declaration) = prepared.declaration
                else {
                    unreachable!()
                };
                prepared.declaration = CandidateDeclaration::NativeBridgeV2(declaration);
            }
            "provider" => {
                let mut selection = prepared.selected.selection().clone();
                selection.seed += 1;
                prepared.selected =
                    SelectedExtensionProvider::new(selection, *prepared.selected.capability());
            }
            "worker_bytes" => bound.maximum_worker_provenance_bytes = 1,
            "native_bytes" => bound.media.max_input_bytes = 1,
            "invalid_limits" => bound.media.timeout_ms = 0,
            _ => {}
        }
        let failure = error(prepared.qualify(
            Path::new("/missing/media"),
            Path::new("/missing/tracker"),
            bound,
            &cancelled,
        ));
        assert!(
            match mode {
                "bridge_variant" | "provider" => matches!(failure, QualificationError::Request(_)),
                "cancel" => matches!(failure, QualificationError::Cancelled),
                "worker_bytes" | "native_bytes" =>
                    matches!(failure, QualificationError::Artifact(_)),
                "invalid_limits" => matches!(failure, QualificationError::Media(_)),
                _ => unreachable!(),
            },
            "{mode}: {failure}"
        );
    }
    let prepared = Prepared::from_native(&base.native);
    let mut bound = limits();
    bound.maximum_host_provenance_bytes = 1;
    let failure = error(prepared.qualify(codec(), &tracker(), bound, &AtomicBool::new(false)));
    assert!(
        matches!(failure, QualificationError::Provenance(_)),
        "{failure}"
    );
}

#[test]
fn extension_qualification_propagates_hidden_native_flash_before_landmark_launch() {
    for direction in DIRECTIONS {
        let prepared = Prepared::from_fixture(Fixture::new(
            direction,
            1,
            [0, 255, 0, 0, 0, 0, 0, 0],
            [255; CONTEXT as usize],
            true,
        ));
        let failure = error(prepared.qualify(
            codec(),
            Path::new("/missing/tracker"),
            limits(),
            &AtomicBool::new(false),
        ));
        assert!(
            matches!(failure, QualificationError::Quality(_)),
            "{failure}"
        );
        assert!(failure.to_string().contains("abrupt lighting"), "{failure}");
        assert!(
            failure
                .to_string()
                .contains("deadpan-extension-motion-lighting-1"),
            "{failure}"
        );
    }
}
