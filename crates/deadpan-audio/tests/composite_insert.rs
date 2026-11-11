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
    StageAudio, StageAudioError, StageLimits, StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);
/// A limited read prepares a whole cold limiter tile with its halo; this is
/// the budget limited inspection gives it (`deadpan-cli` audio inspection).
const LIMITED_TIMEOUT: Duration = Duration::from_secs(60);

#[path = "composite_insert/bound_preserve.rs"]
mod bound_preserve;
#[path = "composite_insert/delete.rs"]
mod delete;
#[path = "composite_insert/delete_range.rs"]
mod delete_range;
#[path = "composite_insert/edited_slice.rs"]
mod edited_slice;
#[path = "composite_insert/explode.rs"]
mod explode;
#[path = "composite_insert/group_selection.rs"]
mod group_selection;
#[path = "composite_insert/repeat_selection.rs"]
mod repeat_selection;
#[path = "composite_insert/trim.rs"]
mod trim;

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
        cutaways: Vec::new(),
        captions: Vec::new(),
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
        cutaways: Vec::new(),
        captions: Vec::new(),
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
        ProjectId::new("composite-insert-pcm").unwrap(),
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
        assert_eq!(project, &ProjectId::new("composite-insert-pcm").unwrap());
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

#[track_caller]
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

fn insert_pause(document: &ProjectDocument, at: i64, duration: i64, name: &str) -> ProjectDocument {
    let result = edit(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: gap(duration, HoldAudio::Silence),
            id: id(&format!("{name}-pause")),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|index| id(&format!("{name}-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        result.duration().unwrap().frames(),
        document.duration().unwrap().frames() + duration
    );
    result
}

#[track_caller]
fn check_reads(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    assert_eq!(expected.len(), 128);
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
}

#[track_caller]
fn check_silence(document: &ProjectDocument, provider: &mut Provider, selected: Range<i64>) {
    let mut actual = renderer(document, provider);
    let calls = provider.calls;
    let mut start = selected.start;
    while start < selected.end {
        let count = u32::try_from((selected.end - start).min(251)).unwrap();
        let end = start + i64::from(count);
        let block = actual
            .read(
                provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, vec![[0.; 2]; count as usize]);
        assert_eq!(block.suppressed, vec![AudioSample(start)..AudioSample(end)]);
        start = end;
    }
    assert_eq!(provider.calls, calls, "silent Holds need no source I/O");
}

fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal,
    }
}

#[test]
fn inserting_before_preserve_crop_keeps_full_history_and_exact_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["lead", "crop", "tail"],
        vec![
            ("lead", silence(7)),
            ("crop", partition("stage", 64..384)),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
            ("tail", silence(11)),
        ],
    );
    let inserted = insert_pause(&original, 7, 93, "preserve-pause");
    let inserted_again = insert_pause(&inserted, 100, 5, "preserve-pause-again");
    let mut provider = Provider::new();
    // One project frame is one mix sample here. The genuine Source host
    // contains [100, ceil(100 + 128*147/160)) = [100,218) decoded samples.
    // Prepare its full 128-sample input and complete 384-sample Preserve
    // history, then take the authored [64,384) crop independently of the plan.
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
    for (document, start) in [(&original, 7), (&inserted, 100), (&inserted_again, 105)] {
        let mut actual = renderer(document, &mut provider);
        // A first seek at the final output points must still prepare from zero.
        assert_close(
            &read(&mut actual, &mut provider, start + 256, 64),
            &oracle[320..384],
        );
        assert_close(
            &read(&mut actual, &mut provider, start, 256),
            &oracle[64..320],
        );
        assert_eq!(actual.cached_stage_count(), 1);
        check_reads(document, &mut provider, start + 79, &oracle[143..271]);
        check_silence(document, &mut provider, start + 320..start + 331);
    }
    check_silence(&inserted, &mut provider, 7..100);
    check_silence(&inserted_again, &mut provider, 7..105);
    let mut limited = StageAudio::with_limits(
        Arc::new(RenderPlan::compile(&inserted_again).unwrap()),
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
            AudioSample(424),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit("output frames"))
    ));
    assert_eq!(provider.calls, calls);
    assert_eq!(limited.cached_stage_count(), 0);
}

#[test]
fn interior_pause_before_nested_retime_preserves_intrinsic_dsp_input_clock() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut follow = preserve("stage", 384, 192);
    let NodeKind::Retime { pitch, .. } = &mut follow.kind else {
        unreachable!()
    };
    *pitch = PitchPolicy::FollowSpeed;
    let original = document(
        rate,
        &["lead", "follow", "tail"],
        vec![
            ("lead", source(rate, 16)),
            ("follow", follow),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
            ("tail", silence(11)),
        ],
    );
    let first = insert_pause(&original, 7, 93, "nested-interior");
    let second = insert_pause(&first, 104, 5, "nested-interior-again");
    let mut provider = Provider::new();
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
    let preserved = stretched(&input, 384);
    let oracle = sample_reference(&preserved, ExactRatio::ZERO, ExactRatio::integer(2), 192);
    for (document, start) in [(&original, 16), (&first, 109), (&second, 114)] {
        // A cold seek into the end of the outer FollowSpeed still prepares
        // Preserve from its complete intrinsic input history, beginning at 0.
        let mut actual = renderer(document, &mut provider);
        assert_close(
            &read(&mut actual, &mut provider, start + 128, 64),
            &oracle[128..],
        );
        assert_eq!(actual.cached_stage_count(), 1);
        check_reads(document, &mut provider, start, &oracle[..128]);
        check_silence(document, &mut provider, start + 192..start + 203);
    }
    let intrinsic = &first.audio_bindings().bindings()[&id("a")];
    assert_eq!(intrinsic, &second.audio_bindings().bindings()[&id("a")]);
    assert_eq!(
        intrinsic.lattice.reference.root,
        AudioClockRoot::PreserveInputPointCeil { stage: id("stage") }
    );
    assert!(intrinsic.reanchors.is_empty());
    check_silence(&first, &mut provider, 7..100);
    check_silence(&second, &mut provider, 7..100);
    check_silence(&second, &mut provider, 104..109);
}
