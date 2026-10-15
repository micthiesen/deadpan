use deadpan_core::*;
use deadpan_store::ProjectStore;

use super::snapshot;

#[test]
fn derived_clocks_open_in_the_native_workspace_without_authored_changes() {
    let id = |name: &str| NodeId::new(name).unwrap();
    let frames = |n| FrameDuration::new(n).unwrap();
    let empty = ProjectDocument::new(
        ProjectId::new("clocked").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut retime = BeatNode::sequence("Retime", vec![]);
    retime.kind = NodeKind::Retime {
        child: id("hold"),
        duration: frames(2),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::json!({
        "root": BeatNode::sequence("Root", vec![id("retime")]),
        "retime": retime,
        "hold": BeatNode::hold("Hold", HoldRecipe { duration: frames(2), video: HoldVideo::Background, audio: HoldAudio::Silence, picture_context: None }),
    });
    wire["derived_clocks"] = serde_json::json!({
        "retime": DerivedRetimeClock { extent: ExactRatio::integer(2), child_start: ExactRatio::new(1, 2).unwrap(), child_end: ExactRatio::new(3, 2).unwrap(), preserve_input: None }
    });
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("clocked.deadpan");
    let store = ProjectStore::create(&path, &document).unwrap();
    // The plan reads the exact clock; integral recipes outside it are exact.
    let workspace = snapshot(&store, 1, path, None).unwrap();
    assert_eq!(workspace.plan.duration(), frames(2));
    assert_eq!(store.snapshot().unwrap(), document);
}
