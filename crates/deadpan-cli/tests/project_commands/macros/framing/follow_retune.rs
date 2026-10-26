//! Existing Follow animations remain editable through public semantic commands.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, FollowScaleRange, FollowScaleTransition, Framing, FramingClock, TargetId,
    TargetRegion,
};

fn targets(package: &Path, input: &Path) -> Result {
    let document = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    for (name, x) in [("left", 300_000), ("right", 700_000)] {
        command(
            package,
            input,
            Command::SetTarget {
                id: TargetId::new(name)?,
                target: AttentionTarget {
                    label: name.into(),
                    asset: AssetId::new("camera")?,
                    span: document.assets()[&AssetId::new("camera")?].video.unwrap(),
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
            &format!("retune-target-{name}"),
        )?;
    }
    Ok(())
}

fn animated(name: &str, base: ExactRatio, fallback: ExactRatio, ranged: bool) -> Framing {
    let mut framing = follow(name, base, fallback);
    framing.clock = FramingClock::RetainedOutput {
        offset: ratio(15, 2),
        duration: ratio(180, 1),
    };
    let FramingValue::Follow {
        scale_transition, ..
    } = &mut framing.value
    else {
        unreachable!()
    };
    *scale_transition = Some(FollowScaleTransition {
        factor: ratio(7, 5),
        curve: ZoomCurve::Linear,
        range: ranged.then(|| FollowScaleRange {
            start: ratio(1, 4),
            end: ratio(3, 4),
            from_factor: ratio(6, 5),
        }),
    });
    framing
}

fn apply_retune(document: &ProjectDocument, version: u64, kind: ZoomKind, revision: &str) -> Value {
    envelope(
        document,
        version,
        json!({"type":"apply","program":{"instructions":[zoom(kind, SemanticZoomCenter::Keep)]},
            "parent":document.root(),"cursor":60,"selected_child":"camera-source",
            "visual_selection":null,"new_revision":revision}),
    )
}

#[test]
fn existing_follow_retune_rebinds_named_intent_and_preserves_clock_window_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("retune.json");
    targets(&package, &input)?;
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ratio(3, 1),
            curve: ZoomCurve::Smoothstep,
        },
        SemanticZoomCenter::Keep,
    );
    invoke(
        &package,
        &input,
        &save(&initial, 0, "r", serde_json::to_value([instruction])?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    for (index, name, base, fallback, ranged) in [
        (0, "left", ratio(3, 2), ratio(3, 5), false),
        (1, "right", ratio(2, 1), ratio(1, 2), true),
    ] {
        let entry = animated(name, base, fallback, ranged);
        let before = command(
            &package,
            &input,
            Command::SetFraming {
                node: node("camera-source"),
                framing: Some(entry.clone()),
            },
            &format!("retune-entry-{index}"),
        )?;
        let request = invocation(
            &before,
            bank.version,
            "r",
            "camera-source",
            60,
            None,
            &format!("retune-apply-{index}"),
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
        let mut expected = entry;
        let FramingValue::Follow {
            scale_transition, ..
        } = &mut expected.value
        else {
            unreachable!()
        };
        let transition = scale_transition.as_mut().unwrap();
        transition.factor = ratio(3, 1).checked_div(base)?;
        transition.curve = ZoomCurve::Smoothstep;
        assert_eq!(predicted, (Some(expected.clone()), 120));
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let after = writer.snapshot()?;
        let mut expected_document = serde_json::to_value(&before)?;
        expected_document["revision_id"] = json!(after.revision_id());
        expected_document["nodes"]["camera-source"]["framing"] = serde_json::to_value(expected)?;
        assert_eq!(serde_json::to_value(&after)?, expected_document);
        assert_eq!(writer.registers()?, bank);
        writer.validate()?;
        let undo = RevisionId::new(format!("retune-undo-{index}"))?;
        writer.undo(after.revision_id(), undo.clone())?;
        same_document(&writer.snapshot()?, &before)?;
        writer.checkpoint()?;
        drop(writer);
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.redo(&undo, RevisionId::new(format!("retune-redo-{index}"))?)?;
        same_document(&writer.snapshot()?, &after)?;
        assert_eq!(writer.registers()?, bank);
        writer.validate()?;
    }
    let final_document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let portable = scratch.path().join("retuned-portable.deadpan");
    success(&[
        "project",
        "copy-portable",
        package.to_str().unwrap(),
        portable.to_str().unwrap(),
    ])?;
    let copied = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_document(&copied.snapshot()?, &final_document)?;
    assert_eq!(copied.registers()?, bank);
    copied.validate()?;
    let stored = serde_json::to_string(&bank.entries[&RegisterName::new('r')?])?;
    assert!(stored.contains("\"zoom\""));
    for resolved in ["left", "right", "from_factor", "retained_output"] {
        assert!(
            !stored.contains(resolved),
            "saved macro captured {resolved}"
        );
    }
    Ok(())
}

#[test]
fn existing_follow_retune_explicit_start_and_refusals_are_atomic() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("retune-admission.json");
    targets(&package, &input)?;
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
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
        &save(&initial, 0, "r", serde_json::to_value([instruction])?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    for ranged in [false, true] {
        let entry = animated("left", ratio(3, 2), ratio(3, 5), ranged);
        let before = command(
            &package,
            &input,
            Command::SetFraming {
                node: node("camera-source"),
                framing: Some(entry.clone()),
            },
            if ranged {
                "retune-ranged-entry"
            } else {
                "retune-whole-entry"
            },
        )?;
        for (anchor, head, extending) in [(0, 120, false), (20, 80, false), (120, 0, true)] {
            let cursor = if extending { head } else { 60 };
            reject(
                &package,
                &input,
                &invocation(
                    &before,
                    bank.version,
                    "r",
                    "camera-source",
                    cursor,
                    Some((anchor, head, extending)),
                    "retune-range-refusal",
                ),
            )?;
        }
        let request = apply_retune(
            &before,
            bank.version,
            ZoomKind::Creep {
                from: Some(ratio(2, 1)),
                to: ratio(3, 1),
                curve: ZoomCurve::Smoothstep,
            },
            if ranged {
                "retune-ranged-start"
            } else {
                "retune-whole-start"
            },
        );
        invoke(&package, &input, &request, false)?;
        let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let mut expected = entry;
        let FramingValue::Follow {
            scale,
            fallback,
            scale_transition,
            ..
        } = &mut expected.value
        else {
            unreachable!()
        };
        let transition = scale_transition.as_mut().unwrap();
        if let Some(range) = &mut transition.range {
            range.from_factor = ratio(4, 3);
            transition.factor = ratio(2, 1);
        } else {
            *scale = ratio(2, 1);
            fallback.scale = ratio(4, 5);
            transition.factor = ratio(3, 2);
        }
        transition.curve = ZoomCurve::Smoothstep;
        assert_eq!(
            after.nodes()[&node("camera-source")].framing,
            Some(expected)
        );
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
            bank
        );
    }
    // Both recipes are valid on entry; a valid requested live endpoint may
    // still exceed the independent fallback's bound when the recipe is retuned.
    let mut dangerous = follow("right", ExactRatio::ONE, ratio(40, 1));
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut dangerous.value
    {
        *scale_transition = Some(FollowScaleTransition {
            factor: ExactRatio::ONE,
            curve: ZoomCurve::Linear,
            range: None,
        });
    }
    let before = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(dangerous),
        },
        "retune-large-fallback",
    )?;
    for (index, to, curve) in [
        (0, ratio(2, 1), ZoomCurve::Linear),
        (1, ratio(65, 1), ZoomCurve::Linear),
        (2, ratio(1, 1), ZoomCurve::Step),
    ] {
        reject(
            &package,
            &input,
            &apply_retune(
                &before,
                bank.version,
                ZoomKind::Creep {
                    from: None,
                    to,
                    curve,
                },
                &format!("retune-invalid-{index}"),
            ),
        )?;
    }
    Ok(())
}
