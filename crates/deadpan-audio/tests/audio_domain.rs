#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, DomainSignalTransfer, PcmWindow, PreparationError, PreparedSource,
    ResampleRecipe, Resampler, RoomTone, RoomToneRecipe, StageAudio, StageAudioError, StageLimits,
    StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{RenderPlan, SignalSample};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn samples(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn audio(start: i64, end: i64, sample_rate: u32) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, sample_rate).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(rate: FrameRate, frames: i64, selected: Range<i64>, sample_rate: u32) -> BeatNode {
    let audio = audio(selected.start, selected.end, sample_rate);
    BeatNode {
        label: "Qualified original".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
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

fn hold(frames: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio,
        },
    )
}

fn retime(child: &str, frames: i64, selected: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Authored retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end))
                .unwrap(),
            pitch,
        },
    }
}

fn partition(child: &str, selected: Range<i64>) -> BeatNode {
    let mut node = retime(
        child,
        selected.end - selected.start,
        selected,
        PitchPolicy::FollowSpeed,
    );
    let NodeKind::Retime { purpose, .. } = &mut node.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    node
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    sample_rate: u32,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("domain-project").unwrap(),
        RevisionId::new("domain-revision").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
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
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "Measured PCM".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(
                    audio(
                        0,
                        if sample_rate == 48_000 { 8197 } else { 44_117 },
                        sample_rate,
                    )
                    .span,
                ),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        ),
        (
            AssetId::new("picture").unwrap(),
            AssetRecord {
                label: "Picture without audio".into(),
                content_hash: "b".repeat(64),
                video: Some(audio(0, 8197, 48_000).span),
                audio: None,
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        ),
    ]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn plan(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    sample_rate: u32,
) -> Arc<RenderPlan> {
    Arc::new(RenderPlan::compile(&document(rate, children, nodes, sample_rate)).unwrap())
}

struct FixtureProvider {
    prepared: PreparedSource,
    calls: usize,
    context_calls: usize,
    unavailable: bool,
    cancel_on_call: bool,
    call_delay: Duration,
}

impl FixtureProvider {
    fn new(sample_rate: u32) -> Self {
        let (filename, layout) = if sample_rate == 48_000 {
            ("pcm-stereo-48000.wav", stereo_layout())
        } else {
            (
                "pcm-mono-44100.wav",
                AudioChannelLayout::Native {
                    channels: 1,
                    mask: 4,
                },
            )
        };
        Self::from_fixture(filename, layout)
    }

    fn from_fixture(filename: &str, layout: AudioChannelLayout) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures")
                .join(filename),
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
        let prepared = PreparedSource::with_layout(session, &index, layout, &cancelled).unwrap();
        Self {
            prepared,
            calls: 0,
            context_calls: 0,
            unavailable: false,
            cancel_on_call: false,
            call_delay: Duration::ZERO,
        }
    }
}

impl AudioSourceProvider for FixtureProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project, &ProjectId::new("domain-project").unwrap());
        assert_eq!(revision, &RevisionId::new("domain-revision").unwrap());
        assert_eq!(asset, &AssetId::new("media").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        if self.unavailable {
            return Err(PreparationError::SourceUnavailable(
                "qualification revoked".into(),
            ));
        }
        if self.cancel_on_call {
            cancelled.store(true, Ordering::Relaxed);
        }
        if !self.call_delay.is_zero() {
            std::thread::sleep(self.call_delay);
        }
        Ok(&self.prepared)
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(expected.content_hash, "a".repeat(64));
        self.context_calls += 1;
        self.source(project, revision, asset, cancelled)
    }
}

fn stereo_layout() -> AudioChannelLayout {
    AudioChannelLayout::Native {
        channels: 2,
        mask: 3,
    }
}

fn fixture_sample(index: i64) -> [f32; 2] {
    let index = usize::try_from(index).unwrap();
    let left = match index % 2048 {
        0 => 24576,
        1 => -24576,
        512..=1023 => ((index * 97) % 16384) as i32 - 8192,
        _ => 0,
    };
    let right = match index % 257 {
        0 => -32768,
        1 => 32767,
        _ => (index % 97) as i32 * 3 - 48 * 3,
    };
    [left as f32 / 32768.0, right as f32 / 32768.0]
}

// These references choose the exact input domain and phase independently of
// RenderPlan. Shared qualified filtering/DSP keeps comparisons deterministic.
fn sample_reference(
    selection: Range<i64>,
    origin: ExactRatio,
    step: ExactRatio,
    frames: u32,
    value: impl Fn(i64) -> [f32; 2],
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        ResampleRecipe::new(
            selection,
            origin,
            AudioSample(0),
            step,
            samples(0, i64::from(frames)),
        )
        .unwrap(),
        StereoMatrix::new(stereo_layout()).unwrap(),
    );
    let mut output = Vec::new();
    while output.len() < frames as usize {
        let start = AudioSample(output.len() as i64);
        let count = (frames - output.len() as u32).min(256);
        let window = sampler
            .required_source_range(start, count)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: range.flat_map(&value).collect(),
            });
        output.extend(
            sampler
                .render(start, count, window, &AtomicBool::new(false))
                .unwrap()
                .samples,
        );
    }
    output
}

fn stretch(input: &[[f32; 2]], frames: u32, numerator: u64, denominator: u64) -> Vec<[f32; 2]> {
    let pcm = StereoPcm::new(
        input.iter().map(|frame| frame[0]).collect(),
        input.iter().map(|frame| frame[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        input.len() as u32,
        frames,
        StretchRate::new(numerator, denominator).unwrap(),
        0,
    )
    .unwrap();
    let mut renderer = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut output = Vec::new();
    while output.len() < frames as usize {
        let count = (frames as usize - output.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            renderer
                .read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        output.extend(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| [left, right]),
        );
    }
    output
}

#[test]
fn bounded_read_does_not_materialize_or_narrow_a_wide_signed_domain() {
    let rate = FrameRate::new(24, 1).unwrap();
    let mut absent = source(rate, 6_000_000_000_000_000, 0..8197, 48_000);
    let NodeKind::Source { source: picture } = &mut absent.kind else {
        unreachable!()
    };
    picture.audio = None;
    picture.audio_mapping = SourceAudioMapping::FitBeat;
    picture.video = SourceVideo::Stream {
        asset: AssetId::new("picture").unwrap(),
        span: audio(0, 8197, 48_000).span,
    };
    let planned = plan(
        rate,
        &["crop"],
        [
            ("picture", absent),
            (
                "crop",
                partition("picture", 3_000_000_000_000_000..3_000_000_000_000_001),
            ),
        ],
        48_000,
    );
    let domain = planned
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    assert_eq!(
        domain.root_samples(),
        samples(-6_000_000_000_000_000_000, 6_000_000_000_000_000_000)
    );
    assert_eq!(domain.visible_samples(), samples(0, 2000));
    let mut provider = FixtureProvider::new(48_000);
    provider.unavailable = true;
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    for start in [-6_000_000_000_000_000_000, 0, 5_999_999_999_999_999_999] {
        let block = renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(start),
                1,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, vec![[0.0; 2]]);
        assert!(
            block.suppressed.is_empty(),
            "absent audio is not a Hold policy"
        );
    }
    assert_eq!(provider.calls, 0);
    assert_eq!(provider.context_calls, 0);
}

#[test]
fn moved_partition_reads_its_hidden_source_instead_of_the_colliding_root_voice() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for prefix in [false, true] {
        let children = if prefix {
            vec!["x", "crop"]
        } else {
            vec!["crop"]
        };
        let mut nodes = vec![
            ("a", source(rate, 8, 0..8, 48_000)),
            ("crop", partition("a", 4..8)),
        ];
        if prefix {
            nodes.push(("x", source(rate, 6, 512..518, 48_000)));
        }
        let planned = plan(rate, &children, nodes, 48_000);
        let visible_start = if prefix { 6 } else { 0 };
        let meaningful_start = visible_start - 4;
        let domain = planned
            .audio_domain_at(AudioSample(visible_start), Default::default())
            .unwrap();
        assert_eq!(
            domain.root_samples(),
            samples(meaningful_start, visible_start + 4)
        );
        assert_eq!(
            domain.visible_samples(),
            samples(visible_start, visible_start + 4)
        );
        assert_eq!(domain.instance().node, id("a"));
        assert!(domain.gap_after().is_none());
        let mut provider = FixtureProvider::new(48_000);
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        // Read the visible suffix before recovering the context that is hidden
        // behind X, or lies at negative captured coordinates without X.
        for (offset, count) in [(4, 4), (0, 3), (3, 5)] {
            let block = renderer
                .read_domain(
                    &mut provider,
                    &domain,
                    AudioSample(meaningful_start + offset),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                (offset..offset + i64::from(count))
                    .map(fixture_sample)
                    .collect::<Vec<_>>()
            );
            assert_eq!(block.start, AudioSample(meaningful_start + offset));
            assert_eq!(block.root_samples, domain.root_samples());
            assert_eq!(block.visible_samples, domain.visible_samples());
            assert_eq!(&block.instance, domain.instance());
            assert!(block.suppressed.is_empty());
        }
        if prefix {
            let whole = renderer
                .read(
                    &mut provider,
                    AudioSample(2),
                    1,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(whole.samples, [fixture_sample(514)]);
            assert_ne!(whole.samples, [fixture_sample(0)]);
        }
    }
}

#[test]
fn ntsc_hidden_44100_context_keeps_the_captured_fractional_root_origin() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let planned = plan(
        rate,
        &["x", "crop"],
        [
            ("x", source(rate, 3, 10_000..15_000, 44_100)),
            ("a", source(rate, 4, 0..6000, 44_100)),
            ("crop", partition("a", 2..4)),
        ],
        44_100,
    );
    let domain = planned
        .audio_domain_at(AudioSample(4805), Default::default())
        .unwrap();
    assert_eq!(domain.root_samples(), samples(1602, 8008));
    assert_eq!(domain.visible_samples(), samples(4805, 8008));
    let original = |at: i64| [((at * 73 % 65_536 - 32_768) as f32) / 32_768.0; 2];
    // B(1)=1602 is 0.4 mix samples beyond frame 1's exact start.
    // Multiplication by 44100/48000 gives 147/400 source samples.
    // Four project frames contain 5885.88 original samples. The Source host
    // excludes later original PCM even though its selected span is longer.
    let expected = sample_reference(0..5886, ratio(147, 400), ratio(147, 160), 6406, original);
    let reset = sample_reference(0..5886, ExactRatio::ZERO, ratio(147, 160), 6406, original);
    assert_ne!(expected[..256], reset[..256]);
    let mut provider = FixtureProvider::new(44_100);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    for offset in (0..6406).step_by(173).collect::<Vec<_>>().into_iter().rev() {
        let count = (6406 - offset).min(173) as u32;
        let block = renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(1602 + offset as i64),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, expected[offset..offset + count as usize]);
        assert!(block.suppressed.is_empty());
    }
    let root = renderer
        .read(
            &mut provider,
            AudioSample(1602),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_ne!(root.samples, expected[..1]);
}

fn nested_fixture() -> (Arc<RenderPlan>, Vec<[f32; 2]>) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut absent = source(rate, 128, 0..128, 48_000);
    let NodeKind::Source { source: picture } = &mut absent.kind else {
        unreachable!()
    };
    picture.audio = None;
    picture.audio_mapping = SourceAudioMapping::FitBeat;
    picture.video = SourceVideo::Stream {
        asset: AssetId::new("picture").unwrap(),
        span: audio(0, 128, 48_000).span,
    };
    let planned = plan(
        rate,
        &["x", "crop"],
        [
            ("x", source(rate, 512, 512..1024, 48_000)),
            ("a", source(rate, 2048, 0..2048, 48_000)),
            ("absent", absent),
            ("silent", hold(128, HoldAudio::Silence)),
            ("b", source(rate, 1792, 2304..4096, 48_000)),
            (
                "cuts",
                BeatNode::sequence(
                    "Input policies",
                    vec![id("a"), id("absent"), id("silent"), id("b")],
                ),
            ),
            (
                "inner",
                retime("cuts", 6144, 0..4096, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 4096, 0..6144, PitchPolicy::Preserve),
            ),
            ("crop", partition("outer", 3000..4096)),
        ],
        48_000,
    );
    let input: Vec<_> = (0..4096)
        .map(|at| {
            if (2048..2304).contains(&at) {
                [0.0; 2]
            } else {
                fixture_sample(at)
            }
        })
        .collect();
    let mut inner = stretch(&input, 6144, 2, 3);
    inner[3264..3456].fill([0.0; 2]);
    let mut expected = stretch(&inner, 4096, 3, 2);
    expected[2176..2304].fill([0.0; 2]);
    (planned, expected)
}

#[test]
fn hidden_nested_preserve_keeps_full_history_and_distinguishes_absence_from_silence() {
    let (planned, expected) = nested_fixture();
    let domain = planned
        .audio_domain_at(AudioSample(512), Default::default())
        .unwrap();
    assert_eq!(domain.root_samples(), samples(-2488, 1608));
    assert_eq!(domain.visible_samples(), samples(512, 1608));
    assert!(
        expected[2048..2112]
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 1e-6)
    );
    let mut provider = FixtureProvider::new(48_000);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    // Suffix-first reads must prepare both complete stages even though most of
    // their history and both interesting input policies are outside the crop.
    for offset in (0..4096).step_by(197).collect::<Vec<_>>().into_iter().rev() {
        let count = (4096 - offset).min(197) as u32;
        let root_start = offset as i64 - 2488;
        let block = renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(root_start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, expected[offset..offset + count as usize]);
        let silent_start = root_start.max(2176 - 2488);
        let silent_end = (root_start + i64::from(count)).min(2304 - 2488);
        let suppressed = if silent_start < silent_end {
            vec![samples(silent_start, silent_end)]
        } else {
            vec![]
        };
        assert_eq!(block.suppressed, suppressed);
    }
    assert_eq!(renderer.cached_stage_count(), 2);
}

#[test]
fn ordinary_edit_crop_keeps_excluded_source_filter_context_out_of_the_domain() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["x", "partition"],
        [
            ("x", source(rate, 40, 0..40, 48_000)),
            ("a", source(rate, 2048, 512..2560, 48_000)),
            ("slow", retime("a", 3072, 0..2048, PitchPolicy::FollowSpeed)),
            (
                "edit",
                retime("slow", 256, 1500..1756, PitchPolicy::FollowSpeed),
            ),
            ("partition", partition("edit", 32..224)),
        ],
        48_000,
    );
    let domain = planned
        .audio_domain_at(AudioSample(40), Default::default())
        .unwrap();
    assert_eq!(domain.root_samples(), samples(8, 264));
    // Edit selected [1512, 1682+2/3) in source samples. Its filtering may use
    // only those selected original samples, despite the outer Partition.
    let expected = sample_reference(
        1512..1683,
        ExactRatio::integer(1512),
        ratio(2, 3),
        256,
        fixture_sample,
    );
    let leaked = sample_reference(
        512..2560,
        ExactRatio::integer(1512),
        ratio(2, 3),
        256,
        fixture_sample,
    );
    assert_ne!(expected, leaked);
    let mut provider = FixtureProvider::new(48_000);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let block = renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(8),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected);
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(7),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(264),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}

#[test]
fn hidden_room_tone_gap_keeps_its_loop_origin_and_rechecks_cached_admission() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["crop"],
        [
            ("a", source(rate, 128, 0..128, 48_000)),
            (
                "repeat",
                BeatNode {
                    label: "Repeated speech".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3)
                            .unwrap(),
                        gap: Some(HoldRecipe {
                            duration: duration(181),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone {
                                source: audio(512, 611, 48_000),
                            },
                        }),
                    },
                },
            ),
            ("crop", partition("repeat", 199..746)),
        ],
        48_000,
    );
    let input: Vec<_> = (512..611).map(fixture_sample).collect();
    let reference = RoomTone::new(
        RoomToneRecipe::new(ExactRatio::integer(99), 181).unwrap(),
        &input,
        &AtomicBool::new(false),
    )
    .unwrap();
    let expected = reference
        .render(AudioSample(0), 181, &AtomicBool::new(false))
        .unwrap()
        .samples;
    for frozen in [false, true] {
        let planned = Arc::new(if frozen {
            RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap()
        } else {
            RenderPlan::compile(&doc).unwrap()
        });
        let domain = planned
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        assert_eq!(domain.root_samples(), samples(-71, 110));
        assert_eq!(domain.visible_samples(), samples(0, 110));
        assert_eq!(domain.instance().node, id("repeat"));
        assert!(domain.gap_after().is_some());
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new(48_000);
        for (offset, count) in [(130, 51), (0, 71), (60, 105)] {
            let block = renderer
                .read_domain(
                    &mut provider,
                    &domain,
                    AudioSample(offset - 71),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                expected[offset as usize..offset as usize + count as usize]
            );
            assert_eq!(block.gap_after.as_ref(), domain.gap_after());
            assert!(block.suppressed.is_empty());
        }
        assert_eq!(renderer.cached_stage_count(), 1);
        assert_eq!(provider.context_calls > 0, frozen);
        let calls = provider.calls;
        provider.unavailable = true;
        assert!(matches!(
            renderer.read_domain(
                &mut provider,
                &domain,
                AudioSample(-71),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        assert_eq!(provider.calls, calls + 1);
    }
}

#[test]
fn domain_reads_reject_foreign_plans_ranges_cancellation_and_preparation_overruns() {
    let (planned, _) = nested_fixture();
    let (foreign, _) = nested_fixture();
    let domain = planned
        .audio_domain_at(AudioSample(512), Default::default())
        .unwrap();
    let mut provider = FixtureProvider::new(48_000);
    let mut renderer = StageAudio::new(foreign);
    assert!(matches!(
        renderer.read_domain(
            &mut provider,
            &domain,
            AudioSample(-2488),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.calls, 0);
    renderer = StageAudio::new(Arc::clone(&planned));
    for (start, frames, timeout) in [
        (-2489, 1, TIMEOUT),
        (1608, 1, TIMEOUT),
        (1607, 2, TIMEOUT),
        (-2488, 0, TIMEOUT),
        (-2488, 257, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (-2488, 1, Duration::ZERO),
        (-2488, 1, Duration::from_secs(61)),
    ] {
        assert!(
            renderer
                .read_domain(
                    &mut provider,
                    &domain,
                    AudioSample(start),
                    frames,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert_eq!(provider.calls, 0);
    }
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(-2488),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    for limits in [
        StageLimits {
            maximum_input_frames: 6143,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 100,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_frames: 20_479,
            ..Default::default()
        },
    ] {
        let mut limited = StageAudio::with_limits(Arc::clone(&planned), limits).unwrap();
        assert!(matches!(
            limited.read_domain(
                &mut provider,
                &domain,
                AudioSample(-2488),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(limited.cached_stage_count(), 0);
        assert_eq!(provider.calls, 0);
    }
    provider.cancel_on_call = true;
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(-2488),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn negative_hidden_domain_transfers_fractional_pcm_and_silence_into_a_new_preserve() {
    let (planned, materialized) = nested_fixture();
    let domain = planned
        .audio_domain_at(AudioSample(512), Default::default())
        .unwrap();
    let transfer = DomainSignalTransfer::new(
        domain,
        ratio(-1535, 3),
        SignalSample(10_000),
        ratio(147, 160),
        SignalSample(10_000)..SignalSample(11_024),
    )
    .unwrap();
    assert_eq!(
        transfer.root_position(SignalSample(10_000)).unwrap(),
        ratio(-1535, 3)
    );
    assert_eq!(transfer.domain().root_samples(), samples(-2488, 1608));
    // The mapped root positions are negative and wholly outside visible crop.
    // Their physical-domain local anchor is 1976+1/3, not root or signal zero.
    let mut expected = sample_reference(0..4096, ratio(5929, 3), ratio(147, 160), 1024, |at| {
        materialized[at as usize]
    });
    expected[218..357].fill([0.0; 2]);
    let mut provider = FixtureProvider::new(48_000);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut actual = vec![[0.0; 2]; 1024];
    for offset in (0..1024).step_by(173).collect::<Vec<_>>().into_iter().rev() {
        let count = (1024 - offset).min(173) as u32;
        let start = 10_000 + offset as i64;
        let block = renderer
            .read_domain_transferred(
                &mut provider,
                &transfer,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, expected[offset..offset + count as usize]);
        let silent_start = start.max(10_218);
        let silent_end = (start + i64::from(count)).min(10_357);
        let suppressed = if silent_start < silent_end {
            vec![SignalSample(silent_start)..SignalSample(silent_end)]
        } else {
            vec![]
        };
        assert_eq!(block.suppressed, suppressed);
        actual[offset..offset + count as usize].copy_from_slice(&block.samples);
    }
    assert_eq!(renderer.cached_stage_count(), 2);
    assert_eq!(stretch(&actual, 1280, 4, 5), stretch(&expected, 1280, 4, 5));
    let wrong = sample_reference(
        0..4096,
        ExactRatio::integer(1976),
        ratio(147, 160),
        1024,
        |at| materialized[at as usize],
    );
    assert_ne!(expected, wrong);
}

#[test]
fn transferred_domain_keeps_one_provenance_observation_across_halo_callbacks() {
    struct ChangingProvider {
        first: FixtureProvider,
        later: FixtureProvider,
        calls: usize,
    }
    impl AudioSourceProvider for ChangingProvider {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            self.calls += 1;
            if self.calls == 1 {
                self.first.source(project, revision, asset, cancelled)
            } else {
                self.later.source(project, revision, asset, cancelled)
            }
        }
    }
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["crop"],
        [
            ("a", source(rate, 4096, 0..4096, 48_000)),
            ("crop", partition("a", 3000..4096)),
        ],
        48_000,
    );
    let domain = planned
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let transfer = DomainSignalTransfer::new(
        domain,
        ratio(-7999, 3),
        SignalSample(0),
        ratio(3, 2),
        SignalSample(0)..SignalSample(256),
    )
    .unwrap();
    let mut provider = ChangingProvider {
        first: FixtureProvider::new(48_000),
        later: FixtureProvider::from_fixture(
            "pcm-stereo-48000.wav",
            AudioChannelLayout::Native {
                channels: 2,
                mask: 5,
            },
        ),
        calls: 0,
    };
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    assert!(matches!(
        renderer.read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::IndexMismatch
        ))
    ));
    assert_eq!(provider.calls, 2);
}

#[test]
fn transferred_domain_rejects_foreign_context_and_uses_the_full_stage_budget() {
    let (planned, _) = nested_fixture();
    let (foreign, _) = nested_fixture();
    let domain = planned
        .audio_domain_at(AudioSample(512), Default::default())
        .unwrap();
    let transfer = DomainSignalTransfer::new(
        domain,
        ExactRatio::integer(-1500),
        SignalSample(0),
        ratio(3, 2),
        SignalSample(0)..SignalSample(256),
    )
    .unwrap();
    let mut provider = FixtureProvider::new(48_000);
    let mut renderer = StageAudio::new(foreign);
    assert!(matches!(
        renderer.read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&planned),
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        renderer.read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    for (start, frames, timeout) in [
        (-1, 1, TIMEOUT),
        (256, 1, TIMEOUT),
        (0, 0, TIMEOUT),
        (0, 257, TIMEOUT),
        (0, 1, Duration::ZERO),
    ] {
        assert!(
            renderer
                .read_domain_transferred(
                    &mut provider,
                    &transfer,
                    SignalSample(start),
                    frames,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert_eq!(provider.calls, 0);
    }
    assert!(
        renderer
            .read_domain_transferred(
                &mut provider,
                &transfer,
                SignalSample(0),
                256,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    let expected = renderer
        .read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(renderer.cached_stage_count(), 2);
    assert_eq!(
        renderer
            .read_domain_transferred(
                &mut provider,
                &transfer,
                SignalSample(0),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        expected.samples
    );
    provider.unavailable = true;
    assert!(matches!(
        renderer.read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
}

#[test]
fn domain_transfer_masks_outside_signed_support_before_returning_filtered_pcm() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["crop"],
        [
            ("a", source(rate, 8, 0..8, 48_000)),
            ("crop", partition("a", 4..8)),
        ],
        48_000,
    );
    let domain = planned
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    assert_eq!(domain.root_samples(), samples(-4, 4));
    let transfer = DomainSignalTransfer::new(
        domain,
        ratio(-13, 3),
        SignalSample(10),
        ExactRatio::ONE,
        SignalSample(10)..SignalSample(22),
    )
    .unwrap();
    let mut expected = sample_reference(0..8, ratio(-1, 3), ExactRatio::ONE, 12, fixture_sample);
    assert_ne!(
        expected[0], [0.0; 2],
        "filter taps alone do not enforce policy"
    );
    expected[0] = [0.0; 2];
    expected[9..].fill([0.0; 2]);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let block = renderer
        .read_domain_transferred(
            &mut FixtureProvider::new(48_000),
            &transfer,
            SignalSample(10),
            12,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected);
    assert_eq!(
        block.suppressed,
        vec![
            SignalSample(10)..SignalSample(11),
            SignalSample(19)..SignalSample(22)
        ]
    );
}

#[test]
fn domain_transfer_halo_callbacks_share_one_deadline() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["crop"],
        [
            ("a", source(rate, 4096, 0..4096, 48_000)),
            ("crop", partition("a", 3000..4096)),
        ],
        48_000,
    );
    let domain = planned
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let transfer = DomainSignalTransfer::new(
        domain,
        ExactRatio::integer(-2800),
        SignalSample(0),
        ExactRatio::integer(4),
        SignalSample(0)..SignalSample(256),
    )
    .unwrap();
    let mut provider = FixtureProvider::new(48_000);
    provider.call_delay = Duration::from_millis(35);
    let mut renderer = StageAudio::new(planned.clone());
    let result = renderer.read_domain_transferred(
        &mut provider,
        &transfer,
        SignalSample(0),
        256,
        Duration::from_millis(50),
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(StageAudioError::Timeout)));
    // The halo spans seven bounded source callbacks. Resetting the deadline
    // per callback would visit them all before discovering the total overrun.
    assert!(
        provider.calls <= 2,
        "deadline reset across {} callbacks",
        provider.calls
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}
