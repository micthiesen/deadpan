use super::*;

use deadpan_core::{
    AssetRecord, Command, CommandRequest, FrameDuration, LinkRelation, NodeId, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId, SourceAudioMapping, SourceEditWindow, SourceNode,
    SourceVideoMapping, apply,
};
use deadpan_media::source_import_timing::derive_source_moment;
use deadpan_plan::{AudioQueryLimits, RenderPlan};

#[test]
fn full_import_window_retains_exact_union_with_audio_lead_tail_and_signed_origin() {
    for shift in [-10, 0, 17] {
        let video = indexed_video(
            shift * 1000 - 5,
            &[11, 13, 17],
            SourceTimeBase::new(1, 1000).unwrap(),
        );
        let audio = indexed_audio(shift * 44_100 - 441, 44_100, &[2441]);
        let timing =
            derive_import_timing(Some(&video), Some(&audio), FrameRate::new(30, 1).unwrap())
                .unwrap();
        assert_eq!(
            timing.origin_seconds,
            ExactRatio::integer(shift)
                .checked_sub(ratio(1, 100))
                .unwrap()
        );
        let expected = SourceEditWindow::new(ExactRatio::ZERO, ratio(2441, 1470)).unwrap();
        assert_eq!(timing.edit_window, expected);
        assert_eq!(timing.duration.frames(), 2);
        let source = timing.source_node(AssetId::new("source").unwrap());
        assert_eq!(source.edit_window, Some(expected));
        assert_eq!(source.audio_offset, AudioSample(0));
        assert_eq!(source.link, LinkRelation::Linked);
        assert_eq!(
            source.video_mapping,
            SourceVideoMapping::Placement {
                start: ratio(3, 20),
                frames: ratio(123, 100),
                endpoints: EndpointPolicy::HoldAdjacent,
            }
        );
        assert_eq!(
            source.audio_mapping,
            SourceAudioMapping::Placement {
                start: ExactRatio::ZERO,
                frames: ratio(2441, 1470),
            }
        );
        let retained = source.audio.unwrap().span;
        assert_eq!(retained.start().ticks, shift * 44_100 - 441);
        assert_eq!(retained.end().ticks, shift * 44_100 + 2000);

        // A delayed stream ending before picture changes neither the exact
        // picture union nor the stream's independent affine placement.
        let delayed = indexed_audio(shift * 44_100 + 441, 44_100, &[1000]);
        let timing =
            derive_import_timing(Some(&video), Some(&delayed), FrameRate::new(30, 1).unwrap())
                .unwrap();
        assert_eq!(timing.edit_window.end(), ratio(123, 100));
        assert_eq!(timing.duration.frames(), 2);
        assert_eq!(timing.audio.unwrap().start_frames, ratio(9, 20));
        assert_eq!(timing.audio.unwrap().duration_frames, ratio(100, 147));
    }
}

#[test]
fn single_stream_imports_keep_their_exact_window_without_inventing_linkage() {
    let video = indexed_video(-5, &[11, 13, 17], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(-441, 44_100, &[2441]);
    let rate = FrameRate::new(30, 1).unwrap();
    for (timing, end, is_video) in [
        (
            derive_import_timing(Some(&video), None, rate).unwrap(),
            ratio(123, 100),
            true,
        ),
        (
            derive_import_timing(None, Some(&audio), rate).unwrap(),
            ratio(2441, 1470),
            false,
        ),
    ] {
        assert_eq!(timing.edit_window.start(), ExactRatio::ZERO);
        assert_eq!(timing.edit_window.end(), end);
        assert_eq!(timing.duration.frames(), 2);
        timing.edit_window.validate(timing.duration).unwrap();
        let source = timing.source_node(AssetId::new("source").unwrap());
        assert_eq!(source.edit_window, Some(timing.edit_window));
        assert_eq!(source.link, LinkRelation::Independent);
        assert_eq!(source.audio.is_none(), is_video);
        assert_eq!(timing.video.is_some(), is_video);
    }
}

fn source_view(
    source: SourceNode,
    video: &SourceIndexSnapshot,
    audio: Option<&AudioIndexSnapshot>,
    rate: FrameRate,
) -> ProjectDocument {
    let timing = derive_import_timing(Some(video), audio, rate).unwrap();
    let asset = AssetId::new("source").unwrap();
    let mut basis = audio_only_basis();
    basis.frame_rate = rate;
    let empty = ProjectDocument::new(
        ProjectId::new("source-edit-window").unwrap(),
        RevisionId::new("initial").unwrap(),
        basis,
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let edit = apply(
        &empty,
        &CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: RevisionId::new("asset").unwrap(),
            command: Command::AddAsset {
                id: asset.clone(),
                asset: AssetRecord {
                    source_qualification: None,
                    label: "Measured original".into(),
                    content_hash: "a".repeat(64),
                    video: timing.video.map(|placement| placement.span),
                    audio: timing.audio.map(|placement| placement.span),
                    still_image: false,
                    frame_count: Some(FrameDuration::new(3).unwrap()),
                },
            },
        },
    )
    .unwrap();
    edit.forward
        .apply(&empty)
        .unwrap()
        .source_view(
            &asset,
            source,
            NodeId::new("root").unwrap(),
            NodeId::new("clip").unwrap(),
        )
        .unwrap()
}

fn assert_same_plan_without_window(
    source: SourceNode,
    video: &SourceIndexSnapshot,
    audio: Option<&AudioIndexSnapshot>,
    rate: FrameRate,
    pictures: &[(i64, u64)],
) {
    assert!(source.edit_window.is_some());
    let mut generic = source.clone();
    generic.edit_window = None;
    let with = RenderPlan::compile(&source_view(source, video, audio, rate)).unwrap();
    let without = RenderPlan::compile(&source_view(generic, video, audio, rate)).unwrap();
    for &(frame, ordinal) in pictures {
        let picture = with.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture, without.picture(ProjectFrame(frame)).unwrap());
        assert_eq!(
            picture
                .picture
                .select_source_frame(video.index())
                .unwrap()
                .identity,
            SourceFrameId(ordinal)
        );
    }
    let end = with.audio_duration().unwrap();
    assert_eq!(end, without.audio_duration().unwrap());
    assert_eq!(
        with.audio(AudioSample(0)..end, AudioQueryLimits::default())
            .unwrap(),
        without
            .audio(AudioSample(0)..end, AudioQueryLimits::default())
            .unwrap()
    );
}

#[test]
fn edit_window_is_inert_for_held_picture_and_audible_dormant_or_absent_audio() {
    let video = indexed_video(0, &[1, 3, 2], SourceTimeBase::new(1, 1000).unwrap());
    let rate = FrameRate::new(300, 1).unwrap();
    let audio = indexed_audio(-441, 44_100, &[1340]);
    let full = derive_import_timing(Some(&video), Some(&audio), rate).unwrap();
    assert_eq!(full.edit_window.end(), ratio(1340, 147));
    assert_eq!(full.duration.frames(), 10);
    // The union starts at -10 ms. Frame centers 0.5, 3.5 and 4.5 at
    // 300 fps reach -8 1/3, 1 2/3 and 5 ms in the Original. They select
    // ordinals 0, 1 and 2 from intervals [0,1), [1,4), [4,6) ms.
    assert_same_plan_without_window(
        full.source_node(AssetId::new("source").unwrap()),
        &video,
        Some(&audio),
        rate,
        &[(0, 0), (3, 1), (4, 2), (5, 2), (9, 2)],
    );

    // All three choices have the same fractional 0..3/10 picture window.
    // The late audio begins in rounded slack and remains explicitly dormant.
    let late = indexed_audio(60, 44_100, &[200]);
    for audio in [Some(&audio), Some(&late), None] {
        let moment = derive_source_moment(&video, audio, 0..1, rate).unwrap();
        assert_eq!(moment.edit_window.end(), ratio(3, 10));
        assert_same_plan_without_window(
            moment.source_node(AssetId::new("source").unwrap()),
            &video,
            audio,
            rate,
            &[(0, 0)],
        );
    }
}
