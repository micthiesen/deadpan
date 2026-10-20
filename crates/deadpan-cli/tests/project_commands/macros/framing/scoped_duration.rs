//! Qualified existing Holds keep explicit play authority through duration edits.
use super::*;
use deadpan_core::{
    InstancePath, NodeKind, RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget,
    SemanticScopedContext,
};

fn pause(frames: u32, black: bool) -> I {
    I::InsertPause {
        length: PauseLength::Frames {
            frames: count(frames),
        },
        black,
    }
}

fn resize(frames: u32) -> I {
    I::SetHoldDuration {
        length: PauseLength::Frames {
            frames: count(frames),
        },
    }
}

fn fixture(root: &Path, input: &Path) -> Result<(PathBuf, ProjectDocument, SemanticScopedContext)> {
    let package = qualified_project(root)?;
    let original = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    invoke(
        &package,
        input,
        &save(&original, 0, "p", serde_json::to_value([pause(3, false)])?),
        false,
    )?;
    let inserted = invoke(
        &package,
        input,
        &invocation(&original, 1, "p", "camera-source", 17, None, "seed-pause"),
        false,
    )?;
    let hold: NodeId = serde_json::from_value(inserted["context"]["selected_child"].clone())?;
    let with_hold = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let NodeKind::Sequence { children } = &with_hold.nodes()[with_hold.root()].kind else {
        panic!("root")
    };
    assert_eq!(children.len(), 3);
    command(
        &package,
        input,
        Command::Group {
            parent: with_hold.root().clone(),
            start: 0,
            end: 3,
            id: node("body"),
            label: "Original with pause".into(),
        },
        "seed-body",
    )?;
    for (child, repeat, plays) in [("body", "inner", 2), ("inner", "outer", 3)] {
        command(
            &package,
            input,
            Command::WrapRepeat {
                node: node(child),
                id: node(repeat),
                plays,
                gap: None,
                anchor_policy: Default::default(),
            },
            &format!("seed-{repeat}"),
        )?;
    }
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(before.duration()?.frames(), 738);
    let steps: Vec<_> = [("outer", 1), ("inner", 1)]
        .into_iter()
        .map(|(name, ordinal)| {
            let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node(name)].kind else {
                panic!("Repeat")
            };
            RepeatInstance {
                node: node(name),
                iteration: iterations.at(ordinal).unwrap(),
            }
        })
        .collect();
    let scoped = SemanticScopedContext {
        root: node("outer"),
        target: ScopedNodeTarget {
            node: hold.clone(),
            repeats: steps
                .iter()
                .map(|step| RepeatEditStep {
                    repeat: step.node.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: InstancePath {
            node: hold,
            repeats: steps,
        },
    };
    assert!(
        scoped
            .target
            .matches_instance(&before, &scoped.presentation)?
    );
    Ok((package, before, scoped))
}

fn scoped_request(
    document: &ProjectDocument,
    version: u64,
    register: &str,
    scoped: &SemanticScopedContext,
    revision: &str,
) -> Value {
    // Selected Hold is [386,389); invoke from its interior. The duration
    // operation must move subsequent semantic actions to exactly frame 386.
    let mut request = invocation(document, version, register, "outer", 387, None, revision);
    request["operation"]["scoped"] = serde_json::to_value(scoped).unwrap();
    request
}

fn history_rows(package: &Path) -> Result<(i64, i64)> {
    Ok(
        rusqlite::Connection::open(package.join("project.sqlite"))?.query_row(
            "SELECT (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?,
    )
}

fn normalize_node(
    value: &mut Value,
    before: &ProjectDocument,
    fresh: &mut BTreeMap<String, String>,
) {
    let id = value.as_str().unwrap();
    if !before.nodes().contains_key(&node(id)) {
        let next = format!("fresh-node-{}", fresh.len());
        *value = json!(fresh.entry(id.to_owned()).or_insert(next));
    }
}

fn normalize_scope(
    value: &mut Value,
    before: &ProjectDocument,
    fresh: &mut BTreeMap<String, String>,
) {
    if value.is_null() {
        return;
    }
    assert_eq!(value["root"], "outer");
    normalize_node(&mut value["target"]["node"], before, fresh);
    normalize_node(&mut value["presentation"]["node"], before, fresh);
    for step in value["target"]["repeats"].as_array_mut().unwrap() {
        normalize_node(&mut step["repeat"], before, fresh);
    }
    for step in value["presentation"]["repeats"].as_array_mut().unwrap() {
        normalize_node(&mut step["node"], before, fresh);
    }
}

fn comparable(output: &Value, before: &ProjectDocument) -> Value {
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
    json!({"trace":trace,"context":context,"duration_delta":output["edit"]["duration_delta"]})
}

fn assert_retained_pictures(before: &ProjectDocument, after: &ProjectDocument, shift: i64) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    for frame in 0..738 {
        if (386..389).contains(&frame) {
            continue;
        }
        let at = frame + if frame >= 389 { shift } else { 0 };
        let expected = old.picture(ProjectFrame(frame)).unwrap();
        let actual = new.picture(ProjectFrame(at)).unwrap();
        assert_eq!(
            actual.picture, expected.picture,
            "provider at old {frame}, new {at}"
        );
        assert_eq!(
            actual.picture_context, expected.picture_context,
            "geometry at {frame}"
        );
        assert_eq!(
            actual.local_position, expected.local_position,
            "clock at {frame}"
        );
    }
}

#[test]
fn headless_scoped_hold_duration_from_interior_maps_named_zoom_pause_and_durable_history() -> Result
{
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("scoped-duration.json");
    let (package, before, scoped) = fixture(scratch.path(), &input)?;
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
        &save(&before, 1, "b", serde_json::to_value(&finish)?),
        false,
    )?;
    let instructions = vec![
        resize(5),
        I::Call {
            register: RegisterName::new('b')?,
            count: count(1),
        },
    ];
    invoke(
        &package,
        &input,
        &save(&before, 2, "a", serde_json::to_value(&instructions)?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let RegisterValue::Macro { program } = bank.entries[&RegisterName::new('a')?].as_ref() else {
        panic!("macro")
    };
    assert_eq!(program.instructions(), instructions.as_slice());
    let serialized = serde_json::to_string(program)?;
    for forbidden in [
        scoped.target.node.as_str(),
        "outer",
        "inner",
        "presentation",
        "scoped",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "saved concrete authority: {serialized}"
        );
    }
    let request = scoped_request(&before, bank.version, "a", &scoped, "scoped-duration");
    let frozen = state(&package)?;
    let history = history_rows(&package)?;
    let preview = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable(&preview, &before),
        comparable(&committed, &before)
    );
    assert_eq!(committed["edit"]["duration_delta"], 3);
    assert_eq!(committed["context"]["cursor"], 386);
    assert_eq!(committed["context"]["selected_child"], "outer");
    assert_eq!(history_rows(&package)?, (history.0 + 1, history.1 + 3));
    let rows = committed["trace"].as_array().unwrap();
    let duration = rows
        .iter()
        .find(|row| row["instruction"]["type"] == "set_hold_duration")
        .unwrap();
    assert_eq!(duration["resolved_range"], json!({"start":386,"end":391}));
    let zoom = rows
        .iter()
        .find(|row| row["instruction"]["type"] == "zoom")
        .unwrap();
    assert_eq!(zoom["before_cursor"], 386);
    assert_eq!(zoom["before_scoped"], duration["after_scoped"]);
    let pause = rows
        .iter()
        .find(|row| row["instruction"]["type"] == "insert_pause")
        .unwrap();
    assert_eq!(pause["before_cursor"], 386);
    assert_eq!(pause["before_scoped"], zoom["after_scoped"]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 741);
    let selected: SemanticScopedContext = serde_json::from_value(duration["after_scoped"].clone())?;
    assert!(
        selected
            .target
            .matches_instance(&after, &selected.presentation)?
    );
    let NodeKind::Hold { recipe } = &after.nodes()[&selected.target.node].kind else {
        panic!("resized Hold")
    };
    assert_eq!(recipe.duration.frames(), 5);
    assert!(matches!(
        recipe.video,
        deadpan_core::HoldVideo::Freeze { .. }
    ));
    assert!(after.nodes()[&selected.target.node].framing.is_some());
    let NodeKind::Hold { recipe: old } = &before.nodes()[&scoped.target.node].kind else {
        panic!("old Hold")
    };
    assert_eq!(recipe.video, old.video);
    assert_eq!(recipe.picture_context, old.picture_context);
    assert_eq!(
        after.nodes()[&scoped.target.node],
        before.nodes()[&scoped.target.node]
    );
    assert_retained_pictures(&before, &after, 3);
    assert!(matches!(
        RenderPlan::compile(&after)?
            .picture(ProjectFrame(386))?
            .picture,
        deadpan_plan::Picture::Background
    ));
    for frame in 387..392 {
        assert!(matches!(
            RenderPlan::compile(&after)?
                .picture(ProjectFrame(frame))?
                .picture,
            deadpan_plan::Picture::Freeze { .. }
        ));
    }
    assert_eq!(store.registers()?, bank);
    drop(store);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(after.revision_id(), RevisionId::new("duration-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("duration-undo")?,
        RevisionId::new("duration-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn headless_scoped_hold_duration_noop_and_late_unsupported_call_leave_history_and_bank_unchanged()
-> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("scoped-duration-refusal.json");
    let (package, before, scoped) = fixture(scratch.path(), &input)?;
    for (version, register, instructions) in [
        (1, "n", vec![resize(3)]),
        (2, "b", vec![I::Split]),
        (
            3,
            "a",
            vec![
                resize(5),
                I::Call {
                    register: RegisterName::new('b')?,
                    count: count(1),
                },
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
    let frozen = state(&package)?;
    let history = history_rows(&package)?;
    for register in ["n", "a"] {
        for dry_run in [false, true] {
            let mut request =
                scoped_request(&before, bank.version, register, &scoped, "refused-duration");
            request["dry_run"] = json!(dry_run);
            let error = reject(&package, &input, &request)?;
            let message = error.to_string();
            assert!(
                message.contains(if register == "n" {
                    "already has that duration"
                } else {
                    "single-play macro authority"
                }),
                "{message}"
            );
        }
        assert_eq!(state(&package)?, frozen);
        assert_eq!(history_rows(&package)?, history);
        same_document(
            &ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
            &before,
        )?;
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
            bank
        );
    }
    Ok(())
}
