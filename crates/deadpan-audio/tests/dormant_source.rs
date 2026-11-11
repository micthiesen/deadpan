//! Dormant audio retains its source clock and linked intent without reading PCM.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PreparationError, PreparedSource, SequenceAudio, StageAudio,
};
use deadpan_core::*;
use deadpan_plan::RenderPlan;

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
        audio_editorial_edges: Default::default(),
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
        cutaways: Vec::new(),
        captions: Vec::new(),
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
