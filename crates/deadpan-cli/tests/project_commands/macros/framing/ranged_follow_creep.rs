//! Saved Keep intent resolves a fresh live target, scale basis and range.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, FollowScaleRange, FollowScaleTransition, Framing, FramingClock, TargetId,
    TargetRegion,
};

#[test]
fn ranged_follow_creep_rebinds_each_invocation_and_keeps_durable_undo_and_registers() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("ranged-follow.json");
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    for (name, x) in [("left", 300_000), ("right", 700_000)] {
        command(
            &package,
            &input,
            Command::SetTarget {
                id: TargetId::new(name)?,
                target: AttentionTarget {
                    label: name.into(),
                    asset: AssetId::new("camera")?,
                    span: initial.assets()[&AssetId::new("camera")?].video.unwrap(),
                    region: TargetRegion {
                        center: [x, 400_000],
                        size: [100_000; 2],
                    }
                    .into(),
                    samples: vec![],
                    corrections: vec![],
                    provenance: None,
                    display_aspect: None,
                },
            },
            &format!("ranged-{name}-target"),
        )?;
    }
    let before = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(follow("left", ratio(3, 2), ratio(3, 5))),
        },
        "ranged-fixed",
    )?;
    let instruction = zoom(
        ZoomKind::Creep {
            from: Some(ratio(9, 5)),
            to: ratio(12, 5),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::Keep,
    );
    invoke(
        &package,
        &input,
        &save(&before, 0, "w", serde_json::to_value([instruction])?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;

    for (number, name, base, fallback, anchor, head) in [
        (0, "left", ratio(3, 2), ratio(3, 5), 20, 80),
        (1, "right", ratio(2, 1), ratio(1, 2), 100, 40),
    ] {
        let before = if number == 0 {
            before.clone()
        } else {
            command(
                &package,
                &input,
                Command::SetFraming {
                    node: node("camera-source"),
                    framing: Some(follow(name, base, fallback)),
                },
                "ranged-rebound-fixed",
            )?
        };
        let start = anchor.min(head);
        let end = anchor.max(head);
        let request = invocation(
            &before,
            bank.version,
            "w",
            "camera-source",
            50,
            Some((anchor, head, false)),
            &format!("ranged-apply-{number}"),
        );
        let frozen = state(&package)?;
        let predicted = preview_framing(&package, &request, "camera-source")?;
        let dry = invoke(&package, &input, &request, true)?;
        assert_eq!(state(&package)?, frozen);
        let result = invoke(&package, &input, &request, false)?;
        assert_eq!(
            comparable_trace(&dry, before.revision_id()),
            comparable_trace(&result, before.revision_id())
        );
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let after = writer.snapshot()?;
        let expected = Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Follow {
                target: TargetId::new(name)?,
                scale: base,
                fallback: FramingPose::new(ratio(1, 3), ratio(2, 3), fallback)?,
                scale_transition: Some(FollowScaleTransition {
                    factor: ratio(12, 5).checked_div(base)?,
                    curve: ZoomCurve::Linear,
                    range: Some(FollowScaleRange {
                        start: ratio(i128::from(start), 120),
                        end: ratio(i128::from(end), 120),
                        from_factor: ratio(9, 5).checked_div(base)?,
                    }),
                }),
            },
        };
        assert_eq!(predicted, (Some(expected.clone()), 120));
        assert_eq!(
            after.nodes()[&node("camera-source")].framing,
            Some(expected)
        );
        assert_eq!(writer.registers()?, bank);
        for frame in [0, start - 1] {
            assert_eq!(
                pose(&after, "camera-source", frame),
                pose(&before, "camera-source", frame)
            );
        }
        for frame in [end, 119] {
            assert_eq!(
                pose(&after, "camera-source", frame).scale,
                FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(12, 5))?
                    .quantized()?
                    .scale
            );
        }
        let undo = RevisionId::new(format!("ranged-undo-{number}"))?;
        writer.undo(after.revision_id(), undo.clone())?;
        same_document(&writer.snapshot()?, &before)?;
        writer.checkpoint()?;
        drop(writer);
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.redo(&undo, RevisionId::new(format!("ranged-redo-{number}"))?)?;
        same_document(&writer.snapshot()?, &after)?;
        assert_eq!(writer.registers()?, bank);
        writer.validate()?;
    }
    let stored = serde_json::to_value(&bank.entries[&RegisterName::new('w')?])?;
    let text = stored.to_string();
    assert!(text.contains("\"zoom\""));
    assert!(
        !text.contains("from_factor")
            && !text.contains("range")
            && !text.contains("left")
            && !text.contains("right")
    );
    Ok(())
}
