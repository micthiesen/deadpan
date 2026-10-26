//! Public semantic commands preserve Follow intent through durable history.
use super::*;
use deadpan_core::{
    AttentionTarget, FollowScaleTransition, Framing, FramingClock, SourceSpan, SourceTimestamp,
    TargetId, TargetRegion,
};

pub(super) fn follow(target: &str, scale: ExactRatio, fallback_scale: ExactRatio) -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new(target).unwrap(),
            scale,
            fallback: FramingPose::new(ratio(1, 3), ratio(2, 3), fallback_scale).unwrap(),
            scale_transition: None,
        },
    }
}

#[test]
fn keep_creep_rebinds_follow_bases_and_target_with_atomic_refusal_and_durable_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("follow-creep.json");
    for (name, x) in [("left", 300_000), ("right", 700_000)] {
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let before = store.snapshot()?;
        let index = store.source_video_index(before.revision_id(), &AssetId::new("camera")?)?;
        let stamp = |ticks| SourceTimestamp {
            ticks,
            time_base: index.time_base(),
        };
        let span = SourceSpan::new(stamp(index.frames()[0].pts), stamp(index.terminal_end()))?;
        drop(store);
        command(
            &package,
            &input,
            Command::SetTarget {
                id: TargetId::new(name)?,
                target: AttentionTarget {
                    asset: AssetId::new("camera")?,
                    label: name.into(),
                    span,
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
            &format!("target-{name}"),
        )?;
    }
    let fixed = follow("left", ratio(3, 2), ratio(3, 5));
    let before = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(fixed.clone()),
        },
        "fixed-follow",
    )?;
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ratio(3, 1),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::Keep,
    );
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "c",
            serde_json::to_value([instruction.clone()])?,
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;

    // Every ordinary Visual selection keeps live Follow and normalizes its
    // own exact window, including a backward full-length selection.
    for selection in [
        Some((0, 120, false)),
        Some((10, 90, false)),
        Some((120, 0, true)),
    ] {
        let cursor = selection
            .filter(|(_, _, extending)| *extending)
            .map_or(30, |(_, head, _)| head);
        let request = invocation(
            &before,
            bank.version,
            "c",
            "camera-source",
            cursor,
            selection,
            "range-preview",
        );
        let frozen = state(&package)?;
        let (framing, frames) = preview_framing(&package, &request, "camera-source")?;
        invoke(&package, &input, &request, true)?;
        assert_eq!(state(&package)?, frozen);
        assert_eq!(frames, 120);
        let Some(Framing {
            value:
                FramingValue::Follow {
                    target,
                    scale_transition: Some(transition),
                    ..
                },
            ..
        }) = framing
        else {
            panic!("live ranged Follow")
        };
        assert_eq!(target.as_str(), "left");
        assert_eq!(transition.factor, ratio(2, 1));
        let (anchor, head, _) = selection.unwrap();
        let range = transition.range.unwrap();
        assert_eq!(range.start, ratio(i128::from(anchor.min(head)), 120));
        assert_eq!(range.end, ratio(i128::from(anchor.max(head)), 120));
        assert_eq!(range.from_factor, ExactRatio::ONE);
    }
    let request = invocation(
        &before,
        bank.version,
        "c",
        "camera-source",
        30,
        None,
        "animated-follow",
    );
    let frozen = state(&package)?;
    let predicted = preview_framing(&package, &request, "camera-source")?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, before.revision_id()),
        comparable_trace(&committed, before.revision_id())
    );
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let animated = writer.snapshot()?;
    let expected = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("left")?,
            scale: ratio(3, 2),
            fallback: FramingPose::new(ratio(1, 3), ratio(2, 3), ratio(3, 5))?,
            scale_transition: Some(FollowScaleTransition {
                factor: ratio(2, 1),
                curve: ZoomCurve::Linear,
                range: None,
            }),
        },
    };
    assert_eq!(
        animated.nodes()[&node("camera-source")].framing,
        Some(expected.clone())
    );
    assert_eq!(predicted, (Some(expected), 120));
    assert_eq!(animated.duration()?, before.duration()?);
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    writer.undo(animated.revision_id(), RevisionId::new("follow-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("follow-undo")?,
        RevisionId::new("follow-redo")?,
    )?;
    let redone = writer.snapshot()?;
    same_document(&redone, &animated)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    drop(writer);
    let copy = scratch.path().join("portable.deadpan");
    success(&[
        "project",
        "copy-portable",
        package.to_str().unwrap(),
        copy.to_str().unwrap(),
    ])?;
    let copied = ProjectStore::open(&copy, AccessMode::ReadOnly)?;
    same_document(&copied.snapshot()?, &redone)?;
    assert_eq!(copied.registers()?, bank);
    copied.validate()?;
    drop(copied);

    // An existing animation now accepts Keep creep retuning. Reusing identical
    // parameters predicts the same authored recipe without writes in dry-run.
    let frozen = state(&package)?;
    invoke(
        &package,
        &input,
        &invocation(
            &redone,
            bank.version,
            "c",
            "camera-source",
            60,
            None,
            "unchanged-retune-preview",
        ),
        true,
    )?;
    assert_eq!(state(&package)?, frozen);
    let rebound = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(follow("right", ratio(2, 1), ratio(1, 2))),
        },
        "different-follow",
    )?;
    let request = invocation(
        &rebound,
        bank.version,
        "c",
        "camera-source",
        90,
        None,
        "rebound-creep",
    );
    invoke(&package, &input, &request, false)?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert!(matches!(&after.nodes()[&node("camera-source")].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, fallback,
            scale_transition: Some(FollowScaleTransition { factor, curve: ZoomCurve::Linear, range: None }) }, .. })
        if target.as_str() == "right" && *scale == ratio(2, 1)
            && fallback.scale == ratio(1, 2) && *factor == ratio(3, 2)));
    assert_eq!(store.registers()?, bank);
    // The saved program is still the semantic instruction, never a resolved
    // SetFraming containing the first target or its factor.
    let stored = serde_json::to_value(&bank.entries[&RegisterName::new('c')?])?;
    assert!(stored.to_string().contains("\"zoom\""));
    assert!(!stored.to_string().contains("animated-follow"));
    for frame in [0, 30, 60, 90, 119] {
        assert_eq!(
            pose(&after, "camera-source", frame).center_x,
            FramingPose::new(ratio(7, 10), ratio(2, 5), ExactRatio::ONE)?
                .quantized()?
                .center_x
        );
    }
    Ok(())
}
