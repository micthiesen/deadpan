use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

fn silent_hold(frames: i64) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        duration: FrameDuration::new(frames)?,
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}

pub(super) fn nested_initial(source: bool) -> Result<ProjectDocument> {
    let root = NodeId::new("root")?;
    let outer = NodeId::new("outer")?;
    let inner = NodeId::new("inner")?;
    let first = NodeId::new("first")?;
    let second = NodeId::new("second")?;
    let initial = ProjectDocument::new(
        ProjectId::new("nested-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )?;
    let mut wire = serde_json::to_value(initial)?;
    let first_node = if source {
        let asset = AssetId::new("original")?;
        let time_base = SourceTimeBase::new(1, 30)?;
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 4,
                time_base,
            },
        )?;
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            asset.clone(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(span),
                audio: None,
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        )]))?;
        BeatNode {
            audio_treatments: Default::default(),
            framing: None,
            label: "Original".into(),
            audio_editorial_edges: Default::default(),
            audio_edges: AudioEdgePolicies::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    edit_window: None,
                    duration: FrameDuration::new(4)?,
                    video: SourceVideo::Stream { asset, span },
                    audio: None,
                    link: LinkRelation::Independent,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio_mapping: SourceAudioMapping::FitBeat,
                    audio_offset: AudioSample(0),
                },
            },
            cutaways: Vec::new(),
        }
    } else {
        BeatNode::hold("First", silent_hold(4)?)
    };
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![outer.clone()])),
        (outer, BeatNode::sequence("Outer", vec![inner.clone()])),
        (
            inner,
            BeatNode::sequence("Inner", vec![first.clone(), second.clone()]),
        ),
        (first, first_node),
        (second, BeatNode::hold("Second", silent_hold(3)?)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}
