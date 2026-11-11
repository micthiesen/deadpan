#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PcmWindow, PreparationError, PreparedSource, ResampleRecipe, Resampler,
    StageAudio, StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{AudioQueryLimits, RenderPlan};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

#[path = "gap_bindings/isolation.rs"]
mod isolation;

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
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal,
    }
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
fn gap(duration: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(duration),
        video: HoldVideo::Background,
        audio,
    }
}
fn room(duration: i64, selected: Range<i64>) -> HoldRecipe {
    gap(
        duration,
        HoldAudio::RoomTone {
            source: audio(selected),
        },
    )
}
fn silence(duration: i64) -> BeatNode {
    BeatNode::hold("Silence", gap(duration, HoldAudio::Silence))
}
fn repeated(plays: u32, recipe: HoldRecipe) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Gapped plays".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("child"),
            iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
            gap: Some(recipe),
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn partition(child: &str, selected: Range<i64>) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Visible suffix".into(),
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
        cutaways: Vec::new(),
        captions: Vec::new(),
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
        ProjectId::new("gap-binding-pcm").unwrap(),
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
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Decoded 44.1 kHz fixture".into(),
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

fn install(document: &ProjectDocument, bindings: AudioBindingState) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn capture(document: &ProjectDocument) -> ProjectDocument {
    install(
        document,
        capture_unbound_audio_bindings(
            document,
            AudioTimingId {
                allocation: revision("capture"),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
}
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    result
}
fn set_gap(
    document: &ProjectDocument,
    name: &str,
    plays: u32,
    recipe: HoldRecipe,
) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SetRepeat {
            node: id("repeat"),
            plays,
            gap: Some(recipe),
        },
    )
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
        assert_eq!(project, &ProjectId::new("gap-binding-pcm").unwrap());
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

// Only decoding and the qualified reconstruction kernel are shared with the
// renderer. All loop geometry and sampling phases below are independent of
// RenderPlan and its resolved binding descriptors.
fn sample_reference(
    input: &[[f32; 2]],
    phase: ExactRatio,
    step: ExactRatio,
    count: u32,
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        ResampleRecipe::new(
            0..i64::try_from(input.len()).unwrap(),
            phase,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(i64::from(count)),
        )
        .unwrap(),
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    );
    let window = sampler
        .required_source_range(AudioSample(0), count)
        .unwrap()
        .map(|range| PcmWindow {
            start: range.start,
            samples: input[range.start as usize..range.end as usize]
                .iter()
                .flatten()
                .copied()
                .collect(),
        });
    sampler
        .render(AudioSample(0), count, window, &AtomicBool::new(false))
        .unwrap()
        .samples
}
fn room_reference(provider: &Provider, selected: Range<i64>, count: usize) -> Vec<[f32; 2]> {
    let extent = ratio(i128::from(selected.end - selected.start) * 160, 147);
    let length = i64::try_from(extent.ceil().unwrap()).unwrap();
    let input = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                selected.clone(),
                ExactRatio::integer(selected.start),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(length),
            )
            .unwrap(),
            AudioSample(0),
            u32::try_from(length).unwrap(),
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let fade = ExactRatio::integer(96);
    let period = extent.checked_sub(fade).unwrap();
    (0..count)
        .map(|point| {
            let at = ExactRatio::integer(i64::try_from(point).unwrap());
            let cycle = at.checked_div(period).unwrap().floor();
            let local = at
                .checked_sub(period.checked_mul(ratio(cycle, 1)).unwrap())
                .unwrap();
            let head = sample_reference(&input, local, ExactRatio::ONE, 1)[0];
            if cycle == 0 || local.checked_sub(fade).unwrap().compare_integer(0).is_ge() {
                head
            } else {
                let tail = sample_reference(
                    &input,
                    period.checked_add(local).unwrap(),
                    ExactRatio::ONE,
                    1,
                )[0];
                let weight = local.checked_div(fade).unwrap();
                let weight = weight.numerator() as f64 / weight.denominator() as f64;
                std::array::from_fn(|channel| {
                    (f64::from(tail[channel]) * (1. - weight) + f64::from(head[channel]) * weight)
                        as f32
                })
            }
        })
        .collect()
}
#[track_caller]
fn assert_close(actual: &[[f32; 2]], expected: &[[f32; 2]]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            assert!(
                (actual[channel] - expected[channel]).abs() <= 2e-6,
                "sample {index}, channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

fn stretch(input: &[[f32; 2]], output: u32, denominator: u64) -> Vec<[f32; 2]> {
    let pcm = StereoPcm::new(
        input.iter().map(|x| x[0]).collect(),
        input.iter().map(|x| x[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        pcm.frames(),
        output,
        StretchRate::new(1, denominator).unwrap(),
        0,
    )
    .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut left = vec![0.; output as usize];
    let mut right = vec![0.; output as usize];
    assert_eq!(
        dsp.read(&mut left, &mut right, &AtomicBool::new(false))
            .unwrap(),
        output as usize
    );
    left.into_iter()
        .zip(right)
        .map(|(left, right)| [left, right])
        .collect()
}

#[test]
fn bound_silent_gap_with_zero_input_points_suppresses_nested_preserve_output() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let original = document(
        rate,
        &["inner"],
        vec![
            ("repeat", repeated(2, gap(1, HoldAudio::Silence))),
            ("child", BeatNode::hold("Room", room(1, 100..321))),
            ("inner", preserve("repeat", 3, 24)),
        ],
    );
    let captured = capture(&original);
    let mut wire = serde_json::to_value(&captured).unwrap();
    wire["nodes"]["outer"] = serde_json::to_value(preserve("inner", 24, 48)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("outer")])).unwrap();
    let nested = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = Provider::new();
    let one = room_reference(&provider, 100..321, 1);
    // Input [1,2) frames is [0.25,0.5) samples and owns no point.
    // It must nevertheless mute inner [2,4), then outer [4,8).
    let mut inner = stretch(&one, 6, 8);
    inner[2..4].fill([0.; 2]);
    let mut expected = stretch(&inner, 12, 2);
    expected[4..8].fill([0.; 2]);
    assert!(
        expected[..4]
            .iter()
            .chain(&expected[8..])
            .flatten()
            .any(|value| *value != 0.)
    );
    let mut actual = renderer(&nested, &mut provider);
    let whole = actual
        .read(
            &mut provider,
            AudioSample(0),
            12,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_close(&whole.samples, &expected);
    assert_eq!(whole.suppressed, vec![AudioSample(4)..AudioSample(8)]);
    for (start, count) in [(8, 4), (0, 5), (5, 3)] {
        assert_eq!(
            read(&mut actual, &mut provider, start, count),
            whole.samples[start as usize..start as usize + count as usize]
        );
    }
}
