//! Production context capture, qualification, store acceptance and cold reads.
//! The worker metadata is synthetic; no model inference or model quality is claimed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::generation::conditioning::prepare_extension_scoped_with_options;
use deadpan_cli::generation::joins::{JoinObservation, measure_scoped_request_joins};
use deadpan_cli::picture::{PreparedPicture, ProjectPictureSession, open_generated_picture};
use deadpan_core::{
    AssetId, BeatNode, ColorPolicy, Command, CommandRequest, ExtensionDirection, FrameDuration,
    FrameRate, GeneratedArtifact, GeneratedObjectRef, GeneratedSamplingMap, HoldAudio, HoldRecipe,
    HoldVideo, NodeId, NodeKind, PresentationBasis, ProjectDocument, ProjectFrame, ProjectId,
    RevisionId, ScopedNodeTarget, SourceFrameId, Subtree,
};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AttemptId, CancellationToken, ContextArtifact, GenerationOptions, GenerationPlan, HoldTarget,
    HostMessage, JobState, MessageIdentity, ProtocolVersion, ProviderPackId, ProviderPackVersion,
    ProviderSelection, RequestId, RuntimeId, RuntimeVersion, WorkerMessage, WorkerStage,
    WorkspaceArtifact, WorkspaceRef,
};
use deadpan_models::{
    ConditioningLimits, ExtensionContext, ExtensionOppositeSeam, QualifiedExtensionBundle,
    capture_extension_conditioning,
};
use deadpan_store::generated_media::GeneratedMediaLimits;
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
};
use deadpan_store::generation_acceptance::GenerationAcceptance;
use deadpan_store::generation_attempts::{
    BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects, BundleValidationReceipt,
    ValidatorIdentity,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::fixture::{CONTEXT, GENERATED, HEIGHT, WIDTH, encode_rgb};
use super::qualification::{Prepared, codec, limits, tracker};

const LEAD: i64 = 24;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn object_limits() -> GeneratedMediaLimits {
    GeneratedMediaLimits::new(32 * 1024 * 1024).unwrap()
}

fn hold_start(direction: ExtensionDirection, output: u32) -> i64 {
    if direction == ExtensionDirection::FromRight && output == 1 {
        0
    } else {
        LEAD
    }
}

fn document(direction: ExtensionDirection, output: u32) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("saved-extension").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: WIDTH,
            height: HEIGHT,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    for (index, (name, count)) in [("left", LEAD), ("hold", i64::from(output)), ("right", LEAD)]
        .into_iter()
        .filter(|(name, _)| {
            output != 1
                || match direction {
                    ExtensionDirection::FromLeft => *name != "right",
                    ExtensionDirection::FromRight => *name != "left",
                }
        })
        .enumerate()
    {
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(&format!("setup-{index}")),
            command: Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(
                        node(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: frames(count),
                                picture_context: None,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        };
        document = deadpan_core::apply(&document, &request)
            .unwrap()
            .forward
            .apply(&document)
            .unwrap();
    }
    document
}

struct Saved {
    _directory: tempfile::TempDir,
    package: PathBuf,
    store: ProjectStore,
    identity: MessageIdentity,
    receipt: BundleValidationReceipt,
    before: ProjectDocument,
}

impl Saved {
    fn ready(direction: ExtensionDirection, output: u32) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("saved.deadpan");
        let mut store = ProjectStore::create(&package, &document(direction, output)).unwrap();
        let before = store.snapshot().unwrap();
        let target = ScopedNodeTarget {
            node: node("hold"),
            repeats: vec![],
        };
        let inputs = prepare_extension_scoped_with_options(
            &package,
            before.revision_id(),
            &target,
            direction,
            &GenerationOptions::default(),
            &active(),
        )
        .unwrap();
        let context: ExtensionContext = serde_json::from_slice(&inputs.manifest).unwrap();
        let stored = store
            .record_scoped_generation_request(
                GenerationRequestInput {
                    request_id: RequestId::new("saved-request").unwrap(),
                    expected_revision: before.revision_id().clone(),
                    hold_id: target.node.clone(),
                    context_sha256: inputs.manifest_sha256.clone(),
                    constraints: inputs.constraints.clone(),
                    provider: ProviderSelection {
                        pack_id: ProviderPackId::new("synthetic-no-model").unwrap(),
                        pack_version: ProviderPackVersion::new("1").unwrap(),
                        runtime_id: RuntimeId::new("synthetic-no-runtime").unwrap(),
                        runtime_version: RuntimeVersion::new("1").unwrap(),
                        seed: 1,
                    },
                },
                target,
                GenerationPlan::Extension(inputs.plan.clone()),
            )
            .unwrap();
        assert_eq!(
            stored.input_binding.as_ref(),
            Some(context.continuity().binding())
        );
        let identity = MessageIdentity::new(
            stored.request_id.clone(),
            AttemptId::new("attempt").unwrap(),
        );
        let cancellation = CancellationToken::new("cancel-saved").unwrap();
        store
            .begin_generation_attempt(BeginGenerationAttempt {
                identity: identity.clone(),
                cancellation_token: cancellation.clone(),
            })
            .unwrap();

        // Only this temporary directory is worker-owned. Qualification drops it;
        // the project and all later reads must depend on retained objects alone.
        let worker = tempfile::tempdir().unwrap();
        let worker_path = worker.path().to_owned();
        fs::create_dir(worker.path().join("inputs")).unwrap();
        fs::create_dir(worker.path().join("outputs")).unwrap();
        let manifest_ref = WorkspaceRef::new("inputs/context.json").unwrap();
        fs::write(worker.path().join(manifest_ref.as_str()), &inputs.manifest).unwrap();
        for (picture, bytes) in context.context().iter().zip(&inputs.context_pngs) {
            fs::write(
                worker.path().join(picture.frame.reference().as_str()),
                bytes,
            )
            .unwrap();
        }
        match (context.opposite(), &inputs.opposite_png) {
            (ExtensionOppositeSeam::PresentUnconditioned { frame, .. }, Some(bytes)) => {
                assert_eq!(output, 8);
                fs::write(worker.path().join(frame.reference().as_str()), bytes).unwrap();
            }
            (ExtensionOppositeSeam::Absent, None) => assert_eq!(output, 1),
            _ => panic!("manifest and retained opposite picture disagree"),
        }
        fs::write(
            worker
                .path()
                .join(context.continuity().signatures().reference().as_str()),
            &inputs.continuity_signatures,
        )
        .unwrap();
        let manifest = WorkspaceArtifact::new(
            manifest_ref.clone(),
            inputs.manifest_sha256.clone(),
            inputs.manifest.len() as u64,
        )
        .unwrap();
        let request = HostMessage::GenerateExtension {
            protocol: ProtocolVersion::V3,
            identity: identity.clone(),
            cancellation_token: cancellation,
            project_id: before.project_id().clone(),
            revision_id: before.revision_id().clone(),
            target: HoldTarget {
                hold_id: node("hold"),
                request_version: stored.binding.request_version,
            },
            input: ContextArtifact {
                manifest: manifest_ref,
                sha256: inputs.manifest_sha256,
            },
            output_workspace: WorkspaceRef::new("outputs").unwrap(),
            constraints: inputs.constraints,
            provider: Box::new(stored.provider),
            plan: Box::new(inputs.plan.clone()),
        };
        let conditioning = capture_extension_conditioning(
            &ArtifactWorkspace::open(worker.path()).unwrap(),
            &request,
            &manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000).unwrap(),
            &active(),
        )
        .unwrap();
        let generated = [0, 2, 4, 6, 8, 10, 12, 14];
        let context_values = [255; CONTEXT as usize];
        let values = match direction {
            ExtensionDirection::FromLeft => {
                [context_values.as_slice(), generated.as_slice()].concat()
            }
            ExtensionDirection::FromRight => {
                [generated.as_slice(), context_values.as_slice()].concat()
            }
        };
        let raster = inputs.plan.native_dimensions();
        let raw: Vec<_> = values
            .into_iter()
            .flat_map(|value| {
                std::iter::repeat_n(value, (raster.width() * raster.height() * 3) as usize)
            })
            .collect();
        let native = encode_rgb(
            worker.path(),
            &raw,
            raster.width(),
            raster.height(),
            CONTEXT + GENERATED,
        );
        let qualified = Prepared::new(worker, request, conditioning, &native)
            .qualify(codec(), &tracker(), limits(), &active())
            .unwrap();
        assert!(
            !worker_path.exists(),
            "worker files are gone before publication"
        );
        for stage in [WorkerStage::Preflight, WorkerStage::Inference] {
            store
                .record_generation_worker_message(&WorkerMessage::Stage {
                    protocol: ProtocolVersion::V3,
                    identity: identity.clone(),
                    stage,
                })
                .unwrap();
        }
        let declaration = qualified.declaration().clone();
        store
            .record_generation_worker_message(&WorkerMessage::CompletedExtension {
                protocol: ProtocolVersion::V3,
                identity: identity.clone(),
                candidate: declaration.clone(),
            })
            .unwrap();
        assert_eq!(
            store
                .generation_attempt(&identity)
                .unwrap()
                .unwrap()
                .checkpoint
                .state,
            JobState::Validating
        );
        let receipt = receipt(&qualified);
        let (mut native, mut sampled, mut provenance, conditioning) = qualified.into_parts();
        store
            .promote_generated_object(&mut native, receipt.native_object(), object_limits())
            .unwrap();
        store
            .promote_generated_object(&mut sampled, receipt.sampled_object(), object_limits())
            .unwrap();
        store
            .promote_generated_object(
                &mut provenance,
                receipt.provenance_object(),
                object_limits(),
            )
            .unwrap();
        let (manifest, context, opposite, mut signatures) = conditioning.into_parts();
        for mut input in std::iter::once(manifest).chain(context).chain(opposite) {
            let object = input.object().clone();
            store
                .promote_generated_object(&mut input, &object, object_limits())
                .unwrap();
        }
        // A completed qualifier still cannot publish Ready without signatures.
        assert!(
            store
                .record_generation_bundle_ready(
                    &identity,
                    &declaration,
                    receipt.clone(),
                    object_limits()
                )
                .is_err()
        );
        assert_eq!(
            store
                .generation_attempt(&identity)
                .unwrap()
                .unwrap()
                .checkpoint
                .state,
            JobState::Validating
        );
        let signature_object = signatures.object().clone();
        store
            .promote_generated_object(&mut signatures, &signature_object, object_limits())
            .unwrap();
        store
            .record_generation_bundle_ready(
                &identity,
                &declaration,
                receipt.clone(),
                object_limits(),
            )
            .unwrap();
        assert_eq!(
            store
                .generation_attempt(&identity)
                .unwrap()
                .unwrap()
                .checkpoint
                .state,
            JobState::Ready
        );
        assert_eq!(
            store.snapshot().unwrap(),
            before,
            "Ready cannot change the picture provider"
        );
        assert_eq!(
            store
                .selected_generation_bundle(&identity.request_id)
                .unwrap()
                .unwrap()
                .receipt,
            receipt
        );
        let joins = measure_scoped_request_joins(
            &package,
            &store.generated_read_handle(),
            before.revision_id(),
            &ScopedNodeTarget {
                node: node("hold"),
                repeats: vec![],
            },
            &receipt,
            &active(),
        )
        .unwrap();
        for (observed, conditioned, ordinal) in [
            (joins.entry, direction == ExtensionDirection::FromLeft, 0),
            (
                joins.exit,
                direction == ExtensionDirection::FromRight,
                output - 1,
            ),
        ] {
            if !conditioned && output == 1 {
                assert_eq!(observed, JoinObservation::Absent);
                assert!(observed.measure().is_none());
                continue;
            }
            let measured = match observed {
                JoinObservation::Conditioned(measured) if conditioned => measured,
                JoinObservation::Unconditioned(measured) if !conditioned => measured,
                _ => panic!("available join must retain its conditioning role"),
            };
            let expected = expected_value(output, ordinal);
            assert_eq!(measured.mean_abs_diff, f64::from(expected));
            assert_eq!(measured.max_abs_diff, expected);
        }
        Self {
            _directory: directory,
            package,
            store,
            identity,
            receipt,
            before,
        }
    }

    fn accept(&mut self) -> GeneratedArtifact {
        let input = GenerationAcceptance {
            expected_revision: self.before.revision_id().clone(),
            new_revision: revision("accepted"),
            identity: self.identity.clone(),
            expected_receipt: self.receipt.clone(),
            sampled_asset: AssetId::new("sampled").unwrap(),
            native_asset: AssetId::new("native").unwrap(),
        };
        let relevance = relevance(&self.store, &input.new_revision);
        self.store
            .accept_generation_bundle(&input, &relevance, object_limits())
            .unwrap();
        self.store.validate_full().unwrap();
        let document = self.store.snapshot().unwrap();
        let NodeKind::Hold { recipe } = &document.nodes()[&node("hold")].kind else {
            panic!("Hold")
        };
        assert_eq!(recipe.audio, HoldAudio::Silence);
        let HoldVideo::Generated { accepted } = &recipe.video else {
            panic!("explicit accepted provider")
        };
        let artifact = accepted.artifact.clone();
        assert!(matches!(
            artifact.sampling,
            GeneratedSamplingMap::Extension(_)
        ));
        assert_eq!(artifact.native_object, *self.receipt.native_object());
        assert_eq!(artifact.sampled_object, *self.receipt.sampled_object());
        assert!(
            self.store
                .accepted_generation_origin(&artifact)
                .unwrap()
                .is_some()
        );
        artifact
    }
}

fn receipt(bundle: &QualifiedExtensionBundle) -> BundleValidationReceipt {
    let binding = bundle.binding();
    let inputs = bundle.conditioning().receipt();
    BundleValidationReceipt::new_extension(
        bundle.declaration(),
        bundle.native().object().clone(),
        bundle.sampled().object().clone(),
        bundle.provenance().object().clone(),
        binding.constraints.video.clone(),
        binding.plan.clone(),
        ValidatorIdentity::new("native-ffv1", "extension-1").unwrap(),
        BundleAdmissionEvidence::new(
            bundle.native_span(),
            bundle.sampled_span(),
            BundleInputObjects::new_extension(
                binding.input.sha256.clone(),
                inputs.manifest().object().clone(),
                inputs
                    .context()
                    .iter()
                    .map(|input| input.object().clone())
                    .collect(),
                inputs.opposite().map(|input| input.object().clone()),
                inputs.signatures().object().clone(),
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn relevance(store: &ProjectStore, next: &RevisionId) -> RelevancePlan {
    RelevancePlan {
        from_revision: store.snapshot().unwrap().revision_id().clone(),
        to_revision: next.clone(),
        observations: store
            .current_generation_requests()
            .unwrap()
            .into_iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id,
                target: request.target,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            })
            .collect(),
    }
}

fn expected_value(output: u32, ordinal: u32) -> u8 {
    match output {
        1 => 7, // E=8 center 3.5 interpolates values 6 and 8.
        8 => (ordinal * 2) as u8,
        _ => panic!("fixture output count"),
    }
}

fn assert_pixels(bytes: &[u8], expected: u8) {
    assert!(!bytes.is_empty());
    assert!(
        bytes
            .chunks_exact(4)
            .all(|pixel| pixel == [expected, expected, expected, 255])
    );
}

fn assert_cold_pictures(
    package: &Path,
    artifact: &GeneratedArtifact,
    direction: ExtensionDirection,
    output: u32,
) {
    let store = ProjectStore::open(package, AccessMode::ReadOnly).unwrap();
    let document = store.snapshot().unwrap();
    let mut decoder = open_generated_picture(
        &store.generated_read_handle(),
        &document,
        artifact,
        &active(),
    )
    .unwrap();
    assert_eq!(decoder.index().index().frames().len(), output as usize);
    for ordinal in (0..output).rev() {
        let frame = decoder
            .frame(
                SourceFrameId(u64::from(ordinal)),
                Duration::from_secs(15),
                &active(),
            )
            .unwrap();
        assert_eq!(frame.metadata.pts, i64::from((ordinal * 1000 + 12) / 24));
        assert_pixels(&frame.rgba, expected_value(output, ordinal));
    }
    drop(decoder);
    // Shared committed picture preparation exercises authored-to-sampled mapping
    // through the same cold admission used by native preview and render inputs.
    let mut pictures =
        ProjectPictureSession::open_revision(package, document.revision_id(), None, &active())
            .unwrap();
    let start = hold_start(direction, output);
    for ordinal in 0..output {
        let prepared = pictures
            .prepare(ProjectFrame(start + i64::from(ordinal)), &active())
            .unwrap();
        let PreparedPicture::Generated {
            artifact: actual,
            id,
            frame,
        } = prepared.picture
        else {
            panic!("generated picture")
        };
        assert_eq!(actual.as_ref(), artifact);
        assert_eq!(id, SourceFrameId(u64::from(ordinal)));
        assert_pixels(frame.bytes(), expected_value(output, ordinal));
    }
    for boundary in [start - 1, start + i64::from(output)] {
        let result = pictures.prepare(ProjectFrame(boundary), &active());
        if (0..document.duration().unwrap().frames()).contains(&boundary) {
            assert!(matches!(
                result.unwrap().picture,
                PreparedPicture::Background
            ));
        } else {
            assert!(
                result.is_err(),
                "definition edge has no extra output picture"
            );
        }
    }
}

fn stored_path(package: &Path, reference: &GeneratedObjectRef) -> PathBuf {
    package
        .join("Media/Generated")
        .join(format!("blake3-{}", reference.content().digest()))
}

#[test]
fn qualified_extensions_become_ready_then_explicitly_accepted_and_reopen_without_model_or_worker() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for output in [1, 8] {
            let mut saved = Saved::ready(direction, output);
            let artifact = saved.accept();
            assert_cold_pictures(&saved.package, &artifact, direction, output);
            let undo = revision("undo");
            saved
                .store
                .undo_reconciled(
                    &revision("accepted"),
                    undo.clone(),
                    &relevance(&saved.store, &undo),
                )
                .unwrap();
            let undone = saved.store.snapshot().unwrap();
            let NodeKind::Hold { recipe } = &undone.nodes()[&node("hold")].kind else {
                panic!("Hold")
            };
            assert_eq!(recipe.video, HoldVideo::Background);
            assert!(undone.assets().is_empty());
            let redo = revision("redo");
            saved
                .store
                .redo_reconciled(&undo, redo.clone(), &relevance(&saved.store, &redo))
                .unwrap();
            saved.store.validate_full().unwrap();
            assert_cold_pictures(&saved.package, &artifact, direction, output);

            let inputs = saved.receipt.admission().unwrap().inputs();
            for reference in [inputs.signatures().unwrap(), &inputs.context().unwrap()[0]] {
                let path = stored_path(&saved.package, reference);
                let missing = saved._directory.path().join("withheld-object");
                fs::rename(&path, &missing).unwrap();
                let document = saved.store.snapshot().unwrap();
                let result = open_generated_picture(
                    &saved.store.generated_read_handle(),
                    &document,
                    &artifact,
                    &active(),
                );
                assert!(
                    result.is_err(),
                    "cold admission requires every retained signatures/PNG object"
                );
                fs::rename(&missing, &path).unwrap();
            }
            assert_cold_pictures(&saved.package, &artifact, direction, output);
            saved.store.validate_full().unwrap();
        }
    }
}
