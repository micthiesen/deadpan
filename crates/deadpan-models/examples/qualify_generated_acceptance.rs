//! Developer qualification of explicit durable acceptance with actual media.
//! No downloads, model execution, UI audition or source-context resolution occurs.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod supported {
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use deadpan_core::{
        AssetId, BeatNode, ColorPolicy, Command, CommandRequest, HoldAudio, HoldRecipe, HoldVideo,
        NodeId, PresentationBasis, ProjectDocument, RevisionId, Subtree,
    };
    use deadpan_jobs::artifact::ArtifactWorkspace;
    use deadpan_jobs::{
        CancellationToken, HostMessage, NativeCandidateManifest, ProtocolVersion, WorkerMessage,
        WorkerStage, WorkspaceArtifact, WorkspaceRef,
    };
    use deadpan_models::{
        BridgeQualification, ConditioningLimits, GenerationBinding, QualificationLimits,
        SelectedBridgeProvider, capture_bridge_conditioning, qualify_bridge,
    };
    use deadpan_store::generated_media::GeneratedMediaLimits;
    use deadpan_store::generation::{
        ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
    };
    use deadpan_store::generation_acceptance::GenerationAcceptance;
    use deadpan_store::generation_attempts::{
        BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects,
        BundleValidationReceipt, ValidatorIdentity,
    };
    use deadpan_store::{AccessMode, ProjectStore};
    use serde::Deserialize;
    use serde_json::json;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        codec: PathBuf,
        workspace: PathBuf,
        request: HostMessage,
        candidate: NativeCandidateManifest,
        selected_provider: SelectedBridgeProvider,
        conditioning: ConditioningConfiguration,
        output_directory: PathBuf,
        limits: QualificationLimits,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ConditioningConfiguration {
        workspace: PathBuf,
        input_scope: WorkspaceRef,
        manifest: WorkspaceArtifact,
        limits: ConditioningLimits,
    }

    // This probe changes only the visual provider and navigates that edit. Its
    // captured context therefore remains fixed. A real host must resolve source
    // dependencies against each proposed revision instead of echoing old hashes.
    fn unchanged_context(
        store: &ProjectStore,
        from: &RevisionId,
        to: &RevisionId,
    ) -> Result<RelevancePlan> {
        Ok(RelevancePlan {
            from_revision: from.clone(),
            to_revision: to.clone(),
            observations: store
                .current_generation_requests()?
                .into_iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id,
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                    binding: request.binding,
                })
                .collect(),
        })
    }

    fn create_document(binding: &GenerationBinding) -> Result<ProjectDocument> {
        let root = NodeId::new("acceptance-probe-root")?;
        let initial = ProjectDocument::new(
            binding.project_id.clone(),
            RevisionId::new("acceptance-probe-initial")?,
            PresentationBasis {
                width: binding.constraints.video.width(),
                height: binding.constraints.video.height(),
                frame_rate: binding.constraints.video.frame_rate(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )?;
        let edit = deadpan_core::apply(
            &initial,
            &CommandRequest {
                project_id: binding.project_id.clone(),
                expected_revision: initial.revision_id().clone(),
                new_revision: binding.revision_id.clone(),
                command: Command::Insert {
                    parent: root,
                    index: 0,
                    subtree: Subtree {
                        root: binding.target.hold_id.clone(),
                        nodes: BTreeMap::from([(
                            binding.target.hold_id.clone(),
                            BeatNode::hold(
                                "Development generation probe",
                                HoldRecipe {
                                    duration: binding.constraints.video.frames(),
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                },
                            ),
                        )]),
                        overrides: BTreeMap::new(),
                    },
                },
            },
        )?;
        Ok(edit.forward.apply(&initial)?)
    }

    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() != 1 {
            return Err("expected one qualification configuration path".into());
        }
        let mut bytes = Vec::new();
        File::open(&args[0])?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            return Err("configuration exceeds 1 MiB".into());
        }
        let config: Configuration = serde_json::from_slice(&bytes)?;
        if [
            &config.codec,
            &config.workspace,
            &config.conditioning.workspace,
            &config.output_directory,
        ]
        .into_iter()
        .any(|path| !path.is_absolute())
        {
            return Err("host paths must be absolute".into());
        }
        // Captured by the pre-launch host, never recaptured from worker inputs.
        let inputs = ArtifactWorkspace::open(&config.conditioning.workspace)?;
        let conditioning = capture_bridge_conditioning(
            &inputs,
            &config.request,
            &config.conditioning.manifest,
            &config.conditioning.input_scope,
            config.conditioning.limits,
            &AtomicBool::new(false),
        )?;
        let workspace = ArtifactWorkspace::open(&config.workspace)?;
        let started = std::time::Instant::now();
        let bundle = qualify_bridge(
            &config.codec,
            &workspace,
            BridgeQualification {
                request: &config.request,
                declaration: &config.candidate,
                selected_provider: &config.selected_provider,
                conditioning,
            },
            config.limits,
            &AtomicBool::new(false),
        )?;
        let binding = bundle.binding().clone();
        let qualification = json!({"native":bundle.native().report(), "sampled":bundle.sampled().report(),
            "native_span":bundle.native_span(), "sampled_span":bundle.sampled_span(),
            "conditioning":bundle.conditioning().receipt()});
        let inputs = bundle.conditioning().receipt();
        let admission = BundleAdmissionEvidence::new(
            bundle.native_span(),
            bundle.sampled_span(),
            BundleInputObjects::new(
                binding.input.sha256.clone(),
                inputs.manifest().object().clone(),
                inputs.left().object().clone(),
                inputs.right().object().clone(),
            )?,
        )?;
        let receipt = BundleValidationReceipt::new(
            &config.candidate,
            bundle.native().object().clone(),
            bundle.sampled().object().clone(),
            bundle.provenance().object().clone(),
            binding.constraints.video.clone(),
            binding.plan.clone(),
            ValidatorIdentity::new("native-ffv1", "bridge-3")?,
        )?
        .with_admission(admission)?;
        let document = create_document(&binding)?;
        fs::create_dir(&config.output_directory)?;
        let package = config.output_directory.join("accepted.deadpan");
        let mut store = ProjectStore::create(&package, &document)?;
        let request = store.record_bridge_generation_request(
            GenerationRequestInput {
                request_id: binding.identity.request_id.clone(),
                expected_revision: document.revision_id().clone(),
                hold_id: binding.target.hold_id.clone(),
                context_sha256: binding.input.sha256.clone(),
                constraints: binding.constraints.clone(),
                provider: binding.provider.clone(),
            },
            binding.plan.clone(),
        )?;
        if request.binding.request_version != binding.target.request_version {
            return Err("probe requires the first request version in a new project".into());
        }
        store.begin_generation_attempt(BeginGenerationAttempt {
            identity: binding.identity.clone(),
            cancellation_token: CancellationToken::new("acceptance-probe-token")?,
        })?;
        for stage in [WorkerStage::Preflight, WorkerStage::Inference] {
            store.record_generation_worker_message(&WorkerMessage::Stage {
                protocol: ProtocolVersion::V2,
                identity: binding.identity.clone(),
                stage,
            })?;
        }
        store.record_generation_worker_message(&WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: binding.identity.clone(),
            candidate: config.candidate.clone(),
        })?;
        let budget = GeneratedMediaLimits::new(
            config
                .limits
                .media
                .max_output_bytes
                .max(config.limits.maximum_host_provenance_bytes)
                .max(config.conditioning.limits.maximum_frame_bytes)
                .max(config.conditioning.limits.maximum_manifest_bytes),
        )?;
        let (mut native, mut sampled, mut provenance, conditioning) = bundle.into_parts();
        for (reader, object) in [
            (&mut native, receipt.native_object()),
            (&mut sampled, receipt.sampled_object()),
        ] {
            store.promote_generated_object(reader, object, budget)?;
        }
        store.promote_generated_object(&mut provenance, receipt.provenance_object(), budget)?;
        let (manifest, left, right) = conditioning.into_parts();
        for mut input in [manifest, left, right] {
            let object = input.object().clone();
            store.promote_generated_object(&mut input, &object, budget)?;
        }
        store.record_generation_bundle_ready(
            &binding.identity,
            &config.candidate,
            receipt.clone(),
            budget,
        )?;
        if store.snapshot()? != document {
            return Err("Ready changed authored state".into());
        }
        let accept = GenerationAcceptance {
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("accepted")?,
            identity: binding.identity.clone(),
            expected_receipt: receipt.clone(),
            native_asset: AssetId::new("accepted-native")?,
            sampled_asset: AssetId::new("accepted-sampled")?,
        };
        let preview = store.preview_generation_acceptance(&accept, budget)?;
        let expected = preview.forward.apply(&document)?;
        let relevance = unchanged_context(&store, document.revision_id(), &accept.new_revision)?;
        store.accept_generation_bundle(&accept, &relevance, budget)?;
        if store.snapshot()? != expected {
            return Err("acceptance differs from preview".into());
        }
        drop(store);
        let relocated = config.output_directory.join("relocated-accepted.deadpan");
        fs::rename(&package, &relocated)?;
        let mut store = ProjectStore::open(&relocated, AccessMode::ReadWrite)?;
        if store.snapshot()? != expected {
            return Err("accepted project changed after reopen".into());
        }
        let undo = RevisionId::new("undo-accept")?;
        let relevance = unchanged_context(&store, &accept.new_revision, &undo)?;
        store.undo_reconciled(&accept.new_revision, undo.clone(), &relevance)?;
        if store.snapshot()?.assets() != document.assets() {
            return Err("undo retained new authored assets".into());
        }
        let redo = RevisionId::new("redo-accept")?;
        let relevance = unchanged_context(&store, &undo, &redo)?;
        store.redo_reconciled(&undo, redo.clone(), &relevance)?;
        if store.snapshot()?.assets() != expected.assets() {
            return Err("redo changed accepted assets".into());
        }
        let revert = RevisionId::new("revert-to-fallback")?;
        let relevance = unchanged_context(&store, &redo, &revert)?;
        store.commit_reconciled(
            &CommandRequest {
                project_id: binding.project_id.clone(),
                expected_revision: redo,
                new_revision: revert,
                command: Command::RevertGeneratedHold {
                    node: binding.target.hold_id.clone(),
                },
            },
            &relevance,
        )?;
        let input_objects = receipt
            .admission()
            .ok_or("missing admission evidence")?
            .inputs();
        let mut readback = Vec::new();
        for (name, object) in [
            ("native.mkv", receipt.native_object()),
            ("sampled.mkv", receipt.sampled_object()),
            ("provenance.json", receipt.provenance_object()),
            ("context.json", input_objects.manifest()),
            ("left.png", input_objects.left()),
            ("right.png", input_objects.right()),
        ] {
            let mut snapshot = store.snapshot_generated_object(object, budget)?;
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(config.output_directory.join(name))?;
            if std::io::copy(&mut snapshot, &mut output)? != object.byte_length() {
                return Err("readback length mismatch".into());
            }
            output.flush()?;
            output.sync_all()?;
            readback.push(json!({"name":name,"object":object}));
        }
        let report = json!({"scope":"actual canonical media and retained inputs through explicit store acceptance, relocation, undo/redo and fallback reversion; no source-clock/color/audition/UI qualification",
            "package":relocated,"qualification":qualification,"receipt":receipt,"readback":readback,
            "final_revision":store.snapshot()?.revision_id(),"elapsed_seconds":started.elapsed().as_secs_f64()});
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(config.output_directory.join("acceptance-report.json"))?;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    supported::run()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("acceptance qualification requires a supported Unix host");
    std::process::exit(1);
}
