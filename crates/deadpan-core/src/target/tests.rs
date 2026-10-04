use super::*;
use crate::{AssetRecord, SourceTimeBase};
use std::collections::BTreeMap;

fn base() -> SourceTimeBase {
    SourceTimeBase::new(1, 30).unwrap()
}
fn stamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: base(),
    }
}
fn region(x: u32) -> TargetRegion {
    TargetRegion {
        center: [x, 500_000],
        size: [100_000, 200_000],
    }
}
fn assets() -> BTreeMap<AssetId, AssetRecord> {
    BTreeMap::from([(
        AssetId::new("original").unwrap(),
        AssetRecord {
            source_qualification: None,
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(SourceSpan::new(stamp(0), stamp(300)).unwrap()),
            audio: None,
            still_image: false,
            frame_count: None,
        },
    )])
}
fn target() -> AttentionTarget {
    AttentionTarget {
        label: "Speaker".into(),
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(stamp(10), stamp(100)).unwrap(),
        region: region(100_000),
        samples: vec![
            TargetSample {
                at: 20,
                region: region(200_000),
                confidence: 900,
                state: TrackState::Tracked,
            },
            TargetSample {
                at: 40,
                region: region(200_000),
                confidence: 100,
                state: TrackState::Lost,
            },
        ],
        corrections: vec![TargetCorrection {
            at: 30,
            region: region(300_000),
        }],
    }
}
fn point(ticks: i64) -> SourcePoint {
    SourcePoint {
        ticks: ExactRatio::integer(ticks),
        time_base: base(),
    }
}

#[test]
fn regions_follow_samples_and_corrections_in_source_time() {
    let target = target();
    target.validate(&assets()).unwrap();
    assert_eq!(
        target.region_at(point(10)),
        Some((region(100_000), TargetSource::Initial))
    );
    assert_eq!(
        target.region_at(point(25)),
        Some((region(200_000), TargetSource::Tracked(TrackState::Tracked)))
    );
    assert_eq!(
        target.region_at(point(35)),
        Some((region(300_000), TargetSource::Manual)),
        "a correction overrides the samples before the next one"
    );
    assert_eq!(
        target.region_at(point(50)),
        Some((region(200_000), TargetSource::Tracked(TrackState::Lost)))
    );
    assert_eq!(target.region_at(point(100)), None, "half-open span");
    assert_eq!(target.region_at(point(5)), None);
    assert_eq!(
        target.correction_range(30),
        Some((stamp(30), stamp(100))),
        "a correction invalidates only its own range"
    );
}

#[test]
fn invalid_targets_are_refused() {
    let assets = assets();
    let mut unordered = target();
    unordered.samples.swap(0, 1);
    assert!(unordered.validate(&assets).is_err());
    let mut outside = target();
    outside.span = SourceSpan::new(stamp(10), stamp(400)).unwrap();
    assert!(outside.validate(&assets).is_err());
    let mut empty = target();
    empty.region.size = [0, 1];
    assert!(empty.validate(&assets).is_err());
    let mut missing = target();
    missing.asset = AssetId::new("other").unwrap();
    assert!(missing.validate(&assets).is_err());
    let json = serde_json::to_string(&target()).unwrap();
    assert_eq!(
        serde_json::from_str::<AttentionTarget>(&json).unwrap(),
        target()
    );
}

#[test]
fn set_and_delete_target_are_reversible_edits_without_picture_time() {
    let empty = crate::ProjectDocument::new(
        crate::ProjectId::new("project").unwrap(),
        crate::RevisionId::new("initial").unwrap(),
        crate::PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: crate::FrameRate::new(30, 1).unwrap(),
            color_policy: crate::ColorPolicy::SdrRec709,
        },
        crate::NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut value = serde_json::to_value(&empty).unwrap();
    value["assets"] = serde_json::to_value(assets()).unwrap();
    let document = crate::ProjectDocument::from_json(&value.to_string()).unwrap();
    let id = TargetId::new("speaker").unwrap();
    let request = |from: &crate::ProjectDocument, to: &str, command| crate::CommandRequest {
        project_id: from.project_id().clone(),
        expected_revision: from.revision_id().clone(),
        new_revision: crate::RevisionId::new(to).unwrap(),
        command,
    };
    let set = crate::apply(
        &document,
        &request(
            &document,
            "set",
            crate::Command::SetTarget {
                id: id.clone(),
                target: target(),
            },
        ),
    )
    .unwrap();
    let after = set.forward.apply(&document).unwrap();
    assert_eq!(after.targets()[&id], target());
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    assert_eq!(set.inverse.apply(&after).unwrap(), document);
    let deleted = crate::apply(
        &after,
        &request(
            &after,
            "delete",
            crate::Command::DeleteTarget { id: id.clone() },
        ),
    )
    .unwrap();
    assert!(deleted.forward.apply(&after).unwrap().targets().is_empty());
    assert!(
        crate::apply(
            &document,
            &request(&document, "missing", crate::Command::DeleteTarget { id }),
        )
        .is_err()
    );
}

#[test]
fn copying_a_followed_beat_carries_its_target_to_a_destination_without_it() {
    let basis = crate::PresentationBasis {
        width: 640,
        height: 360,
        frame_rate: crate::FrameRate::new(30, 1).unwrap(),
        color_policy: crate::ColorPolicy::SdrRec709,
    };
    let empty = |revision: &str| {
        let document = crate::ProjectDocument::new(
            crate::ProjectId::new("project").unwrap(),
            crate::RevisionId::new(revision).unwrap(),
            basis.clone(),
            crate::NodeId::new("root").unwrap(),
        )
        .unwrap();
        let mut value = serde_json::to_value(&document).unwrap();
        value["assets"] = serde_json::to_value(assets()).unwrap();
        value
    };
    let mut source = empty("source");
    let mut hold = crate::BeatNode::hold(
        "Pause",
        crate::HoldRecipe {
            duration: crate::FrameDuration::new(5).unwrap(),
            video: crate::HoldVideo::Background,
            picture_context: None,
            audio: crate::HoldAudio::Silence,
        },
    );
    hold.framing = Some(crate::Framing {
        clock: crate::FramingClock::OwnerOutput,
        value: crate::FramingValue::Follow {
            target: TargetId::new("speaker").unwrap(),
            scale: ExactRatio::integer(2),
            fallback: crate::FramingPose::identity(),
        },
    });
    source["nodes"]["root"]["kind"]["children"] = serde_json::json!(["held"]);
    source["nodes"]["held"] = serde_json::to_value(&hold).unwrap();
    source["targets"] = serde_json::json!({ "speaker": target() });
    let source = crate::ProjectDocument::from_json(&source.to_string()).unwrap();
    let slice = crate::CapturedEditSlice::capture_selection(
        &source,
        &crate::NodeId::new("root").unwrap(),
        &crate::SliceCaptureSelection::Child {
            node: crate::NodeId::new("held").unwrap(),
        },
        crate::AudioTimingId {
            allocation: crate::RevisionId::new("copy").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let destination = crate::ProjectDocument::from_json(&empty("destination").to_string()).unwrap();
    let required = slice.identity_requirements().unwrap();
    let pasted = crate::apply(
        &destination,
        &crate::CommandRequest {
            project_id: destination.project_id().clone(),
            expected_revision: destination.revision_id().clone(),
            new_revision: crate::RevisionId::new("pasted").unwrap(),
            command: crate::Command::SpliceSlice {
                parent: crate::NodeId::new("root").unwrap(),
                index: 0,
                slice,
                timing: crate::AudioTimingId {
                    allocation: crate::RevisionId::new("pasted").unwrap(),
                    ordinal: 0,
                },
                identities: crate::SlicePasteIdentities {
                    authored: crate::OccurrenceIdentities {
                        nodes: (0..required.nodes)
                            .map(|i| crate::NodeId::new(format!("pasted-{i}")).unwrap())
                            .collect(),
                        marks: Vec::new(),
                    },
                    aliases: (0..required.aliases)
                        .map(|i| crate::NodeId::new(format!("alias-{i}")).unwrap())
                        .collect(),
                },
            },
        },
    )
    .unwrap();
    let after = pasted.forward.apply(&destination).unwrap();
    assert_eq!(
        after.targets()[&TargetId::new("speaker").unwrap()],
        target()
    );
}
