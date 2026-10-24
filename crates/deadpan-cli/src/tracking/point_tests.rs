use super::*;
use deadpan_core::*;

#[test]
fn point_target_correction_refuses_before_media_preparation_or_worker_launch()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("points.deadpan");
    let tb = SourceTimeBase::new(1, 1000)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: tb,
        },
        SourceTimestamp {
            ticks: 1000,
            time_base: tb,
        },
    )?;
    let doc = ProjectDocument::new(
        ProjectId::new("point-tracking")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let asset = AssetId::new("original")?;
    let id = TargetId::new("point")?;
    let target = AttentionTarget {
        label: "Point".into(),
        asset: asset.clone(),
        span,
        region: TargetPoint {
            center: [500_000, 500_000],
        }
        .into(),
        samples: vec![],
        corrections: vec![],
        provenance: None,
        display_aspect: None,
    };
    let mut raw = serde_json::to_value(doc)?;
    raw["assets"] = serde_json::json!({asset.as_str():AssetRecord{label:"Original".into(),content_hash:"a".repeat(64),
        video:Some(span),audio:None,still_image:false,frame_count:None,source_qualification:None}});
    raw["targets"] = serde_json::json!({id.as_str():target});
    let before = ProjectDocument::from_json(&raw.to_string())?;
    drop(deadpan_store::ProjectStore::create(&path, &before)?);
    let runtime = TrackingRuntime {
        executable: dir.path().join("must-not-exist-worker"),
        environment: BTreeMap::new(),
    };
    let error = correct_target(
        &runtime,
        &path,
        &id,
        0,
        NormalizedRect::new(0.4, 0.4, 0.2, 0.2)?,
        1,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("point targets cannot be rectangle-tracked"),
        "{error}"
    );
    assert_eq!(target_rect_at(&target, 0), None);
    let store = deadpan_store::ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, before);
    Ok(())
}
