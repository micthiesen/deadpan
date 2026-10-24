//! Typed point history is durable; rectangle-only tracking is a separate concern.
use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};

#[test]
fn point_target_correction_survives_reopen_and_independent_undo_redo()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("points.deadpan");
    let tb = SourceTimeBase::new(1, 1000)?;
    let stamp = |ticks| SourceTimestamp {
        ticks,
        time_base: tb,
    };
    let video = SourceSpan::new(stamp(0), stamp(1000))?;
    let empty = ProjectDocument::new(
        ProjectId::new("points")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut raw = serde_json::to_value(empty)?;
    let asset = AssetId::new("original")?;
    raw["assets"] = serde_json::json!({asset.as_str():AssetRecord{label:"Original".into(),content_hash:"a".repeat(64),
        video:Some(video),audio:None,still_image:false,frame_count:None,source_qualification:None}});
    let before = ProjectDocument::from_json(&raw.to_string())?;
    let mut store = ProjectStore::create(&path, &before)?;
    let id = TargetId::new("point")?;
    let target = AttentionTarget {
        label: "Point".into(),
        asset,
        span: video,
        region: TargetPoint {
            center: [250_000, 750_000],
        }
        .into(),
        samples: vec![],
        corrections: vec![],
        provenance: None,
        display_aspect: Some(ExactRatio::new(16, 9)?),
    };
    let request = |doc: &ProjectDocument,
                   revision: &str,
                   target: AttentionTarget|
     -> Result<CommandRequest, DocumentError> {
        Ok(CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command: Command::SetTarget {
                id: id.clone(),
                target,
            },
        })
    };
    store.commit(&request(&before, "saved", target.clone())?)?;
    let saved = store.snapshot()?;
    let corrected = target.with_point_correction(
        500,
        TargetPoint {
            center: [750_000, 250_000],
        },
    )?;
    store.commit(&request(&saved, "corrected", corrected.clone())?)?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?.targets()[&id], corrected);
    assert_eq!(
        store.snapshot_at(&RevisionId::new("saved")?)?.targets()[&id],
        target
    );
    store.undo(&RevisionId::new("corrected")?, RevisionId::new("undone")?)?;
    assert_eq!(store.snapshot()?.targets()[&id], target);
    store.redo(&RevisionId::new("undone")?, RevisionId::new("redone")?)?;
    assert_eq!(store.snapshot()?.targets()[&id], corrected);
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?.targets()[&id], corrected);
    assert_eq!(store.snapshot()?.duration()?, before.duration()?);
    Ok(())
}
