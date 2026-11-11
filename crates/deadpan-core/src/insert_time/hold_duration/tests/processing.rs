//! Hold edits preserve physical processing-input clocks, not old DSP output.
use super::*;

#[path = "processing/output_owned.rs"]
mod output_owned;

fn rate(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    let mut node = BeatNode::sequence("Stage", vec![]);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(length),
        mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    node
}

fn processing(pitch: PitchPolicy, crop: i64, affine: bool) -> ProjectDocument {
    let mut document = fixture(0, false);
    document.nodes.insert(id("before"), source(3));
    document.nodes.insert(id("after"), source(4));
    let child = if affine {
        document.nodes.insert(
            id("affine"),
            rate("body", 6, 0, 8, PitchPolicy::FollowSpeed),
        );
        "affine"
    } else {
        "body"
    };
    document.nodes.insert(
        id("processing"),
        rate(child, 4, crop, if affine { 6 } else { 8 }, pitch),
    );
    document.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("processing"), id("tail")]),
    );
    document.validate().unwrap();
    document
}

fn instance(owner: &str, repeats: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(owner),
        repeats: repeats
            .iter()
            .map(|(owner, ordinal)| RepeatInstance {
                node: id(owner),
                iteration: IterationId {
                    allocation: rev(owner),
                    ordinal: *ordinal,
                },
            })
            .collect(),
    }
}

fn captured(document: &ProjectDocument, allocation: &str) -> AudioBindingState {
    capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: rev(allocation),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn attach_effect(document: &mut ProjectDocument, owner: &str) -> SoundId {
    let effect = SoundId::new("effect").unwrap();
    let source = SourceAudio {
        asset: audio().asset,
        span: span(3100, 3200),
    };
    document.beat_sounds.insert(
        id(owner),
        std::collections::BTreeMap::from([(
            effect.clone(),
            BeatSound {
                label: "Effect".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                source,
                offset: AudioSample(37),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    );
    document.validate().unwrap();
    effect
}

#[test]
fn ordinary_hold_duration_processing_changed_attachment_scope_refuses_atomically() {
    for historical in [false, true] {
        let mut before = processing(PitchPolicy::Preserve, 1, false);
        if historical {
            before.audio_bindings = captured(&before, "attached-input");
        }
        attach_effect(&mut before, "after");
        let snapshot = before.clone();
        let json = before.to_json().unwrap();
        let error = apply(&before, &request(&before, "hold", 3)).unwrap_err();
        // This descendant's sound enters the changed processing input. The
        // output owner's narrower proof must never admit this dependency.
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("carries an attached sound"));
        assert_eq!(before, snapshot);
        assert_eq!(before.to_json().unwrap(), json);
        assert_eq!(before.audio_bindings, snapshot.audio_bindings);
        assert_eq!(before.beat_sounds, snapshot.beat_sounds);
    }
}
