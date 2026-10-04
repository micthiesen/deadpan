#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PreparationError, PreparedSource, ResampleRecipe, StageAudio,
    StageAudioError, StageLimits,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{AudioDefinitionSelector, RenderPlan, SignalSample};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn ntsc() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}

fn audio(selected: Range<i64>) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: selected.start,
                time_base,
            },
            SourceTimestamp {
                ticks: selected.end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(rate: FrameRate, duration: i64) -> BeatNode {
    let audio = audio(100..20_100);
    BeatNode {
        framing: None,
        label: "Original samples".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(duration),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn silence(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            picture_context: None,
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn repeat(child: &str, plays: u32) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Repeated source".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
            gap: None,
            escalation: None,
        },
    }
}

fn partition(child: &str, selected: Range<i64>) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Retained allocation".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(selected.end - selected.start),
            mapping: FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end))
                .unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    }
}

fn preserve(child: &str, input: i64, output: i64) -> BeatNode {
    let mut node = partition(child, 0..input);
    let NodeKind::Retime {
        duration,
        pitch,
        purpose,
        ..
    } = &mut node.kind
    else {
        unreachable!()
    };
    *duration = frames(output);
    *pitch = PitchPolicy::Preserve;
    *purpose = RetimePurpose::Edit;
    node
}

fn document(rate: FrameRate, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("reanchor-pcm").unwrap(),
        revision("r0"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(key, node)| (id(key), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|key| id(key)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Known 44.1 kHz PCM".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(0..44_117).span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn timing(ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: revision("timings"),
        ordinal,
    }
}

fn placement(
    ordinal: u32,
    owner: &str,
    repeated: bool,
    birth_root: &str,
) -> AudioPlacementTemplate {
    AudioPlacementTemplate {
        reference_local_offset: deadpan_core::ExactRatio::ZERO,
        gap_after: None,
        reference: AudioReferenceClock {
            recipe: AudioRecipeKind::Node,
            timing: timing(ordinal),
            root: AudioClockRoot::ProjectRootRoundEven,
            physical: id(owner),
        },
        arguments: if repeated {
            vec![AudioRepeatArgument {
                reference_repeat: id("repeat"),
                value: AudioRepeatValue::Live {
                    repeat: id("repeat"),
                },
            }]
        } else {
            vec![]
        },
        births: if repeated {
            vec![AudioBirthClause {
                repeat: id("repeat"),
                survivors: AudioBirthSurvivors::CapturedRepeat {
                    repeat: id("repeat"),
                },
                definition_root: id(birth_root),
            }]
        } else {
            vec![]
        },
    }
}

fn step(placement: AudioPlacementTemplate, window: Option<Range<i64>>) -> AudioReanchorStep {
    AudioReanchorStep {
        anchor: Default::default(),
        placement,
        window: window.map(|range| {
            ExactFrameRange::new(
                ExactRatio::integer(range.start),
                ExactRatio::integer(range.end),
            )
            .unwrap()
        }),
    }
}

fn bind(
    current: &ProjectDocument,
    layouts: &[&ProjectDocument],
    owner: &str,
    binding: OwnedAudioBinding,
) -> ProjectDocument {
    let state = AudioBindingState::new(
        layouts
            .iter()
            .enumerate()
            .map(|(index, doc)| AudioTimingRecord {
                id: timing(u32::try_from(index).unwrap()),
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            })
            .collect(),
        BTreeMap::from([(id(owner), binding)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

struct Provider {
    prepared: PreparedSource,
    revisions: BTreeSet<RevisionId>,
    calls: usize,
}

impl Provider {
    fn new() -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        let prepared = PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
            &cancelled,
        )
        .unwrap();
        Self {
            prepared,
            revisions: BTreeSet::from([revision("r0")]),
            calls: 0,
        }
    }
}

impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project, &ProjectId::new("reanchor-pcm").unwrap());
        assert!(self.revisions.contains(revision));
        assert_eq!(asset, &AssetId::new("media").unwrap());
        self.calls += 1;
        Ok(&self.prepared)
    }
}

fn renderer(document: &ProjectDocument, provider: &mut Provider) -> StageAudio {
    provider.revisions.insert(document.revision_id().clone());
    StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()))
}

fn read(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    renderer
        .read(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

// Coordinates are handwritten sample-clock arithmetic, not resolved binding
// anchors or production phase descriptors. Only decode and the qualified
// reconstruction kernel are shared with StageAudio.
fn expected(provider: &Provider, mix_phase: ExactRatio, count: u32) -> Vec<[f32; 2]> {
    provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..20_100,
                ExactRatio::integer(100)
                    .checked_add(mix_phase.checked_mul(ratio(147, 160)).unwrap())
                    .unwrap(),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(i64::from(count)),
            )
            .unwrap(),
            AudioSample(0),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn assert_close(actual: &[[f32; 2]], expected: &[[f32; 2]]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert!(
                (actual[channel] - expected[channel]).abs() <= 2e-6,
                "sample {index}, channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let tx = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let changed = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&changed).unwrap(), *document);
    changed
}

#[test]
fn repeat_suffix_reanchors_first_partial_play_and_later_full_play_separately() {
    let rate = ntsc();
    let old = document(
        rate,
        &["repeat"],
        vec![("a", source(rate, 2)), ("repeat", repeat("a", 2))],
    );
    let current = document(
        rate,
        &["prefix", "pause", "suffix"],
        vec![
            ("prefix", source(rate, 1)),
            ("pause", silence(1)),
            ("suffix", partition("repeat", 1..4)),
            ("repeat", repeat("a", 2)),
            ("a", source(rate, 2)),
        ],
    );
    let template = placement(0, "a", true, "a");
    let bound = bind(
        &current,
        &[&old],
        "a",
        OwnedAudioBinding {
            lattice: template.clone(),
            resume: None,
            reanchors: vec![step(template, Some(1..4))],
        },
    );
    let mut provider = Provider::new();
    let mut original = renderer(&old, &mut provider);
    let mut actual = renderer(&bound, &mut provider);
    // B(1)=1602, B(2)=3203, B(3)=4805. The surviving first suffix
    // resumes old sample 1602. The second play resumes old 3203, whose local
    // waveform phase is -1/5 because its exact origin was 3203.2.
    for (new, old_start, phase) in [
        (3203, 1602, ExactRatio::integer(1602)),
        (4805, 3203, ratio(-1, 5)),
    ] {
        let oracle = expected(&provider, phase, 128);
        let full = read(&mut actual, &mut provider, new, 128);
        assert_close(&full, &oracle);
        assert_eq!(full, read(&mut original, &mut provider, old_start, 128));
        for (offset, count) in [(91, 37), (0, 51), (51, 40)] {
            assert_eq!(
                read(&mut actual, &mut provider, new + offset, count),
                full[offset as usize..offset as usize + count as usize]
            );
        }
        let mut fresh = renderer(&bound, &mut provider);
        assert_eq!(read(&mut fresh, &mut provider, new + 91, 37), full[91..]);
    }
    let silence = actual
        .read(
            &mut provider,
            AudioSample(1602),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(silence.samples, vec![[0.; 2]; 128]);
    assert_eq!(
        silence.suppressed,
        vec![AudioSample(1602)..AudioSample(1730)]
    );

    let split = edit(
        &bound,
        "partition",
        Command::Split {
            node: id("a"),
            at: frames(1),
            identities: SplitIdentities {
                nodes: (0..12).map(|n| id(&format!("transparent-{n}"))).collect(),
            },
        },
    );
    let mut divided = renderer(&split, &mut provider);
    for at in [3203, 4677, 4805, 6278] {
        assert_eq!(
            read(&mut divided, &mut provider, at, 64),
            read(&mut actual, &mut provider, at, 64)
        );
    }
}

#[test]
fn sequential_reanchors_compose_prior_resume_on_each_retained_ntsc_grid() {
    let rate = ntsc();
    let old = document(rate, &["a"], vec![("a", source(rate, 6))]);
    let first = document(
        rate,
        &["lead", "a"],
        vec![("lead", silence(1)), ("a", source(rate, 6))],
    );
    let second = document(
        rate,
        &["lead", "a"],
        vec![("lead", silence(2)), ("a", source(rate, 6))],
    );
    let current = document(
        rate,
        &["lead", "suffix"],
        vec![
            ("lead", silence(4)),
            ("suffix", partition("a", 2..6)),
            ("a", source(rate, 6)),
        ],
    );
    let bound = bind(
        &current,
        &[&old, &first, &second],
        "a",
        OwnedAudioBinding {
            lattice: placement(0, "a", false, "a"),
            resume: Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ratio(64 * 5, 8008),
                    terms: vec![],
                },
            }),
            reanchors: vec![
                step(placement(1, "a", false, "a"), Some(2..7)),
                step(placement(2, "a", false, "a"), Some(4..8)),
            ],
        },
    );
    // The first move adds B(2)-B(1)=1601; the second adds B(4)-B(3)=1601.
    // Collapsing both onto the last placement adds 3203 instead of 3202.
    let mut provider = Provider::new();
    let mut actual = renderer(&bound, &mut provider);
    let oracle = expected(&provider, ExactRatio::integer(3266), 256);
    let full = read(&mut actual, &mut provider, 6406, 256);
    assert_close(&full, &oracle);
    assert_ne!(full, expected(&provider, ExactRatio::integer(3267), 256));
    for (offset, count) in [(191, 65), (0, 79), (79, 112)] {
        assert_eq!(
            read(&mut actual, &mut provider, 6406 + offset, count),
            full[offset as usize..offset as usize + count as usize]
        );
    }
    let instance = InstancePath {
        node: id("a"),
        repeats: vec![],
    };
    assert!(
        bound
            .audio_bindings()
            .resolve(&id("a"), &instance, 1)
            .is_err()
    );
    let cancelled = AtomicBool::new(true);
    let calls = provider.calls;
    assert!(
        actual
            .read(&mut provider, AudioSample(6406), 1, TIMEOUT, &cancelled)
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, calls);
}

#[test]
fn born_play_drops_outer_window_but_preserves_intrinsic_partition_entry() {
    let rate = ntsc();
    let old = document(
        rate,
        &["repeat"],
        vec![
            ("a", source(rate, 3)),
            ("cut", partition("a", 1..3)),
            ("repeat", repeat("cut", 2)),
        ],
    );
    let template = placement(0, "a", true, "cut");
    let bound = bind(
        &old,
        &[&old],
        "a",
        OwnedAudioBinding {
            lattice: template.clone(),
            resume: None,
            reanchors: vec![step(template, Some(2..4))],
        },
    );
    let grown = edit(
        &bound,
        "birth",
        Command::InsertPlays {
            node: id("repeat"),
            index: 0,
            count: 1,
        },
    );
    let mut provider = Provider::new();
    let mut actual = renderer(&grown, &mut provider);
    // A new play has a definition PointCeil grid. The intrinsic Partition
    // enters source frame 1, exactly 1601.6 mix samples, even though its old
    // enclosing [2,4) window did not allocate the original first play.
    let oracle = expected(&provider, ratio(8008, 5), 128);
    let full = read(&mut actual, &mut provider, 0, 128);
    assert_close(&full, &oracle);
    assert_ne!(full, expected(&provider, ExactRatio::ZERO, 128));
    let plan = Arc::new(RenderPlan::compile(&grown).unwrap());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        })
        .unwrap();
    let mut definition_renderer = StageAudio::new(Arc::clone(&plan));
    let intrinsic = definition_renderer
        .read_definition(
            &mut provider,
            &definition,
            SignalSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(intrinsic.samples, full);
    let mut fresh = renderer(&grown, &mut provider);
    assert_eq!(read(&mut fresh, &mut provider, 91, 37), full[91..]);
}

#[test]
fn insert_time_after_a_reanchor_resumes_the_pre_edit_bound_waveform() {
    let rate = ntsc();
    let old = document(rate, &["a"], vec![("a", source(rate, 6))]);
    let captured = document(
        rate,
        &["lead", "a"],
        vec![("lead", silence(1)), ("a", source(rate, 6))],
    );
    let current = document(
        rate,
        &["lead", "suffix"],
        vec![
            ("lead", silence(2)),
            ("suffix", partition("a", 1..6)),
            ("a", source(rate, 6)),
        ],
    );
    let bound = bind(
        &current,
        &[&old, &captured],
        "a",
        OwnedAudioBinding {
            lattice: placement(0, "a", false, "a"),
            resume: None,
            reanchors: vec![step(placement(1, "a", false, "a"), Some(2..7))],
        },
    );
    let inserted = edit(
        &bound,
        "pause",
        Command::InsertTime {
            at: ProjectFrame(3),
            hold: HoldRecipe {
                picture_context: None,
                duration: frames(1),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: id("inserted-pause"),
            identities: SplitIdentities {
                nodes: (0..bound.nodes().len() + 4)
                    .map(|n| id(&format!("pause-part-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("pause"),
                ordinal: 0,
            },
        },
    );
    let mut provider = Provider::new();
    let mut before = renderer(&bound, &mut provider);
    let mut after = renderer(&inserted, &mut provider);
    // The existing step began at source mix point 1601 at current B(2).
    // Cutting at B(3) adds 4805-3203=1602, so the new suffix at B(4)
    // must resume source mix point 3203, not recompute source frame 2.
    let oracle = expected(&provider, ExactRatio::integer(3203), 256);
    let full = read(&mut after, &mut provider, 6406, 256);
    assert_close(&full, &oracle);
    assert_eq!(full, read(&mut before, &mut provider, 4805, 256));
    assert_eq!(
        read(&mut after, &mut provider, 3203, 128),
        read(&mut before, &mut provider, 3203, 128)
    );
    let pause = after
        .read(
            &mut provider,
            AudioSample(4805),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(pause.samples, vec![[0.; 2]; 128]);
    assert_eq!(pause.suppressed, vec![AudioSample(4805)..AudioSample(4933)]);
    let mut fresh = renderer(&inserted, &mut provider);
    assert_eq!(read(&mut fresh, &mut provider, 6517, 145), full[111..]);
}

#[test]
fn hidden_retained_occurrence_does_not_replace_its_legacy_resume_anchor() {
    let rate = ntsc();
    let old = document(
        rate,
        &["repeat"],
        vec![("a", source(rate, 2)), ("repeat", repeat("a", 2))],
    );
    let hidden = document(
        rate,
        &["crop"],
        vec![
            ("crop", partition("repeat", 2..4)),
            ("repeat", repeat("a", 2)),
            ("a", source(rate, 2)),
        ],
    );
    let current = document(
        rate,
        &["lead", "repeat"],
        vec![
            ("lead", silence(2)),
            ("repeat", repeat("a", 2)),
            ("a", source(rate, 2)),
        ],
    );
    let bound = bind(
        &current,
        &[&old, &hidden],
        "a",
        OwnedAudioBinding {
            lattice: placement(0, "a", true, "a"),
            resume: Some(AudioResume {
                local_boundary: ExactRatio::ONE,
                phase: AudioLocalPhase {
                    constant: ratio(2048 * 5, 8008),
                    terms: vec![],
                },
            }),
            reanchors: vec![step(placement(1, "a", true, "a"), None)],
        },
    );
    let mut provider = Provider::new();
    let mut actual = renderer(&bound, &mut provider);
    // Current first play starts at B(2)=3203, but its retained local-1
    // resume anchor lies at B(3)=4805. Hidden history supplies no new entry:
    // old PCM 2048 + (3203 - 4805) = 446.
    let oracle = expected(&provider, ExactRatio::integer(446), 128);
    let full = read(&mut actual, &mut provider, 3203, 128);
    assert_close(&full, &oracle);
    assert_ne!(full, expected(&provider, ExactRatio::integer(447), 128));
}

fn stretched(input: &[[f32; 2]], output: u32) -> Vec<[f32; 2]> {
    let input = StereoPcm::new(
        input.iter().map(|x| x[0]).collect(),
        input.iter().map(|x| x[1]).collect(),
    )
    .unwrap();
    let recipe =
        CanonicalRecipe::with_rate(input.frames(), output, StretchRate::new(1, 3).unwrap(), 0)
            .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, input).unwrap();
    let mut left = vec![0.; output as usize];
    let mut right = vec![0.; output as usize];
    for (left, right) in left.chunks_mut(256).zip(right.chunks_mut(256)) {
        assert_eq!(
            dsp.read(left, right, &AtomicBool::new(false)).unwrap(),
            left.len()
        );
    }
    left.into_iter().zip(right).map(|(l, r)| [l, r]).collect()
}

#[test]
fn opaque_preserve_reanchor_reads_full_history_and_enforces_full_preparation_budget() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let old = document(
        rate,
        &["stage"],
        vec![("a", source(rate, 128)), ("stage", preserve("a", 128, 384))],
    );
    let captured = document(
        rate,
        &["lead", "cut"],
        vec![
            ("lead", silence(7)),
            ("cut", partition("stage", 64..384)),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
        ],
    );
    let current = document(
        rate,
        &["lead", "cut"],
        vec![
            ("lead", silence(100)),
            ("cut", partition("stage", 64..384)),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
        ],
    );
    let bound = bind(
        &current,
        &[&old, &captured],
        "stage",
        OwnedAudioBinding {
            lattice: placement(0, "stage", false, "stage"),
            resume: None,
            reanchors: vec![step(placement(1, "stage", false, "stage"), None)],
        },
    );
    let mut provider = Provider::new();
    // This Source host ends at input mix frame 128. Its genuine trim limits
    // filter context to [100, ceil(100 + 128*147/160)) = [100,218), even
    // though the registered source selection continues beyond that host.
    let input = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..218,
                ExactRatio::integer(100),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(128),
            )
            .unwrap(),
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let oracle = stretched(&input, 384);
    let mut original = renderer(&old, &mut provider);
    assert_close(&read(&mut original, &mut provider, 0, 256), &oracle[..256]);
    assert_close(
        &read(&mut original, &mut provider, 256, 128),
        &oracle[256..],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let mut actual = StageAudio::new(Arc::clone(&plan));
    // Seek near the end first. Preparation must still begin at intrinsic zero.
    assert_close(
        &read(&mut actual, &mut provider, 356, 64),
        &oracle[320..384],
    );
    assert_close(
        &read(&mut actual, &mut provider, 100, 256),
        &oracle[64..320],
    );
    assert_eq!(actual.cached_stage_count(), 1);
    let mut fresh = StageAudio::new(Arc::clone(&plan));
    assert_eq!(read(&mut fresh, &mut provider, 227, 127), oracle[191..318]);
    let mut limited = StageAudio::with_limits(
        plan,
        StageLimits {
            maximum_output_frames: 383,
            ..Default::default()
        },
    )
    .unwrap();
    let calls = provider.calls;
    assert!(matches!(
        limited.read(
            &mut provider,
            AudioSample(419),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("output frames"))
    ));
    assert_eq!(provider.calls, calls);
    assert_eq!(limited.cached_stage_count(), 0);
}
