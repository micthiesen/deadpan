#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Decoded witnesses for the root-map primitive, before a combined Source command.
//! Expected sample labels and filter support are literal, independent of the plan.

use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PreparationError, PreparedSource, ResampleRecipe, StageAudio,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}
fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn sound_id() -> SoundId {
    SoundId::new("effect").unwrap()
}
fn asset() -> AssetId {
    AssetId::new("media").unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}
fn mix_frames(n: i128, d: i128) -> ExactRatio {
    ratio(n * 5, d * 8008)
}

#[derive(Clone, Copy)]
enum Fixture {
    Stereo48,
    Mono441,
}
impl Fixture {
    fn clock(self) -> u32 {
        match self {
            Self::Stereo48 => 48_000,
            Self::Mono441 => 44_100,
        }
    }
    fn samples(self) -> i64 {
        match self {
            Self::Stereo48 => 8197,
            Self::Mono441 => 44_117,
        }
    }
    fn file(self) -> &'static str {
        match self {
            Self::Stereo48 => "pcm-stereo-48000.wav",
            Self::Mono441 => "pcm-mono-44100.wav",
        }
    }
    fn layout(self) -> AudioChannelLayout {
        match self {
            Self::Stereo48 => AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            Self::Mono441 => AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
        }
    }
    fn step(self) -> ExactRatio {
        match self {
            Self::Stereo48 => ExactRatio::ONE,
            Self::Mono441 => ratio(147, 160),
        }
    }
    fn source(self) -> SourceAudio {
        let time_base = SourceTimeBase::new(1, self.clock()).unwrap();
        SourceAudio {
            asset: asset(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base,
                },
                SourceTimestamp {
                    ticks: self.samples(),
                    time_base,
                },
            )
            .unwrap(),
        }
    }
}

struct Provider {
    prepared: PreparedSource,
    calls: usize,
}
impl Provider {
    fn new(fixture: Fixture) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures")
                .join(fixture.file()),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(
                Sha256::digest(&bytes).into(),
                u64::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            0,
            // At most 256 output samples plus the fixed 128-sample kernel halo
            // on either side. The 44.1 kHz step is below one source sample.
            AudioSessionLimits {
                maximum_read_frames: 512,
                ..Default::default()
            },
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        assert_eq!(
            index.valid_samples(),
            u64::try_from(fixture.samples()).unwrap()
        );
        let prepared =
            PreparedSource::with_layout(session, &index, fixture.layout(), &cancelled).unwrap();
        Self { prepared, calls: 0 }
    }
}
impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        requested: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project.as_str(), "root-trim-pcm");
        assert_eq!(revision.as_str(), "r0");
        assert_eq!(requested, &asset());
        self.calls += 1;
        Ok(&self.prepared)
    }
}

fn event(fixture: Fixture, selection: ExactFrameRange, offset: i64) -> SoundEvent {
    let source = fixture.source();
    SoundEvent {
        owner: node("root"),
        label: "Retained sound".into(),
        mapping: SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: SourceAudioMapping::natural_rate(source.span, rate())
                .unwrap()
                .duration_frames(FrameDuration::ZERO)
                .unwrap(),
            selection,
        },
        source,
        offset: AudioSample(offset),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }
}
fn selected(a: ExactRatio, b: ExactRatio) -> ExactFrameRange {
    ExactFrameRange::new(a, b).unwrap()
}
fn full_event(fixture: Fixture) -> SoundEvent {
    let end = match fixture {
        Fixture::Stereo48 => mix_frames(8197, 1),
        // Root recipe is 6 frames. Offset7 moves this endpoint exactly to6;
        // source filter support is [0,ceil(9602.6*147/160)) = [0,8823).
        Fixture::Mono441 => ExactRatio::integer(6)
            .checked_sub(mix_frames(7, 1))
            .unwrap(),
    };
    event(fixture, selected(ExactRatio::ZERO, end), 7)
}
fn edit(operation: RootSoundOperation, automatic: bool) -> RootSoundEdit {
    let policy = if automatic {
        AudioEdgePolicy::Automatic
    } else {
        AudioEdgePolicy::Hard
    };
    RootSoundEdit {
        grid: RootSoundGrid::root(rate()),
        operation,
        cuts: RootSoundCutEdges {
            before: policy,
            after: policy,
        },
    }
}
fn trim(i: i64, o: i64, end: i64, automatic: bool) -> RootSoundEdit {
    edit(
        RootSoundOperation::Trim {
            range: range(1, end),
            in_frames: i,
            out_frames: o,
        },
        automatic,
    )
}
fn route(extent: i64, edits: Vec<RootSoundEdit>) -> RootSoundRoute {
    RootSoundRoute {
        recipe_extent: frames(extent),
        recipe_grid: RootSoundGrid::root(rate()),
        edits,
    }
}
fn document(
    output_frames: i64,
    fixture: Fixture,
    sound: SoundEvent,
    journal: Option<RootSoundRoute>,
) -> Result<ProjectDocument, DocumentError> {
    let empty = ProjectDocument::new(
        ProjectId::new("root-trim-pcm").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let picture_time_base = SourceTimeBase::new(1, 30).unwrap();
    let picture_span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: picture_time_base,
        },
        SourceTimestamp {
            ticks: 30,
            time_base: picture_time_base,
        },
    )
    .unwrap();
    let picture = BeatNode {
        label: "Picture clock without an audio gate".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(output_frames),
                edit_window: None,
                video: SourceVideo::Stream {
                    asset: AssetId::new("picture").unwrap(),
                    span: picture_span,
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![node("picture")])).unwrap();
    wire["nodes"]["picture"] = serde_json::to_value(picture).unwrap();
    wire["assets"]["picture"] = serde_json::to_value(AssetRecord {
        label: "Picture without audio".into(),
        content_hash: "c".repeat(64),
        video: Some(picture_span),
        audio: None,
        still_image: false,
        frame_count: Some(frames(30)),
        source_qualification: None,
    })
    .unwrap();
    wire["assets"]["media"] = serde_json::to_value(AssetRecord {
        label: fixture.file().into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(fixture.source().span),
        still_image: false,
        frame_count: None,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    })
    .unwrap();
    wire["sounds"]["effect"] = serde_json::to_value(sound).unwrap();
    if let Some(journal) = journal {
        wire["sound_routes"]["effect"] = serde_json::to_value(journal).unwrap();
    }
    ProjectDocument::from_json(&wire.to_string())
}

// Shuffled cold reads, two uneven output chunkings. No produced plan coordinates
// participate in expected sample labels or source filter bounds.
fn read(
    doc: &ProjectDocument,
    provider: &mut Provider,
    interval: Range<i64>,
    chunk: usize,
    bus: bool,
) -> Vec<[f32; 2]> {
    assert!((1..=256).contains(&chunk));
    let plan = Arc::new(RenderPlan::compile(doc).unwrap());
    let mut reader = StageAudio::new(Arc::clone(&plan));
    let voice = plan.root_sound(&sound_id()).unwrap();
    let count = usize::try_from(interval.end - interval.start).unwrap();
    let mut result = vec![[0.; 2]; count];
    let mut offsets: Vec<_> = (0..count).step_by(chunk).collect();
    offsets.reverse();
    if offsets.len() > 2 {
        offsets.swap(1, 2);
    }
    for offset in offsets {
        let n = (count - offset).min(chunk);
        let start = AudioSample(interval.start + i64::try_from(offset).unwrap());
        let samples = if bus {
            reader
                .prepare_authored_bus(
                    provider,
                    start,
                    u32::try_from(n).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        } else {
            reader
                .read_routed_root(
                    provider,
                    voice.routed_input().unwrap(),
                    start,
                    u32::try_from(n).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        };
        result[offset..offset + n].copy_from_slice(&samples);
    }
    result
}
#[track_caller]
fn exact(actual: &[[f32; 2]], expected: &[[f32; 2]], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    if let Some((at, (a, b))) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("{label}: first mismatch at {at}: actual {a:?}, expected {b:?}");
    }
}
fn oracle(
    provider: &Provider,
    fixture: Fixture,
    support: Range<i64>,
    phase: ExactRatio,
    count: usize,
) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        support,
        phase,
        AudioSample(0),
        fixture.step(),
        AudioSample(0)..AudioSample(i64::try_from(count).unwrap()),
    )
    .unwrap();
    let mut result = Vec::with_capacity(count);
    for at in (0..count).step_by(251) {
        result.extend(
            provider
                .prepared
                .prepare(
                    recipe.clone(),
                    AudioSample(i64::try_from(at).unwrap()),
                    u32::try_from((count - at).min(251)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    result
}
// Independent repository WAV recipe from generate_audio_fixtures.py.
fn stereo_sample(index: i64) -> [f32; 2] {
    let left = match index % 2048 {
        0 => 24576,
        1 => -24576,
        512..=1023 => ((index * 97) % 16384) - 8192,
        _ => 0,
    };
    let right = match index % 257 {
        0 => -32768,
        1 => 32767,
        _ => (index % 97) * 3 - 144,
    };
    [left as f32 / 32768., right as f32 / 32768.]
}
fn ramp(distance: i64) -> f32 {
    ((2 * distance + 1) as f64 / 192.).clamp(0., 1.) as f32
}

#[test]
fn in_only_merges_nonzero_translation_before_phase_and_fades_at_both_source_rates() {
    for fixture in [Fixture::Stereo48, Fixture::Mono441] {
        let doc = document(
            5,
            fixture,
            full_event(fixture),
            Some(route(6, vec![trim(1, 0, 3, true)])),
        )
        .unwrap();
        let immutable = doc.clone();
        let mut provider = Provider::new(fixture);
        let (support, phase, wrong) = match fixture {
            Fixture::Stereo48 => (
                0..8197,
                ExactRatio::integer(4797),
                ExactRatio::integer(4798),
            ),
            Fixture::Mono441 => (0..8823, ratio(705159, 160), ratio(705306, 160)),
        };
        // Keep[2,6)->[1,5): 3203 + (3203-1602) = old4804.
        // Independent offset7 gives source4797. Artificial U=3 must not restart.
        let expected = oracle(&provider, fixture, support.clone(), phase, 640);
        assert_ne!(expected, oracle(&provider, fixture, support, wrong, 640));
        for chunk in [193, 239] {
            exact(
                &read(&doc, &mut provider, 3203..3843, chunk, false),
                &expected,
                "In-only raw phase",
            );
            exact(
                &read(&doc, &mut provider, 3203..3843, chunk, true),
                &expected,
                "no artificial U fade",
            );
        }
        assert_eq!(doc, immutable);
    }
}

#[test]
fn complete_equal_in_out_keeps_terminal_sample_lost_by_sequential_editing() {
    let fixture = Fixture::Mono441;
    let sound = full_event(fixture);
    let direct = document(
        6,
        fixture,
        sound.clone(),
        Some(route(6, vec![trim(1, 1, 3, false)])),
    )
    .unwrap();
    let sequential = document(
        6,
        fixture,
        sound,
        Some(route(
            6,
            vec![
                edit(RootSoundOperation::Delete { range: range(1, 2) }, false),
                edit(
                    RootSoundOperation::Insert {
                        at: ProjectFrame(2),
                        duration: frames(1),
                    },
                    false,
                ),
            ],
        )),
    )
    .unwrap();
    let mut provider = Provider::new(fixture);
    // Direct suffix translation is zero. Filter support remains the original
    // exact selection ceil(9602.6*147/160)=8823, not a final-route crop.
    let expected = oracle(&provider, fixture, 0..8823, ratio(1374009, 160), 256); // (9354-7)*147/160
    // Delete translates by-1601 samples; the later Insert translates by+1602.
    // Thus the sequential suffix reads one earlier sample, losing old9609.
    let shifted = oracle(&provider, fixture, 0..8823, ratio(1373862, 160), 256);
    assert_ne!(expected, shifted);
    for chunk in [193, 239] {
        let actual = read(&direct, &mut provider, 9354..9610, chunk, false);
        exact(&actual, &expected, "direct terminal support");
        let sequential_pcm = read(&sequential, &mut provider, 9354..9610, chunk, false);
        exact(
            &sequential_pcm,
            &shifted,
            "sequential suffix loses old terminal label",
        );
        assert_ne!(actual[255], sequential_pcm[255]);
        exact(
            &read(&direct, &mut provider, 9354..9610, chunk, true),
            &expected,
            "Hard terminal bus",
        );
    }
    // A sound owning only the old terminal sample exposes genuine exhaustion:
    // unlike the full sound above, it cannot substitute old9608 after deletion.
    let terminal = event(
        fixture,
        selected(mix_frames(96016, 10), mix_frames(96026, 10)),
        7,
    );
    let retained = document(
        6,
        fixture,
        terminal.clone(),
        direct.sound_routes().get(&sound_id()).cloned(),
    )
    .unwrap();
    let last = oracle(&provider, fixture, 8822..8823, ratio(705747, 80), 1);
    assert_ne!(last, vec![[0.; 2]]);
    exact(
        &read(&retained, &mut provider, 9609..9610, 193, false),
        &last,
        "sole old terminal sample survives direct map",
    );
    let error = document(
        6,
        fixture,
        terminal,
        sequential.sound_routes().get(&sound_id()).cloned(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no surviving selected support"));
}

#[test]
fn both_true_cuts_use_sample_centered_ramps_and_keep_outer_hard_policy() {
    let fixture = Fixture::Stereo48;
    let mut provider = Provider::new(fixture);
    for automatic in [false, true] {
        let doc = document(
            4,
            fixture,
            full_event(fixture),
            Some(route(6, vec![trim(1, -1, 4, automatic)])),
        )
        .unwrap();
        for interval in [7..263, 1502..1758, 3103..3359, 4745..5001] {
            let expected: Vec<_> = interval
                .clone()
                .map(|at| {
                    // Keeps old[0,1),[2,3),[4,6), output[0,1),[1,2),[2,4).
                    let (old, gain) = if at < 1602 {
                        (at, if automatic { ramp(1601 - at) } else { 1. })
                    } else if at < 3203 {
                        // Transported end label is B(3)-1601=3204, although the
                        // destination allocation ends at B(2)=3203. The old label
                        // still owns envelope progress; its final audible gain3/192.
                        (
                            at + 1601,
                            if automatic {
                                ramp(at - 1602).min(ramp(3203 - at))
                            } else {
                                1.
                            },
                        )
                    } else {
                        (at + 3203, if automatic { ramp(at - 3203) } else { 1. })
                    };
                    stereo_sample(old - 7).map(|x| x * gain)
                })
                .collect();
            for chunk in [193, 239] {
                exact(
                    &read(&doc, &mut provider, interval.clone(), chunk, true),
                    &expected,
                    "two cut policies",
                );
            }
        }
    }
}

#[test]
fn only_exact_hard_coincidence_suppresses_a_new_cut_ramp() {
    let fixture = Fixture::Stereo48;
    let mut provider = Provider::new(fixture);
    // After offset7, the first authored start is exactly frame2 (3203.2).
    // The second is3203.1: the same rounded label but a distinct earlier edge.
    for (start, coincident) in [(31962, true), (31961, false)] {
        let sound = event(
            fixture,
            selected(mix_frames(start, 10), mix_frames(8197, 1)),
            7,
        );
        let doc = document(5, fixture, sound, Some(route(6, vec![trim(1, 0, 3, true)]))).unwrap();
        let raw: Vec<_> = (0..64)
            .map(|n| {
                if n == 0 {
                    [0.; 2]
                } else {
                    stereo_sample(3196 + n)
                }
            })
            .collect();
        // Both exact selections admit source integers[3197,8197). The mapping
        // remains at3196 on the first allocated output sample, which is zero.
        let expected: Vec<_> = raw
            .iter()
            .enumerate()
            .map(|(n, frame)| {
                frame.map(|sample| {
                    sample
                        * if coincident {
                            1.
                        } else {
                            ramp(i64::try_from(n).unwrap())
                        }
                })
            })
            .collect();
        if !coincident {
            assert_ne!(expected, raw);
        }
        for chunk in [193, 239] {
            exact(
                &read(&doc, &mut provider, 1602..1666, chunk, false),
                &raw,
                "same raw filter support",
            );
            exact(
                &read(&doc, &mut provider, 1602..1666, chunk, true),
                &expected,
                "exact Hard versus rounded coincidence",
            );
        }
    }
}

#[test]
fn prior_route_and_two_new_gaps_preserve_complete_filter_context() {
    let fixture = Fixture::Mono441;
    let sound = event(fixture, selected(ExactRatio::ZERO, mix_frames(7900, 1)), 7);
    let previous = edit(
        RootSoundOperation::Insert {
            at: ProjectFrame(1),
            duration: frames(1),
        },
        false,
    );
    let before = document(6, fixture, sound.clone(), Some(route(5, vec![previous]))).unwrap();
    let after = document(
        8,
        fixture,
        sound,
        Some(route(5, vec![previous, trim(-1, 1, 3, false)])),
    )
    .unwrap();
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(
        &after.sound_routes()[&sound_id()].edits[..1],
        before.sound_routes()[&sound_id()].edits.as_slice()
    );
    assert_eq!(
        after.sound_routes()[&sound_id()].recipe_grid,
        before.sound_routes()[&sound_id()].recipe_grid
    );
    assert_eq!(after.sound_routes()[&sound_id()].recipe_extent, frames(5));
    let mut provider = Provider::new(fixture);
    for (at, phase) in [
        (100, ratio(13671, 160)),
        (5100, ratio(277977, 160)),
        (8500, ratio(542283, 160)),
    ] {
        // Original labels are100,1898,3696; offset7 gives93,1891,3689.
        // Selection ends at mix7900, hence filter end ceil(7258.125)=7259.
        let expected = oracle(&provider, fixture, 0..7259, phase, 640);
        for chunk in [193, 239] {
            exact(
                &read(&after, &mut provider, at..at + 640, chunk, false),
                &expected,
                "prior chronological route",
            );
        }
    }
    // The shifted *old* gap ends at3203+1601=4804, one sample before B(3).
    // Its first sample reads old1602, phase(1602-7)*147/160=46893/32.
    // A route cut cannot clip the sinc halo toceil(1465.40625)=1466.
    let entering = oracle(&provider, fixture, 0..7259, ratio(46893, 32), 128);
    assert_ne!(
        entering,
        oracle(&provider, fixture, 1466..7259, ratio(46893, 32), 128)
    );
    for chunk in [193, 239] {
        exact(
            &read(&after, &mut provider, 4804..4932, chunk, false),
            &entering,
            "full context through retained gap",
        );
        exact(
            &read(&after, &mut provider, 4804..4932, chunk, true),
            &entering,
            "retained Hard gap boundary",
        );
        assert_eq!(
            read(&after, &mut provider, 4803..4804, chunk, false),
            vec![[0.; 2]]
        );
        for gap in [1602..2242, 3500..4140, 6406..7046] {
            assert_eq!(
                read(&after, &mut provider, gap.clone(), chunk, false),
                vec![[0.; 2]; 640]
            );
            assert_eq!(
                read(&after, &mut provider, gap, chunk, true),
                vec![[0.; 2]; 640]
            );
        }
    }
}

#[test]
fn zero_trim_projection_is_identity_and_does_not_reset_existing_pcm() {
    let fixture = Fixture::Stereo48;
    let zero = trim(0, 0, 3, true);
    assert!(zero.operation.projection(6).unwrap().is_identity());
    let direct = document(6, fixture, full_event(fixture), None).unwrap();
    assert!(direct.sound_routes().is_empty());
    // This deliberately serialized identity is a renderer stress case. The
    // private core capture must avoid authoring it; core tests own that claim.
    let explicit = document(6, fixture, full_event(fixture), Some(route(6, vec![zero]))).unwrap();
    let mut provider = Provider::new(fixture);
    let expected: Vec<_> = (3000..3640).map(|at| stereo_sample(at - 7)).collect();
    for chunk in [193, 239] {
        exact(
            &read(&direct, &mut provider, 3000..3640, chunk, true),
            &expected,
            "identity direct bus",
        );
        exact(
            &read(&explicit, &mut provider, 3000..3640, chunk, true),
            &expected,
            "identity route bus",
        );
    }
    assert!(direct.sound_routes().is_empty());
    let prior = edit(RootSoundOperation::Delete { range: range(1, 2) }, false);
    let old = document(5, fixture, full_event(fixture), Some(route(6, vec![prior]))).unwrap();
    let repeated_identity = document(
        5,
        fixture,
        full_event(fixture),
        Some(route(6, vec![prior, zero])),
    )
    .unwrap();
    let expected: Vec<_> = (3203..3843)
        .map(|at| stereo_sample(at + 1601 - 7))
        .collect();
    for chunk in [193, 239] {
        exact(
            &read(&old, &mut provider, 3203..3843, chunk, true),
            &expected,
            "retained route before zero",
        );
        exact(
            &read(&repeated_identity, &mut provider, 3203..3843, chunk, true),
            &expected,
            "zero cannot reset prior route",
        );
    }
}

#[test]
fn tiny_fractional_selection_keeps_physical_samples_and_shortened_ramps() {
    let fixture = Fixture::Stereo48;
    let mut provider = Provider::new(fixture);
    for (end, count, gain) in [
        (ratio(48066, 10), 1usize, 1.0_f32),
        (ratio(48076, 10), 2, 0.5),
    ] {
        let mut sound = event(
            fixture,
            selected(
                mix_frames(48056, 10),
                end.checked_mul(ratio(5, 8008)).unwrap(),
            ),
            0,
        );
        sound.start_edge = AudioEdgePolicy::Automatic;
        sound.end_edge = AudioEdgePolicy::Automatic;
        let doc = document(5, fixture, sound, Some(route(6, vec![trim(1, 0, 3, true)]))).unwrap();
        // B(4805.6)=4806. One Keep transports integral labels by-1601,
        // retaining sample3205 even though the moved exact start is3204.0.
        let mut raw = vec![[0.; 2]; 8];
        let mut faded = raw.clone();
        for index in 0..count {
            raw[3 + index] = stereo_sample(4806 + i64::try_from(index).unwrap());
            faded[3 + index] = raw[3 + index].map(|value| value * gain);
        }
        for chunk in [193, 239] {
            exact(
                &read(&doc, &mut provider, 3202..3210, chunk, false),
                &raw,
                "tiny retained samples",
            );
            exact(
                &read(&doc, &mut provider, 3202..3210, chunk, true),
                &faded,
                "one sample unity, two half",
            );
        }
    }
}

#[test]
fn initially_sampleless_intent_survives_without_inventing_pcm_but_exhausted_support_rejects() {
    let fixture = Fixture::Stereo48;
    // Old B(4806.1)=B(4806.4)=4806. Moving the logical interval by-1f
    // would round [3204.5,3204.8) to one sample; it cannot invent physical PCM.
    let sampleless = event(
        fixture,
        selected(mix_frames(48061, 10), mix_frames(48064, 10)),
        0,
    );
    let doc = document(
        5,
        fixture,
        sampleless,
        Some(route(6, vec![trim(1, 0, 3, true)])),
    )
    .unwrap();
    assert!(doc.sounds().contains_key(&sound_id()));
    let mut provider = Provider::new(fixture);
    for chunk in [193, 239] {
        assert_eq!(
            read(&doc, &mut provider, 3000..3640, chunk, false),
            vec![[0.; 2]; 640]
        );
        assert_eq!(
            read(&doc, &mut provider, 3000..3640, chunk, true),
            vec![[0.; 2]; 640]
        );
    }
    let removed = event(
        fixture,
        selected(ExactRatio::ONE, ExactRatio::integer(2)),
        0,
    );
    let error = document(
        5,
        fixture,
        removed,
        Some(route(6, vec![trim(1, 0, 3, true)])),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no surviving selected support"));
}
