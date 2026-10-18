//! A saved program keeps intent; every invocation supplies its current play.

use deadpan_core::{
    InstancePath, IterationId, NodeKind, RepeatEditBranch, RepeatEditStep, RepeatInstance,
    ScopedNodeTarget, SemanticScopedContext,
};

use super::*;

fn iteration(document: &ProjectDocument, owner: &str, ordinal: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(owner)].kind else {
        panic!("Repeat");
    };
    iterations.at(ordinal).unwrap()
}

fn authority(document: &ProjectDocument, outer: u32, inner: u32) -> SemanticScopedContext {
    let steps = [("outer", outer), ("inner", inner)]
        .map(|(owner, ordinal)| (node(owner), iteration(document, owner, ordinal)));
    SemanticScopedContext {
        root: node("outer"),
        target: ScopedNodeTarget {
            node: node("camera-source"),
            repeats: steps
                .iter()
                .map(|(repeat, iteration)| RepeatEditStep {
                    repeat: repeat.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: InstancePath {
            node: node("camera-source"),
            repeats: steps
                .into_iter()
                .map(|(node, iteration)| RepeatInstance { node, iteration })
                .collect(),
        },
    }
}

fn scoped_invocation(
    document: &ProjectDocument,
    version: u64,
    register: &str,
    scoped: &SemanticScopedContext,
    frame: i64,
    revision: &str,
) -> Value {
    let mut request = invocation(document, version, register, "outer", frame, None, revision);
    request["operation"]["scoped"] = serde_json::to_value(scoped).unwrap();
    request
}

fn prepared_plan(package: &Path, request: &Value) -> Result<deadpan_core::SemanticPlan> {
    let parsed = Request::from_json(&serde_json::to_vec(request)?)?;
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    deadpan_cli::macros::prepare(&store, &parsed)?
        .plan
        .ok_or_else(|| "expected semantic plan".into())
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
    for step in value["target"]["repeats"].as_array_mut().unwrap() {
        normalize_node(&mut step["repeat"], before, fresh);
    }
    normalize_node(&mut value["presentation"]["node"], before, fresh);
    for step in value["presentation"]["repeats"].as_array_mut().unwrap() {
        normalize_node(&mut step["node"], before, fresh);
    }
}

/// Dry-run and commit allocate independent fresh node/revision pools. Compare
/// all semantic fields after naming only fresh node IDs by first appearance;
/// repeated identity relationships and every pre-existing/stable play ID remain
/// exact. No camera pose, coordinate, branch choice or duration is normalized.
fn comparable_scoped(output: &Value, before: &ProjectDocument) -> Value {
    let mut trace = comparable_trace(output, before.revision_id());
    let mut fresh = BTreeMap::new();
    for row in trace.as_array_mut().unwrap() {
        normalize_scope(&mut row["before_scoped"], before, &mut fresh);
        normalize_scope(&mut row["after_scoped"], before, &mut fresh);
        if row["resolved_selection"]["type"] == "child" {
            normalize_node(&mut row["resolved_selection"]["node"], before, &mut fresh);
        }
    }
    let mut context = output["context"].clone();
    normalize_scope(&mut context["scoped"], before, &mut fresh);
    json!({"context":context,"trace":trace,"duration_delta":output["edit"]["duration_delta"]})
}

fn assert_other_pictures(
    before: &ProjectDocument,
    after: &ProjectDocument,
    changed: std::ops::Range<i64>,
) {
    let before = RenderPlan::compile(before).unwrap();
    let after = RenderPlan::compile(after).unwrap();
    assert_eq!(before.duration(), after.duration());
    for frame in 0..before.duration().frames() {
        if changed.contains(&frame) {
            continue;
        }
        let expected = before.picture(ProjectFrame(frame)).unwrap();
        let mut actual = after.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(actual.picture, expected.picture, "provider at {frame}");
        assert_eq!(
            actual.local_position, expected.local_position,
            "provider clock at {frame}"
        );
        assert_eq!(
            actual.picture_context, expected.picture_context,
            "captured geometry at {frame}"
        );
        assert_eq!(actual.gap_after, expected.gap_after);
        assert_eq!(actual.captions, expected.captions);
        assert_eq!(actual.framing.len(), expected.framing.len());
        for (actual, expected) in actual.framing.iter_mut().zip(&expected.framing) {
            // Isolation may clone an enclosing subtree even for an unchanged
            // sibling play. Its ID changes; its exact picture semantics cannot.
            actual.instance = expected.instance.clone();
            assert_eq!(&*actual, expected, "framing and retained clocks at {frame}");
        }
    }
}

#[test]
fn headless_scoped_zoom_macro_rebinds_nested_plays_and_maps_calls_with_one_durable_undo() -> Result
{
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("scoped-framing.json");
    let initial_pose = FramingPose::new(ratio(1, 4), ratio(1, 2), ratio(5, 4))?;
    command(
        &package,
        &input,
        Command::SetFraming {
            node: node("camera-source"),
            framing: Some(deadpan_core::Framing::static_pose(initial_pose)?),
        },
        "initial-pose",
    )?;
    command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("camera-source"),
            id: node("inner"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "inner-plays",
    )?;
    let before = command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("inner"),
            id: node("outer"),
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
        "outer-plays",
    )?;
    let creep = zoom(
        ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(2),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::Keep,
    );
    let finish = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(3),
        },
        SemanticZoomCenter::OriginalCenter,
    );
    let body = vec![
        I::Call {
            register: RegisterName::new('z')?,
            count: count(1),
        },
        finish,
    ];
    invoke(
        &package,
        &input,
        &save(&before, 0, "z", serde_json::to_value([creep])?),
        false,
    )?;
    invoke(
        &package,
        &input,
        &save(&before, 1, "a", serde_json::to_value(&body)?),
        false,
    )?;
    invoke(
        &package,
        &input,
        &save(
            &before,
            2,
            "b",
            serde_json::to_value([
                I::Call {
                    register: RegisterName::new('z')?,
                    count: count(1),
                },
                I::Split,
            ])?,
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let RegisterValue::Macro { program } = bank.entries[&RegisterName::new('a')?].as_ref() else {
        panic!("saved intent");
    };
    assert_eq!(program.instructions(), body.as_slice());
    let wire = serde_json::to_string(program)?;
    for forbidden in [
        "camera-source",
        "outer-plays",
        "inner-plays",
        "presentation",
        "scoped",
    ] {
        assert!(
            !wire.contains(forbidden),
            "saved body contains invocation authority: {wire}"
        );
    }
    let scoped = authority(&before, 1, 1);
    let failed = scoped_invocation(&before, bank.version, "b", &scoped, 361, "never-scoped");
    reject(&package, &input, &failed)?;
    let request = scoped_invocation(&before, bank.version, "a", &scoped, 361, "one-play-framed");
    let frozen = state(&package)?;
    let rows = history_rows(&package)?;
    let predicted = prepared_plan(&package, &request)?;
    let preview = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let result = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_scoped(&result, &before),
        comparable_scoped(&preview, &before)
    );
    assert_eq!(history_rows(&package)?, (rows.0 + 1, rows.1 + 2));
    assert_eq!(result["context"]["cursor"], 361);
    assert_eq!(result["context"]["selected_child"], "outer");
    assert!(result["context"]["visual_selection"].is_null());
    let mapped: SemanticScopedContext =
        serde_json::from_value(result["context"]["scoped"].clone())?;
    assert_ne!(mapped.target.node, scoped.target.node);
    assert_ne!(
        mapped.target.repeats[1].repeat,
        scoped.target.repeats[1].repeat
    );
    let rows = result["trace"].as_array().unwrap();
    let zooms: Vec<_> = rows
        .iter()
        .filter(|row| row["instruction"]["type"] == "zoom")
        .collect();
    assert_eq!(zooms.len(), 2);
    assert_eq!(zooms[0]["after_scoped"], zooms[1]["before_scoped"]);
    assert_eq!(
        zooms[0]["after_scoped"]["target"]["node"],
        result["context"]["scoped"]["target"]["node"]
    );
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert!(
        mapped
            .target
            .matches_instance(&after, &mapped.presentation)?
    );
    let predicted_scope = predicted.context.scoped.as_ref().unwrap();
    assert_eq!(
        after.nodes()[&mapped.target.node].framing,
        predicted.document.nodes()[&predicted_scope.target.node].framing
    );
    assert_eq!(
        after.nodes()[&node("camera-source")],
        before.nodes()[&node("camera-source")]
    );
    assert_eq!(after.overrides()[&node("outer")].len(), 1);
    assert_eq!(writer.registers()?, bank);
    for at in [360, 361, 420, 479] {
        let sample = RenderPlan::compile(&after)?.picture(ProjectFrame(at))?;
        let layer = sample
            .framing
            .iter()
            .find(|layer| layer.instance == mapped.presentation && !layer.escalation)
            .unwrap();
        assert_eq!(
            layer.pose.unwrap(),
            FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(3))?
        );
        assert_eq!(layer.duration.frames(), 120);
    }
    assert_other_pictures(&before, &after, 360..480);
    // The Apply wire also receives current mapped authority. Editing an owned
    // play a second time must not clone another subtree or override.
    drop(writer);
    let apply = envelope(
        &after,
        bank.version,
        json!({"type":"apply",
        "program":{"instructions":[zoom(ZoomKind::Smash { scale: ExactRatio::integer(4) },SemanticZoomCenter::Keep)]},
        "parent":after.root(),"selected_child":"outer","cursor":361,"scoped":mapped,"new_revision":"owned-framed"}),
    );
    let parsed = Request::from_json(&serde_json::to_vec(&apply)?)?;
    assert!(matches!(
        parsed.operation,
        deadpan_cli::macros::Operation::Apply {
            scoped: Some(_),
            ..
        }
    ));
    let owned = invoke(&package, &input, &apply, false)?;
    assert_eq!(owned["context"]["scoped"], result["context"]["scoped"]);
    writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(writer.snapshot()?.nodes().len(), after.nodes().len());
    assert_eq!(writer.snapshot()?.overrides(), after.overrides());
    writer.undo(
        &RevisionId::new("owned-framed")?,
        RevisionId::new("owned-undo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    writer.undo(
        &RevisionId::new("owned-undo")?,
        RevisionId::new("scoped-undo")?,
    )?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("scoped-undo")?,
        RevisionId::new("scoped-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    let current = writer.snapshot()?;
    drop(writer);
    // Same saved body, new current invocation. The old recorded play IDs are
    // absent from that body and cannot direct this call back into play two.
    let replay = scoped_invocation(
        &current,
        bank.version,
        "a",
        &authority(&current, 2, 0),
        481,
        "another-play-framed",
    );
    let predicted = prepared_plan(&package, &replay)?;
    let dry = invoke(&package, &input, &replay, true)?;
    let replayed = invoke(&package, &input, &replay, false)?;
    assert_eq!(
        comparable_scoped(&replayed, &current),
        comparable_scoped(&dry, &current)
    );
    let next_scope: SemanticScopedContext =
        serde_json::from_value(replayed["context"]["scoped"].clone())?;
    writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let next = writer.snapshot()?;
    assert_ne!(next_scope.target.node, mapped.target.node);
    assert!(
        next_scope
            .target
            .matches_instance(&next, &next_scope.presentation)?
    );
    assert_eq!(next.overrides()[&node("outer")].len(), 2);
    assert_eq!(
        next.nodes()[&mapped.target.node],
        current.nodes()[&mapped.target.node]
    );
    assert_eq!(
        next.nodes()[&next_scope.target.node].framing,
        predicted.document.nodes()[&predicted.context.scoped.as_ref().unwrap().target.node].framing
    );
    assert_other_pictures(&current, &next, 480..600);
    assert_eq!(writer.registers()?, bank);
    writer.undo(next.revision_id(), RevisionId::new("second-scoped-undo")?)?;
    same_document(&writer.snapshot()?, &current)?;
    writer.undo(
        &RevisionId::new("second-scoped-undo")?,
        RevisionId::new("all-scoped-undo")?,
    )?;
    same_document(&writer.snapshot()?, &before)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn headless_scoped_zoom_wire_and_occurrence_refusals_leave_the_bank_and_history_untouched() -> Result
{
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("scoped-refusals.json");
    command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("camera-source"),
            id: node("inner"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "inner-plays",
    )?;
    let before = command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("inner"),
            id: node("outer"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "outer-plays",
    )?;
    let body = serde_json::to_value([zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::OriginalCenter,
    )])?;
    invoke(
        &package,
        &input,
        &save(&before, 0, "z", body.clone()),
        false,
    )?;
    let request = scoped_invocation(&before, 1, "z", &authority(&before, 0, 1), 121, "refused");
    let mut invalid = request.clone();
    invalid["operation"]["scoped"]["presentation"]["repeats"][1]["iteration"] =
        json!(iteration(&before, "inner", 0));
    reject(&package, &input, &invalid)?;
    let mut invalid = request.clone();
    invalid["operation"]["scoped"]["target"]["repeats"][0]["branch"] = json!({"type":"default"});
    reject(&package, &input, &invalid)?;
    let mut invalid = request.clone();
    invalid["operation"]["visual_selection"] =
        json!({"type":"time","anchor":121,"head":122,"extending":false});
    reject(&package, &input, &invalid)?;
    let mut invalid = request.clone();
    invalid["operation"]["scoped"]["root"] = json!("camera-source");
    reject(&package, &input, &invalid)?;
    let mut invalid = request.clone();
    invalid["operation"]["scoped"]["also"] = json!([]);
    assert_eq!(
        reject(&package, &input, &invalid)?["error"]["code"],
        "HostProtocolInvalid"
    );
    let mut invalid = save(&before, 1, "a", body);
    invalid["operation"]["program"]["instructions"][0]["scoped"] =
        request["operation"]["scoped"].clone();
    assert_eq!(
        reject(&package, &input, &invalid)?["error"]["code"],
        "HostProtocolInvalid"
    );
    let mut invalid = request;
    invalid["operation"]["scoped"]
        .as_object_mut()
        .unwrap()
        .remove("presentation");
    assert_eq!(
        reject(&package, &input, &invalid)?["error"]["code"],
        "HostProtocolInvalid"
    );
    Ok(())
}
