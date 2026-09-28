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
    StageAudio, StageAudioError, StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{
    AudioDefinitionSelector, AudioQueryLimits, AudioRootPlacement, ReferenceSample, RenderPlan,
    SignalSample,
};
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
fn selector() -> AudioDefinitionSelector {
    AudioDefinitionSelector::RepeatGap {
        repeat: id("repeat"),
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
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("child"),
            iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
            gap: Some(recipe),
        },
    }
}
fn partition(child: &str, selected: Range<i64>) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Visible suffix".into(),
        audio_treatments: Default::default(),
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
#[track_caller]
fn check_reads(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    let mut actual = renderer(document, provider);
    let whole = read(&mut actual, provider, start, 128);
    assert_close(&whole, expected);
    for (offset, count) in [(91, 37), (0, 51), (51, 40)] {
        assert_eq!(
            read(&mut actual, provider, start + offset, count),
            whole[offset as usize..offset as usize + count as usize]
        );
    }
    let mut fresh = renderer(document, provider);
    assert_eq!(read(&mut fresh, provider, start + 91, 37), whole[91..]);
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let domain = plan
        .audio_domain_at(AudioSample(start), AudioQueryLimits::default())
        .unwrap();
    let mut seeded = StageAudio::new(Arc::clone(&plan));
    let block = seeded
        .read_domain(
            provider,
            &domain,
            AudioSample(start),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(block.gap_after.is_some());
    assert_eq!(block.samples, whole);
}

#[test]
fn interrupted_first_gap_and_later_full_gap_resume_distinct_ntsc_phases() {
    let old = document(
        ntsc(),
        &["repeat"],
        vec![
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let current = document(
        ntsc(),
        &["lead", "suffix"],
        vec![
            ("lead", silence(3)),
            ("suffix", partition("repeat", 2..7)),
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let captured = capture(&old);
    let mut gaps = captured.audio_bindings().gap_bindings().clone();
    let binding = gaps.get_mut(&id("repeat")).unwrap();
    binding.reanchors.push(AudioReanchorStep {
        placement: binding.lattice.clone(),
        window: Some(ExactFrameRange::new(ExactRatio::integer(2), ExactRatio::integer(7)).unwrap()),
    });
    let bound = install(
        &current,
        AudioBindingState::new_with_gaps(
            captured
                .audio_bindings()
                .timings()
                .iter()
                .map(|(id, layout)| AudioTimingRecord {
                    id: id.clone(),
                    layout: layout.clone(),
                })
                .collect(),
            BTreeMap::new(),
            gaps,
        )
        .unwrap(),
    );
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    // B(1)=1602, B(2)=3203, B(4)=6406. The partial first gap
    // resumes at 3203 - 1601.6 = 1601.4; the later full gap retains
    // 6406 - 6406.4 = -0.4, despite moving to current B(5)=8008.
    for (start, phase) in [(4805, ratio(8007, 5)), (8008, ratio(-2, 5))] {
        check_reads(
            &bound,
            &mut provider,
            start,
            &sample_reference(&waveform, phase, ExactRatio::ONE, 128),
        );
    }
    // The explicit gap definition is a narrower coordinate domain even though
    // its NodeId equals the Repeat output. The old [2,7) window must be dropped.
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let definition = plan.audio_definition(selector()).unwrap();
    let mut actual = StageAudio::new(Arc::clone(&plan));
    let block = actual
        .read_definition(
            &mut provider,
            &definition,
            SignalSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_close(&block.samples, &waveform[..128]);
}

#[test]
fn captured_one_play_gap_and_former_final_gap_are_born_on_the_definition_clock() {
    let original = document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", silence(1)),
            ("repeat", repeated(1, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let captured = capture(&original);
    assert_eq!(captured.audio_bindings().gap_bindings().len(), 1);
    let grown = set_gap(&captured, "grow", 3, room(2, 100..321));
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    // Neither old final play zero nor new play one had a historical gap.
    // Both begin at canonical point zero, rather than root phase -1/5.
    for start in [3203, 8008] {
        check_reads(&grown, &mut provider, start, &waveform[..128]);
    }
    let two = set_gap(&original, "two-uncaptured", 2, room(2, 100..321));
    let two = capture(&two);
    let three = set_gap(&two, "three", 3, room(2, 100..321));
    check_reads(
        &three,
        &mut provider,
        3203,
        &sample_reference(&waveform, ratio(-1, 5), ExactRatio::ONE, 128),
    );
    check_reads(&three, &mut provider, 8008, &waveform[..128]);
    let plan = Arc::new(RenderPlan::compile(&captured).unwrap());
    let definition = plan.audio_definition(selector()).unwrap();
    let mut actual = StageAudio::new(Arc::clone(&plan));
    assert_close(
        &actual
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples,
        &waveform[..128],
    );

    // Bound definitions pin canonical point zero to B(current local zero),
    // independently on each consuming grid. They do not acquire the unbound
    // placement's fractional start phase. A 3/2 placement advances the
    // retained waveform by exactly 2/3 sample per output point.
    let placement = AudioRootPlacement::new(
        ratio(-1, 3),
        ratio(3, 2),
        ExactRatio::ZERO..ExactRatio::integer(2),
    )
    .unwrap();
    let root = definition.in_root_clock(placement.clone()).unwrap();
    assert_eq!(root.root_samples().start, AudioSample(-534));
    let block = actual
        .read_domain(
            &mut provider,
            &root,
            AudioSample(-534),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_close(
        &block.samples,
        &sample_reference(&waveform, ExactRatio::ZERO, ratio(2, 3), 128),
    );
    let point = definition.in_point_clock(placement, ratio(1, 7)).unwrap();
    assert_eq!(point.reference_samples().start, ReferenceSample(-762));
    let block = actual
        .read_point_domain(
            &mut provider,
            &point,
            ReferenceSample(-762),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_close(
        &block.samples,
        &sample_reference(&waveform, ExactRatio::ZERO, ratio(2, 3), 128),
    );
}

#[test]
fn gaps_survive_overridden_preceding_children_and_follow_stable_play_reordering() {
    let original = document(
        ntsc(),
        &["repeat"],
        vec![
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let overridden = edit(
        &original,
        "override",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: play(0),
            subtree: Subtree {
                root: id("long"),
                nodes: BTreeMap::from([(id("long"), silence(2))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let captured = capture(&overridden);
    let cleared = edit(
        &captured,
        "clear",
        Command::ClearPlayOverride {
            node: id("repeat"),
            iteration: play(0),
        },
    );
    let reordered = edit(
        &cleared,
        "reorder",
        Command::MovePlays {
            node: id("repeat"),
            start: 1,
            end: 2,
            destination: 0,
        },
    );
    assert_eq!(
        reordered.audio_bindings().gap_bindings(),
        captured.audio_bindings().gap_bindings()
    );
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    // The gap after play one keeps old origin 5 (phase 0). The gap after
    // the once-overridden play zero keeps old origin 2 (phase -1/5).
    for (start, phase) in [(1602, ExactRatio::ZERO), (6406, ratio(-1, 5))] {
        check_reads(
            &reordered,
            &mut provider,
            start,
            &sample_reference(&waveform, phase, ExactRatio::ONE, 128),
        );
    }
}

#[test]
fn moved_gap_uses_current_duration_media_and_policy_without_recapturing_phase() {
    let original = document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", silence(1)),
            ("repeat", repeated(2, gap(2, HoldAudio::Silence))),
            ("child", silence(1)),
        ],
    );
    let captured = capture(&original);
    let moved = edit(
        &captured,
        "move",
        Command::Move {
            node: id("repeat"),
            parent: id("root"),
            index: 0,
        },
    );
    let room = set_gap(&moved, "room", 2, room(3, 700..921));
    assert_eq!(room.audio_bindings(), moved.audio_bindings());
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 700..921, 4805);
    // The old silent gap occupied root [2,4), and therefore starts at
    // retained phase -1/5. Its live recipe now extends through frame 4.
    check_reads(
        &room,
        &mut provider,
        1602,
        &sample_reference(&waveform, ratio(-1, 5), ExactRatio::ONE, 128),
    );
    check_reads(
        &room,
        &mut provider,
        5000,
        &sample_reference(&waveform, ratio(16_989, 5), ExactRatio::ONE, 128),
    );
    let mut silent_renderer = renderer(&moved, &mut provider);
    let calls = provider.calls;
    let silent = silent_renderer
        .read(
            &mut provider,
            AudioSample(1602),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(silent.samples, vec![[0.; 2]; 128]);
    assert_eq!(
        silent.suppressed,
        vec![AudioSample(1602)..AudioSample(1730)]
    );
    assert_eq!(provider.calls, calls);

    let tail = set_gap(
        &room,
        "tail",
        2,
        gap(
            3,
            HoldAudio::Tail {
                source: audio(700..921),
                maximum: frames(2),
            },
        ),
    );
    assert_eq!(tail.audio_bindings(), room.audio_bindings());
    let mut actual = renderer(&tail, &mut provider);
    let calls = provider.calls;
    assert!(matches!(
        actual.read(
            &mut provider,
            AudioSample(1602),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Unsupported("effect tails"))
    ));
    assert_eq!(
        provider.calls, calls,
        "live Tail must fail before source I/O"
    );
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
