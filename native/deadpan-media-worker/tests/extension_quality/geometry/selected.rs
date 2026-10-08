//! A measured Original and saved target through production context capture.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::generation::conditioning::prepare_extension_scoped_with_options;
use deadpan_core::{
    AssetId, AttentionTarget, BeatNode, ColorPolicy, Command, CommandRequest, ExtensionDirection,
    FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis,
    ProjectDocument, ProjectId, RevisionId, ScopedNodeTarget, SourceSpan, SourceTimestamp, Subtree,
    TargetId, TargetRegion,
};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AttemptId, CancellationToken, ContextArtifact, GenerationOptions, GenerationTarget, HoldTarget,
    HostMessage, MessageIdentity, ProtocolVersion, RequestId, RequestVersion, WorkspaceArtifact,
    WorkspaceRef,
};
use deadpan_media::protocol::{
    ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest, ExtensionOperation,
    VideoContract,
};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_media::{CanonicalExtension, InputIdentity, canonicalize_extension};
use deadpan_models::{
    BoundaryPicture, ConditioningLimits, ExtensionContext, ExtensionOppositeSeam,
    RetainedExtensionConditioning, capture_extension_conditioning,
};
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use sha2::Digest;

use super::super::fixture::{CONTEXT, GENERATED, HEIGHT, WIDTH, encode_rgb};

pub(super) struct SelectedFixture {
    pub directory: tempfile::TempDir,
    pub request: HostMessage,
    pub conditioning: RetainedExtensionConditioning,
    pub media: CanonicalExtension,
    anchor: image::RgbImage,
}

impl SelectedFixture {
    pub fn new(direction: ExtensionDirection, available: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source_directory = directory.path().join("source");
        std::fs::create_dir(&source_directory).unwrap();
        let picture = subject_picture();
        let source_bytes = picture.repeat((CONTEXT + GENERATED) as usize);
        encode_rgb(
            &source_directory,
            &source_bytes,
            WIDTH,
            HEIGHT,
            CONTEXT + GENERATED,
        );
        let package = directory.path().join("selected.deadpan");
        let mut store = original_project(&package, &source_directory.join("native.mp4"));
        let document = store.snapshot().unwrap();
        let index = store
            .source_video_index(document.revision_id(), &asset())
            .unwrap();
        let anchor_ordinal = match direction {
            ExtensionDirection::FromLeft => index.frames().len() - 1,
            ExtensionDirection::FromRight => 0,
        };
        let stamp = |ordinal: usize| SourceTimestamp {
            ticks: index.frames()[ordinal].pts,
            time_base: index.time_base(),
        };
        edit(
            &mut store,
            "hold",
            Command::Insert {
                parent: node("root"),
                index: usize::from(direction == ExtensionDirection::FromLeft),
                subtree: Subtree {
                    root: node("hold"),
                    nodes: BTreeMap::from([(
                        node("hold"),
                        BeatNode::hold(
                            "Extension",
                            HoldRecipe {
                                duration: FrameDuration::new(3).unwrap(),
                                picture_context: None,
                                video: HoldVideo::Freeze {
                                    asset: asset(),
                                    timestamp: stamp(anchor_ordinal),
                                },
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        );
        let full_span = document.assets()[&asset()].video.unwrap();
        let span = if available {
            full_span
        } else {
            match direction {
                ExtensionDirection::FromLeft => SourceSpan::new(stamp(0), stamp(1)).unwrap(),
                ExtensionDirection::FromRight => {
                    SourceSpan::new(stamp(index.frames().len() - 1), full_span.end()).unwrap()
                }
            }
        };
        edit(
            &mut store,
            "target",
            Command::SetTarget {
                id: target(),
                target: AttentionTarget {
                    label: "Textured subject".into(),
                    asset: asset(),
                    span,
                    region: TargetRegion {
                        center: [500_000, 500_000],
                        size: [333_333, 333_333],
                    },
                    samples: vec![],
                    corrections: vec![],
                    provenance: None,
                },
            },
        );
        let origin = store.snapshot().unwrap();
        let inputs = prepare_extension_scoped_with_options(
            &package,
            origin.revision_id(),
            &ScopedNodeTarget {
                node: node("hold"),
                repeats: vec![],
            },
            direction,
            &GenerationOptions {
                region_target: GenerationTarget::Saved(target()),
                ..Default::default()
            },
            &active(),
        )
        .unwrap();
        let context: ExtensionContext = serde_json::from_slice(&inputs.manifest).unwrap();
        assert!(matches!(
            context.anchor().picture,
            BoundaryPicture::Original { .. }
        ));
        assert!(matches!(context.opposite(), ExtensionOppositeSeam::Absent));
        assert!(context.region().target_id().is_some());
        let raster = [
            inputs.plan.native_dimensions().width(),
            inputs.plan.native_dimensions().height(),
        ];
        assert_eq!(
            context
                .region()
                .seed(context.anchor(), context.presentation(), raster)
                .unwrap()
                .is_some(),
            available
        );
        std::fs::create_dir(directory.path().join("inputs")).unwrap();
        std::fs::create_dir(directory.path().join("outputs")).unwrap();
        let manifest_ref = WorkspaceRef::new("inputs/context.json").unwrap();
        std::fs::write(
            directory.path().join(manifest_ref.as_str()),
            &inputs.manifest,
        )
        .unwrap();
        for (retained, bytes) in context.context().iter().zip(&inputs.context_pngs) {
            std::fs::write(
                directory.path().join(retained.frame.reference().as_str()),
                bytes,
            )
            .unwrap();
        }
        std::fs::write(
            directory
                .path()
                .join(context.continuity().signatures().reference().as_str()),
            &inputs.continuity_signatures,
        )
        .unwrap();
        let declaration = WorkspaceArtifact::new(
            manifest_ref.clone(),
            inputs.manifest_sha256.clone(),
            inputs.manifest.len() as u64,
        )
        .unwrap();
        let request = HostMessage::GenerateExtension {
            protocol: ProtocolVersion::V3,
            identity: MessageIdentity::new(
                RequestId::new("selected-extension").unwrap(),
                AttemptId::new("inspection").unwrap(),
            ),
            cancellation_token: CancellationToken::new("cancel-selected").unwrap(),
            project_id: origin.project_id().clone(),
            revision_id: origin.revision_id().clone(),
            target: HoldTarget {
                hold_id: node("hold"),
                request_version: RequestVersion::new(1).unwrap(),
            },
            input: ContextArtifact {
                manifest: manifest_ref,
                sha256: inputs.manifest_sha256.clone(),
            },
            output_workspace: WorkspaceRef::new("outputs").unwrap(),
            constraints: inputs.constraints,
            provider: Box::new(deadpan_cli::generation::development_provider(1)),
            plan: Box::new(inputs.plan),
        };
        let conditioning = capture_extension_conditioning(
            &ArtifactWorkspace::open(directory.path()).unwrap(),
            &request,
            &declaration,
            &WorkspaceRef::new("inputs").unwrap(),
            ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000).unwrap(),
            &active(),
        )
        .unwrap();
        let anchor_bytes = match direction {
            ExtensionDirection::FromLeft => inputs.context_pngs.last().unwrap(),
            ExtensionDirection::FromRight => &inputs.context_pngs[0],
        };
        let anchor = image::load_from_memory(anchor_bytes).unwrap().to_rgb8();
        let media = native_movie(directory.path(), "static", &request, &anchor, None);
        // The public capture and object registration leave authored state alone.
        assert_eq!(store.snapshot().unwrap(), origin);
        Self {
            directory,
            request,
            conditioning,
            media,
            anchor,
        }
    }

    pub fn drifted_media(&self) -> CanonicalExtension {
        let context = self.conditioning.context();
        let raster = [self.anchor.width(), self.anchor.height()];
        let seed = context
            .region()
            .seed(context.anchor(), context.presentation(), raster)
            .unwrap()
            .unwrap();
        let x = (seed.x() * f64::from(raster[0])).floor() as u32;
        let y = (seed.y() * f64::from(raster[1])).floor() as u32;
        let right = ((seed.x() + seed.width()) * f64::from(raster[0])).ceil() as u32;
        let bottom = ((seed.y() + seed.height()) * f64::from(raster[1])).ceil() as u32;
        native_movie(
            self.directory.path(),
            "drift",
            &self.request,
            &self.anchor,
            Some([x, y, right - x, bottom - y]),
        )
    }
}

fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}
fn target() -> TargetId {
    TargetId::new("subject").unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn edit(store: &mut ProjectStore, name: &str, command: Command) {
    let before = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

fn original_project(package: &Path, source: &Path) -> ProjectStore {
    let initial = ProjectDocument::new(
        ProjectId::new("extension-selected-quality").unwrap(),
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
    let mut store = ProjectStore::create(package, &initial).unwrap();
    let limits = OriginalMediaLimits::new(4 * 1024 * 1024, Duration::from_secs(30)).unwrap();
    let original = store
        .retain_original(
            source,
            OriginalOwnership::Linked { bookmark: None },
            limits,
            &active(),
        )
        .unwrap()
        .record;
    let snapshot = store
        .snapshot_original(original.object().content(), limits, &active())
        .unwrap();
    let input = snapshot
        .into_source_input(
            SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        )
        .unwrap();
    let video =
        SourceSession::open_input(input, asset(), SourceSessionLimits::default(), &active())
            .unwrap();
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), None).unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: revision("initial"),
                new_revision: revision("registered"),
                original: original.object().content().clone(),
                new_asset_id: asset(),
                label: "Textured Original".into(),
                insertion: Some(SourceInsertionRequest {
                    parent: node("root"),
                    index: 0,
                    node: node("source"),
                    label: "Original".into(),
                    purpose: Default::default(),
                }),
            },
            &decoded,
            None,
            limits,
            &active(),
        )
        .unwrap();
    store
}

fn subject_picture() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let pixel = if (64..128).contains(&x) && (36..72).contains(&y) {
                let bit = ((x / 5 + y / 4) % 2) as u8;
                [
                    100 + bit * 110,
                    80 + ((x * 7 + y * 13) % 150) as u8,
                    220 - bit * 100,
                ]
            } else {
                [24, 28, 32]
            };
            bytes.extend_from_slice(&pixel);
        }
    }
    bytes
}

fn native_movie(
    directory: &Path,
    name: &str,
    request: &HostMessage,
    anchor: &image::RgbImage,
    drift: Option<[u32; 4]>,
) -> CanonicalExtension {
    let HostMessage::GenerateExtension { plan, .. } = request else {
        unreachable!()
    };
    let (width, height) = anchor.dimensions();
    let mut generated = Vec::new();
    for ordinal in 0..GENERATED {
        let mut picture = anchor.clone();
        if let Some([x, y, box_width, box_height]) = drift {
            let background = *anchor.get_pixel(x.checked_sub(4).unwrap(), y);
            for dy in 0..box_height {
                for dx in 0..box_width {
                    picture.put_pixel(x + dx, y + dy, background);
                }
            }
            // Gradual 16px steps keep association possible; the final pictures
            // exceed the unchanged presentation-diagonal displacement limit.
            let shift = ordinal * 16;
            assert!(x + box_width + shift <= width);
            for dy in 0..box_height {
                for dx in 0..box_width {
                    picture.put_pixel(x + dx + shift, y + dy, *anchor.get_pixel(x + dx, y + dy));
                }
            }
        }
        generated.push(picture.into_raw());
    }
    if plan.direction() == ExtensionDirection::FromRight {
        generated.reverse();
    }
    let context = vec![anchor.as_raw().clone(); CONTEXT as usize];
    let pictures = match plan.direction() {
        ExtensionDirection::FromLeft => [context, generated].concat(),
        ExtensionDirection::FromRight => [generated, context].concat(),
    };
    let output = directory.join(name);
    std::fs::create_dir(&output).unwrap();
    let bytes = encode_rgb(
        &output,
        &pictures.concat(),
        width,
        height,
        CONTEXT + GENERATED,
    );
    canonicalize_extension(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(&bytes),
        InputIdentity {
            sha256: sha2::Sha256::digest(&bytes).into(),
        },
        &ExtensionConversionRequest {
            protocol: EXTENSION_PROTOCOL_VERSION,
            operation: ExtensionOperation::SampleExtension,
            native: VideoContract {
                width,
                height,
                frames: CONTEXT + GENERATED,
                rate_num: 24,
                rate_den: 1,
            },
            sampling: plan.sampling_map().clone(),
            input_byte_length: bytes.len() as u64,
            limits: ConversionLimits {
                max_input_bytes: bytes.len() as u64,
                max_output_bytes: 16 * 1024 * 1024,
                max_scratch_bytes: u64::from(width * height * 3 * (CONTEXT + GENERATED)),
                timeout_ms: 30_000,
            },
        },
        &active(),
    )
    .unwrap()
}
