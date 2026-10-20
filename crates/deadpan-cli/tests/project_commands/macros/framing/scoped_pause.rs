//! Qualified freeze/black intent carries the inserted Hold through calls.

use super::*;
use std::sync::Arc;

fn pause(frames: u32, black: bool) -> I {
    I::InsertPause {
        length: PauseLength::Frames {
            frames: count(frames),
        },
        black,
    }
}

fn comparable_pause(output: &Value, before: &ProjectDocument) -> Value {
    let mut trace = comparable_trace(output, before.revision_id());
    let mut fresh = BTreeMap::new();
    for row in trace.as_array_mut().unwrap() {
        normalize_scope(&mut row["before_scoped"], before, &mut fresh);
        normalize_scope(&mut row["after_scoped"], before, &mut fresh);
        if row["resolved_selection"]["type"] == "child" {
            normalize_node(&mut row["resolved_selection"]["node"], before, &mut fresh);
        }
        if !row["resolved_parent"].is_null() {
            normalize_node(&mut row["resolved_parent"], before, &mut fresh);
        }
    }
    let mut context = output["context"].clone();
    normalize_scope(&mut context["scoped"], before, &mut fresh);
    json!({"context":context,"trace":trace,"duration_delta":output["edit"]["duration_delta"]})
}

fn assert_retained_original(
    before: &ProjectDocument,
    after: &ProjectDocument,
    cut: i64,
    frames: i64,
) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    for frame in 0..old.duration().frames() {
        let old = old.picture(ProjectFrame(frame)).unwrap();
        let new = new
            .picture(ProjectFrame(frame + i64::from(frame >= cut) * frames))
            .unwrap();
        let deadpan_plan::Picture::Source {
            asset: expected_asset,
            point: expected_point,
            ..
        } = old.picture
        else {
            panic!("Original")
        };
        let deadpan_plan::Picture::Source {
            asset: actual_asset,
            point: actual_point,
            ..
        } = new.picture
        else {
            panic!("retained Original at{frame}")
        };
        assert_eq!(actual_asset, expected_asset);
        assert_eq!(actual_point, expected_point, "exact Original frame{frame}");
    }
}

#[test]
fn headless_scoped_pause_macro_keeps_qualified_freeze_maps_calls_and_zoom_with_dry_run_and_history()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("scoped-pause.json");
    command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(deadpan_core::Framing::static_pose(FramingPose::new(
                ratio(1, 4),
                ratio(1, 2),
                ratio(3, 2),
            )?)?),
        },
        "source-view",
    )?;
    for (child, repeat, plays) in [("camera-source", "inner", 2), ("inner", "outer", 3)] {
        command(
            &package,
            &input,
            Command::WrapRepeat {
                node: node(child),
                id: node(repeat),
                plays,
                gap: None,
                anchor_policy: Default::default(),
            },
            &format!("wrap-{repeat}"),
        )?;
    }
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let finish = vec![
        zoom(
            ZoomKind::Smash { scale: ratio(2, 1) },
            SemanticZoomCenter::Keep,
        ),
        pause(1, true),
    ];
    invoke(
        &package,
        &input,
        &save(&before, 0, "b", serde_json::to_value(&finish)?),
        false,
    )?;
    let body = vec![
        pause(2, false),
        I::Call {
            register: RegisterName::new('b')?,
            count: count(1),
        },
    ];
    invoke(
        &package,
        &input,
        &save(&before, 1, "a", serde_json::to_value(&body)?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let RegisterValue::Macro { program } = bank.entries[&RegisterName::new('a')?].as_ref() else {
        panic!("intent")
    };
    assert_eq!(program.instructions(), body.as_slice());
    let wire = serde_json::to_string(program)?;
    for forbidden in [
        "camera-source",
        "outer",
        "inner",
        "scoped",
        "target",
        "presentation",
    ] {
        assert!(!wire.contains(forbidden), "concrete authority in{wire}");
    }
    let scoped = authority(&before, 1, 1);
    let request = scoped_invocation(&before, bank.version, "a", &scoped, 377, "scoped-pause");
    let frozen = state(&package)?;
    let history = history_rows(&package)?;
    let predicted = prepared_plan(&package, &request)?;
    assert_eq!(predicted.document.duration()?.frames(), 723);
    let preview = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_pause(&preview, &before),
        comparable_pause(&committed, &before)
    );
    assert_eq!(history_rows(&package)?, (history.0 + 1, history.1 + 3));
    assert_eq!(committed["context"]["cursor"], 377);
    assert_eq!(committed["context"]["selected_child"], "outer");
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mapped: SemanticScopedContext =
        serde_json::from_value(committed["context"]["scoped"].clone())?;
    assert!(
        mapped
            .target
            .matches_instance(&after, &mapped.presentation)?
    );
    assert_ne!(mapped.target.node, node("camera-source"));
    let NodeKind::Hold { recipe } = &after.nodes()[&mapped.target.node].kind else {
        panic!("mapped Hold")
    };
    assert_eq!(recipe.duration.frames(), 1);
    assert_eq!(recipe.video, deadpan_core::HoldVideo::Background);
    assert!(after.nodes()[&mapped.target.node].framing.is_none());
    let pause_trace: Vec<_> = committed["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["instruction"]["type"] == "insert_pause")
        .collect();
    assert_eq!(pause_trace.len(), 2);
    assert_eq!(
        pause_trace[0]["after_scoped"],
        pause_trace[1]["before_scoped"]
    );
    let freeze_id: NodeId =
        serde_json::from_value(pause_trace[0]["after_scoped"]["target"]["node"].clone())?;
    let NodeKind::Hold { recipe: freeze } = &after.nodes()[&freeze_id].kind else {
        panic!("freeze")
    };
    assert!(after.nodes()[&freeze_id].framing.is_some());
    assert_eq!(freeze.duration.frames(), 2);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let index = store.source_video_index(after.revision_id(), &AssetId::new("camera")?)?;
    assert_eq!(
        freeze.video,
        deadpan_core::HoldVideo::Freeze {
            asset: AssetId::new("camera")?,
            timestamp: deadpan_core::SourceTimestamp {
                ticks: index.frames()[16].pts,
                time_base: index.time_base()
            }
        }
    );
    let expected_provider = deadpan_cli::pause::pause_provider_within(
        &before,
        &RenderPlan::compile(&before)?,
        ProjectFrame(377),
        Some(&scoped.presentation),
        &mut |_| Ok(Arc::new(index.clone())),
    )?;
    assert_eq!(freeze.picture_context, expected_provider.picture_context);
    assert_retained_original(&before, &after, 377, 3);
    assert_eq!(
        after.nodes()[&node("camera-source")],
        before.nodes()[&node("camera-source")]
    );
    assert_eq!(store.registers()?, bank);
    drop(store);
    let plan = RenderPlan::compile(&after)?;
    assert!(matches!(
        plan.picture(ProjectFrame(377))?.picture,
        deadpan_plan::Picture::Background
    ));
    for frame in [378, 379] {
        assert!(matches!(
            plan.picture(ProjectFrame(frame))?.picture,
            deadpan_plan::Picture::Freeze { .. }
        ));
    }
    let path = package.to_str().unwrap();
    success(&[
        "project",
        "undo",
        path,
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    same_document(&undone, &before)?;
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    same_document(&restored, &after)?;
    let another = scoped_invocation(
        &restored,
        bank.version,
        "a",
        &authority(&restored, 0, 0),
        17,
        "another-play-pause",
    );
    invoke(&package, &input, &another, false)?;
    let replayed = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(replayed.duration()?.frames(), 726);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn headless_scoped_pause_refuses_late_unsupported_calls_mixed_authority_and_foreign_freeze_parent_without_writes()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("scoped-pause-refused.json");
    for (child, repeat, plays) in [("camera-source", "inner", 2), ("inner", "outer", 3)] {
        command(
            &package,
            &input,
            Command::WrapRepeat {
                node: node(child),
                id: node(repeat),
                plays,
                gap: None,
                anchor_policy: Default::default(),
            },
            &format!("wrap-{repeat}"),
        )?;
    }
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    for (version, register, instructions) in [
        (0, "a", vec![pause(2, true), I::Split]),
        (1, "b", vec![pause(2, false)]),
        (
            2,
            "c",
            vec![
                pause(1, true),
                I::InsertAiPause {
                    length: PauseLength::Frames { frames: count(1) },
                },
            ],
        ),
        (
            3,
            "d",
            vec![
                pause(2, false),
                pause(1, true),
                zoom(
                    ZoomKind::Smash { scale: ratio(2, 1) },
                    SemanticZoomCenter::Keep,
                ),
            ],
        ),
    ] {
        invoke(
            &package,
            &input,
            &save(
                &before,
                version,
                register,
                serde_json::to_value(instructions)?,
            ),
            false,
        )?;
    }
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let scoped = authority(&before, 1, 1);
    for register in ["a", "c", "d"] {
        let request = scoped_invocation(
            &before,
            bank.version,
            register,
            &scoped,
            377,
            "late-refusal",
        );
        let error = reject(&package, &input, &request)?;
        if register == "d" {
            assert!(
                error["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("this beat has no picture to frame"),
                "{error}"
            );
        }
        let mut dry = request;
        dry["dry_run"] = json!(true);
        let error = reject(&package, &input, &dry)?;
        if register == "d" {
            assert!(
                error["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("this beat has no picture to frame"),
                "{error}"
            );
        }
    }
    let mut mixed = scoped_invocation(&before, bank.version, "b", &scoped, 377, "mixed-refusal");
    mixed["operation"]["scoped"]["target"]["repeats"][0]["branch"] = json!({"type":"default"});
    reject(&package, &input, &mixed)?;
    // At the first inner play of outer play1, the preceding picture belongs
    // to outer play0's different insertion parent. Keep the native refusal.
    let edge = scoped_invocation(
        &before,
        bank.version,
        "b",
        &authority(&before, 1, 0),
        240,
        "foreign-parent",
    );
    reject(&package, &input, &edge)?;
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    Ok(())
}
