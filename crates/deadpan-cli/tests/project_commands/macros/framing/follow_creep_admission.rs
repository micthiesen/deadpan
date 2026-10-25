//! Public command and semantic rejection preserve the whole package state.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{AttentionTarget, FollowScaleTransition, TargetId, TargetRegion};

#[test]
fn explicit_follow_creep_from_rebases_both_scales_and_invalid_endpoints_never_publish() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("explicit-follow.json");
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    command(
        &package,
        &input,
        Command::SetTarget {
            id: TargetId::new("subject")?,
            target: AttentionTarget {
                asset: AssetId::new("camera")?,
                label: "Subject".into(),
                span: before.assets()[&AssetId::new("camera")?].video.unwrap(),
                region: TargetRegion {
                    center: [400_000, 500_000],
                    size: [100_000; 2],
                }
                .into(),
                samples: vec![],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            },
        },
        "target",
    )?;
    let before = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(follow("subject", ratio(3, 2), ratio(3, 5))),
        },
        "fixed",
    )?;
    let instruction = zoom(
        ZoomKind::Creep {
            from: Some(ratio(2, 1)),
            to: ratio(3, 1),
            curve: ZoomCurve::Smoothstep,
        },
        SemanticZoomCenter::Keep,
    );
    invoke(
        &package,
        &input,
        &save(&before, 0, "f", serde_json::to_value([instruction])?),
        false,
    )?;
    invoke(
        &package,
        &input,
        &invocation(&before, 1, "f", "camera-source", 20, None, "explicit-from"),
        false,
    )?;
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = follow("subject", ratio(2, 1), ratio(4, 5));
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut expected.value
    {
        *scale_transition = Some(FollowScaleTransition {
            factor: ratio(3, 2),
            curve: ZoomCurve::Smoothstep,
            range: None,
        });
    }
    assert_eq!(
        after.nodes()[&node("camera-source")].framing,
        Some(expected.clone())
    );

    let frozen = state(&package)?;
    // These pass JSON parsing but fail actual command/complete-result admission.
    for (name, target, factor, fallback) in [
        ("live-endpoint", "subject", ratio(33, 1), ratio(4, 5)),
        ("fallback-endpoint", "subject", ratio(2, 1), ratio(40, 1)),
        ("missing-target", "absent", ratio(3, 2), ratio(4, 5)),
    ] {
        let mut invalid = expected.clone();
        if let FramingValue::Follow {
            target: actual,
            scale_transition,
            fallback: pose,
            ..
        } = &mut invalid.value
        {
            *actual = TargetId::new(target)?;
            scale_transition.as_mut().unwrap().factor = factor;
            pose.scale = fallback;
        }
        fs::write(
            &input,
            serde_json::to_vec(&json!({
                "protocol":1,"project_id":after.project_id(),"expected_revision":after.revision_id(),
                "new_revision":name,"command":Command::SetFraming { node:node("camera-source"),framing:Some(invalid) },
            }))?,
        )?;
        let result = cli(&[
            "command",
            package.to_str().unwrap(),
            "--json",
            input.to_str().unwrap(),
        ])?;
        assert!(!result.status.success(), "{name} unexpectedly published");
        assert_eq!(state(&package)?, frozen, "{name}");
        let error: Value = serde_json::from_slice(&result.stderr)?;
        assert!(error["error"]["message"].is_string(), "{error}");
    }
    let mut raw = json!({
        "protocol":1,"project_id":after.project_id(),"expected_revision":after.revision_id(),
        "new_revision":"invalid-curve","command":Command::SetFraming { node:node("camera-source"),framing:Some(expected) },
    });
    raw["command"]["framing"]["value"]["scale_transition"]["curve"] = json!("cubic");
    fs::write(&input, serde_json::to_vec(&raw)?)?;
    let result = cli(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    assert!(!result.status.success());
    assert_eq!(state(&package)?, frozen);
    Ok(())
}
