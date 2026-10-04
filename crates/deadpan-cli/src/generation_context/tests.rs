use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 30_000).unwrap()
}
fn node(kind: NodeKind) -> BeatNode {
    BeatNode {
        framing: None,
        cutaways: Vec::new(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: "Fixture".into(),
        kind,
    }
}
fn hold(frames: i64) -> BeatNode {
    node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    })
}
/// `frames` project frames showing source pictures from `first`.
fn source(frames: i64, first: i64) -> BeatNode {
    let stamp = |picture: i64| SourceTimestamp {
        ticks: picture * 1001,
        time_base: clock(),
    };
    node(NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: FrameDuration::new(frames).unwrap(),
            video_mapping: SourceVideoMapping::FitBeat,
            video: SourceVideo::Stream {
                asset: AssetId::new("video").unwrap(),
                span: SourceSpan::new(stamp(first), stamp(first + frames)).unwrap(),
            },
            audio: None,
            link: LinkRelation::Independent,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
        },
    })
}
fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut value = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| id(name)).collect()),
    );
    value["nodes"] = serde_json::to_value(nodes).unwrap();
    value["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            source_qualification: None,
            label: "Video".into(),
            content_hash: "a".repeat(64),
            video: Some(
                SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base: clock(),
                    },
                    SourceTimestamp {
                        ticks: 1001 * 300,
                        time_base: clock(),
                    },
                )
                .unwrap(),
            ),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(300).unwrap()),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

#[test]
fn identity_follows_the_holds_neighbours_not_its_position() {
    // Source pictures 0..10, the Hold, then pictures 10..20.
    let base = document(
        &["left", "h", "right"],
        vec![
            ("left", source(10, 0)),
            ("h", hold(6)),
            ("right", source(10, 10)),
        ],
    );
    let identity = context_identity(&base, &id("h")).unwrap();
    // A pause before everything moves the Hold but keeps its neighbours.
    let moved = document(
        &["lead", "left", "h", "right"],
        vec![
            ("lead", hold(4)),
            ("left", source(10, 0)),
            ("h", hold(6)),
            ("right", source(10, 10)),
        ],
    );
    assert_eq!(context_identity(&moved, &id("h")), Some(identity));
    // A different picture before the Hold changes its context.
    let trimmed = document(
        &["left", "h", "right"],
        vec![
            ("left", source(9, 0)),
            ("h", hold(6)),
            ("right", source(10, 10)),
        ],
    );
    assert_ne!(context_identity(&trimmed, &id("h")), Some(identity));
    // So does its duration.
    let longer = document(
        &["left", "h", "right"],
        vec![
            ("left", source(10, 0)),
            ("h", hold(7)),
            ("right", source(10, 10)),
        ],
    );
    assert_ne!(context_identity(&longer, &id("h")), Some(identity));
    // A missing Hold has no identity.
    let without = document(&["left"], vec![("left", source(10, 0))]);
    assert_eq!(context_identity(&without, &id("h")), None);
}
