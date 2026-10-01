//! Dormant audio retains its source clock and linked intent without reading PCM.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PcmWindow, PreparationError, PreparedSource, ResampleRecipe, Resampler,
    SequenceAudio, StageAudio, StereoMatrix,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset() -> AssetId {
    AssetId::new("media").unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn document(
    rate: FrameRate,
    beat_frames: i64,
    start: ExactRatio,
    selected: ExactFrameRange,
    offset: i64,
) -> ProjectDocument {
    let audio = SourceAudio {
        asset: asset(),
        span: span(100, 2100),
    };
    let mapping_frames = SourceAudioMapping::natural_rate(audio.span, rate)
        .unwrap()
        .duration_frames(duration(beat_frames))
        .unwrap();
    let source = BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        label: "Linked Original with dormant audio".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: duration(beat_frames),
                video: SourceVideo::Stream {
                    asset: asset(),
                    span: span(0, 8197),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio),
                audio_mapping: SourceAudioMapping::SelectedPlacement {
                    start,
                    frames: mapping_frames,
                    selection: selected,
                },
                audio_offset: AudioSample(offset),
                link: LinkRelation::Linked,
            },
        },
    };
    let empty = ProjectDocument::new(
        ProjectId::new("dormant-source").unwrap(),
        revision("dormant"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("source")])),
        (id("source"), source),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset(),
        AssetRecord {
            label: "Known stereo PCM fixture".into(),
            content_hash: "a".repeat(64),
            video: Some(span(0, 8197)),
            audio: Some(span(0, 8197)),
            still_image: false,
            frame_count: Some(duration(3)),
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn source(document: &ProjectDocument) -> &SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&id("source")].kind else {
        panic!("source fixture")
    };
    source
}

fn bound(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            document,
            AudioTimingId {
                allocation: revision("retained-source-clock"),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[derive(Default)]
struct NoReads {
    calls: usize,
}

impl AudioSourceProvider for NoReads {
    fn source(
        &mut self,
        _project: &ProjectId,
        _revision: &RevisionId,
        _asset: &AssetId,
        _cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.calls += 1;
        Err(PreparationError::SourceUnavailable(
            "Dormant audio must not request media".into(),
        ))
    }
}

fn assert_silence(document: &ProjectDocument) {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let total = plan.audio_duration().unwrap().0;
    let mut provider = NoReads::default();
    let direct = SequenceAudio::new(Arc::clone(&plan));
    let mut staged = StageAudio::new(Arc::clone(&plan));
    let mut at = 0;
    let mut chunk = 0;
    while at < total {
        let count = u32::try_from((total - at).min([199, 1, 127][chunk % 3])).unwrap();
        if !plan.has_audio_bindings() {
            let block = direct
                .read_sources(
                    &mut provider,
                    AudioSample(at),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(block.samples.len(), usize::try_from(count).unwrap());
            assert!(
                block
                    .samples
                    .iter()
                    .flatten()
                    .all(|sample| sample.to_bits() == 0)
            );
        }
        let block = staged
            .read(
                &mut provider,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.start, AudioSample(at));
        assert_eq!(&block.revision_id, document.revision_id());
        assert_eq!(block.samples.len(), usize::try_from(count).unwrap());
        assert!(
            block
                .samples
                .iter()
                .flatten()
                .all(|sample| sample.to_bits() == 0)
        );
        let faded = staged
            .read_edge_faded(
                &mut provider,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(
            faded
                .samples
                .iter()
                .flatten()
                .all(|sample| sample.to_bits() == 0)
        );
        at += i64::from(count);
        chunk += 1;
    }
    assert_eq!(
        provider.calls, 0,
        "An empty selection grants no media reads"
    );
}

#[test]
fn explicit_empty_support_is_digital_silence_without_media_for_live_and_retained_clocks() {
    for (rate, beat_frames, start, offset, points) in [
        (
            FrameRate::new(48_000, 1).unwrap(),
            400,
            ExactRatio::ZERO,
            0,
            [
                ExactRatio::ZERO,
                ExactRatio::integer(100),
                ExactRatio::integer(2000),
            ],
        ),
        (
            FrameRate::new(30_000, 1001).unwrap(),
            3,
            ratio(-1, 7),
            3,
            [
                ratio(-1, 7),
                ratio(1, 3),
                ratio(-1, 7).checked_add(ratio(1250, 1001)).unwrap(),
            ],
        ),
    ] {
        for point in points {
            let original = document(
                rate,
                beat_frames,
                start,
                ExactFrameRange {
                    start: point,
                    end: point,
                },
                offset,
            );
            assert_eq!(source(&original).link, LinkRelation::Linked);
            assert_eq!(
                source(&original).audio.as_ref().unwrap().span,
                span(100, 2100)
            );
            for candidate in [original.clone(), bound(&original)] {
                assert_eq!(source(&candidate), source(&original));
                assert_silence(&candidate);
            }
        }
    }
}

struct FixtureProvider {
    source: PreparedSource,
    revisions: BTreeSet<RevisionId>,
    calls: usize,
}

impl FixtureProvider {
    fn new(revisions: BTreeSet<RevisionId>) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
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
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let measured = session.index().clone();
        let source =
            PreparedSource::with_layout(session, &measured, stereo_layout(), &cancelled).unwrap();
        Self {
            source,
            revisions,
            calls: 0,
        }
    }
}

impl AudioSourceProvider for FixtureProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        requested_asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project, &ProjectId::new("dormant-source").unwrap());
        assert!(self.revisions.contains(revision));
        assert_eq!(requested_asset, &asset());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        Ok(&self.source)
    }
}

fn stereo_layout() -> AudioChannelLayout {
    // This generated WAV has no speaker mask. Its fixture recipe declares L/R.
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
        512..=1023 => i32::try_from((index * 97) % 16384).unwrap() - 8192,
        _ => 0,
    };
    let right = match index % 257 {
        0 => -32768,
        1 => 32767,
        _ => i32::try_from(index % 97).unwrap() * 3 - 48 * 3,
    };
    [left as f32 / 32768.0, right as f32 / 32768.0]
}

fn expected_fractional_pcm(total: i64) -> Vec<[f32; 2]> {
    // Independent arithmetic, without plan-derived support or sample maps:
    // 1601.6 samples/frame, offset 3, selected [1/5,3/5) -> [323,964).
    // Source context starts at sample 100 and frame -1/7. Its selected sample
    // support is [ceil(649.12),ceil(1289.76)) = [650,1290), never a refitted span.
    let recipe = ResampleRecipe::new(
        650..1290,
        ratio(3244, 5),
        AudioSample(323),
        ExactRatio::ONE,
        AudioSample(323)..AudioSample(964),
    )
    .unwrap();
    let sampler = Resampler::new(recipe, StereoMatrix::new(stereo_layout()).unwrap());
    assert!(total >= 964);
    let mut output = vec![[0.0; 2]; usize::try_from(total).unwrap()];
    let mut at = 323;
    while at < 964 {
        let count = u32::try_from((964 - at).min(256)).unwrap();
        let window = sampler
            .required_source_range(AudioSample(at), count)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: range.flat_map(fixture_sample).collect(),
            });
        let block = sampler
            .render(AudioSample(at), count, window, &AtomicBool::new(false))
            .unwrap();
        let start = usize::try_from(at).unwrap();
        output[start..start + usize::try_from(count).unwrap()].copy_from_slice(&block.samples);
        at += i64::from(count);
    }
    output
}

#[test]
fn growing_support_reveals_canonical_pcm_without_reclocking_fractional_source_mapping() {
    let original = document(
        FrameRate::new(30_000, 1001).unwrap(),
        3,
        ratio(-1, 7),
        ExactFrameRange {
            start: ratio(1, 3),
            end: ratio(1, 3),
        },
        3,
    );
    for before in [original.clone(), bound(&original)] {
        assert_silence(&before);
        let mapping = SourceAudioMapping::SelectedPlacement {
            start: ratio(-1, 7),
            frames: ratio(1250, 1001),
            selection: ExactFrameRange::new(ratio(1, 5), ratio(3, 5)).unwrap(),
        };
        let transaction = apply(
            &before,
            &CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision("grown"),
                command: Command::SetSourceAudioMapping {
                    node: id("source"),
                    mapping,
                    offset: AudioSample(3),
                },
            },
        )
        .unwrap();
        let after = transaction.forward.apply(&before).unwrap();
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
        assert_eq!(transaction.duration_delta, 0);
        let old = source(&before);
        let grown = source(&after);
        assert_eq!(grown.audio, old.audio);
        assert_eq!(grown.video, old.video);
        assert_eq!(grown.video_mapping, old.video_mapping);
        assert_eq!(grown.audio_offset, old.audio_offset);
        assert_eq!(grown.link, LinkRelation::Linked);
        assert_eq!(
            grown.audio_mapping.start_frames(),
            old.audio_mapping.start_frames()
        );
        assert_eq!(
            grown.audio_mapping.duration_frames(grown.duration).unwrap(),
            old.audio_mapping.duration_frames(old.duration).unwrap()
        );
        assert_eq!(before.audio_bindings(), after.audio_bindings());
        let plan = Arc::new(RenderPlan::compile(&after).unwrap());
        assert_eq!(plan.audio_duration().unwrap(), AudioSample(4805));
        let expected = expected_fractional_pcm(4805);
        assert!(
            expected[..323]
                .iter()
                .chain(&expected[964..])
                .flatten()
                .all(|value| value.to_bits() == 0)
        );
        assert!(expected[323..964].iter().any(|value| *value != [0.; 2]));
        let mut provider = FixtureProvider::new(BTreeSet::from([after.revision_id().clone()]));
        for chunks in [[199, 1, 127], [7, 251, 3]] {
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let mut actual = Vec::new();
            for requested in chunks.into_iter().cycle() {
                if actual.len() == expected.len() {
                    break;
                }
                let count = requested.min(u32::try_from(expected.len() - actual.len()).unwrap());
                let start = AudioSample(i64::try_from(actual.len()).unwrap());
                let block = renderer
                    .read(
                        &mut provider,
                        start,
                        count,
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert_eq!(block.start, start);
                assert_eq!(&block.revision_id, after.revision_id());
                actual.extend(block.samples);
            }
            assert_eq!(actual, expected);
        }
        assert!(
            provider.calls > 0,
            "Nonempty support must resolve its qualified source"
        );
        // Undo restores dormant support and revokes the need for any PCM reader.
        assert_silence(&transaction.inverse.apply(&after).unwrap());
    }
}

#[test]
fn intentionally_absent_audio_stays_absent_and_cannot_be_created_by_growing_support() {
    let dormant = document(
        FrameRate::new(48_000, 1).unwrap(),
        400,
        ExactRatio::ZERO,
        ExactFrameRange {
            start: ExactRatio::integer(100),
            end: ExactRatio::integer(100),
        },
        0,
    );
    let mut wire = serde_json::to_value(&dormant).unwrap();
    wire["nodes"]["source"]["kind"]["source"]["audio"] = serde_json::Value::Null;
    wire["nodes"]["source"]["kind"]["source"]["audio_mapping"] =
        serde_json::to_value(SourceAudioMapping::FitBeat).unwrap();
    wire["nodes"]["source"]["kind"]["source"]["link"] =
        serde_json::to_value(LinkRelation::Independent).unwrap();
    let absent = ProjectDocument::from_json(&wire.to_string()).unwrap();
    for before in [absent.clone(), bound(&absent)] {
        assert!(source(&before).audio.is_none());
        assert_silence(&before);
        let rejected = apply(
            &before,
            &CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision("must-not-invent-audio"),
                command: Command::SetSourceAudioMapping {
                    node: id("source"),
                    mapping: SourceAudioMapping::SelectedPlacement {
                        start: ExactRatio::ZERO,
                        frames: ExactRatio::integer(2000),
                        selection: ExactFrameRange::new(
                            ExactRatio::integer(100),
                            ExactRatio::integer(200),
                        )
                        .unwrap(),
                    },
                    offset: AudioSample(0),
                },
            },
        );
        assert_eq!(
            rejected.unwrap_err().code,
            EditErrorCode::SourceRangeInvalid
        );
        assert!(source(&before).audio.is_none());
        assert_silence(&before);
    }
}

#[path = "dormant_source/edit_window.rs"]
mod edit_window;
