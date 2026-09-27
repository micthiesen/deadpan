//! Real keyboard replay for a pause inserted inside nested live Sequences.

use deadpan_core::{
    Command, CommandRequest, ExactRatio, Framing, FramingPose, HoldAudio, HoldVideo, NodeId,
    NodeKind, ProjectDocument, ProjectFrame, RevisionId, SourceVideo,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "This replay enters Sequence groups. Repeat and Retime entry, occurrence isolation, and their pause insertion remain outside this scenario.".into(),
    );

    let initial = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("Nested-pause replay requires the initial private workspace")?;
    let path = initial.path.clone();
    let source = initial
        .document
        .children(initial.document.root())
        .next()
        .cloned()
        .ok_or("Initial project has no Original Source")?;
    let inner = NodeId::new("ui-nested-pause-inner").map_err(|error| error.to_string())?;
    let outer = NodeId::new("ui-nested-pause-outer").map_err(|error| error.to_string())?;
    let source_pose = FramingPose {
        center_x: ExactRatio::new(1, 3).map_err(|error| error.to_string())?,
        scale: ExactRatio::new(3, 2).map_err(|error| error.to_string())?,
        ..Default::default()
    };
    let inner_pose = FramingPose {
        center_y: ExactRatio::new(2, 5).map_err(|error| error.to_string())?,
        scale: ExactRatio::new(5, 4).map_err(|error| error.to_string())?,
        ..Default::default()
    };
    let outer_pose = FramingPose {
        center_x: ExactRatio::new(3, 5).map_err(|error| error.to_string())?,
        scale: ExactRatio::new(4, 5).map_err(|error| error.to_string())?,
        ..Default::default()
    };
    let root_pose = FramingPose {
        center_y: ExactRatio::new(1, 3).map_err(|error| error.to_string())?,
        scale: ExactRatio::new(6, 5).map_err(|error| error.to_string())?,
        ..Default::default()
    };

    // Close the service-owned writer before adding a realistic nested fixture
    // through the normal typed store commands. Reopen remains a scripted OS
    // picker result; project admission and all following actions are real.
    d.app()
        .service
        .submit(ProjectRequest::Close)
        .map_err(|error| error.to_string())?;
    d.wait_for("Initial project writer closes", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)
        .map_err(|error| format!("Open nested-pause fixture: {error}"))?;
    let document = apply(
        &mut store,
        "ui-nested-pause-source-framing",
        Command::SetFraming {
            node: source.clone(),
            framing: Some(Framing::static_pose(source_pose).map_err(|error| error.to_string())?),
        },
    )?;
    apply(
        &mut store,
        "ui-nested-pause-inner-group",
        Command::Group {
            parent: document.root().clone(),
            start: 0,
            end: 1,
            id: inner.clone(),
            label: "Inner live group".into(),
        },
    )?;
    let document = apply(
        &mut store,
        "ui-nested-pause-inner-framing",
        Command::SetFraming {
            node: inner.clone(),
            framing: Some(Framing::static_pose(inner_pose).map_err(|error| error.to_string())?),
        },
    )?;
    apply(
        &mut store,
        "ui-nested-pause-outer-group",
        Command::Group {
            parent: document.root().clone(),
            start: 0,
            end: 1,
            id: outer.clone(),
            label: "Outer live group".into(),
        },
    )?;
    let document = apply(
        &mut store,
        "ui-nested-pause-outer-framing",
        Command::SetFraming {
            node: outer.clone(),
            framing: Some(Framing::static_pose(outer_pose).map_err(|error| error.to_string())?),
        },
    )?;
    apply(
        &mut store,
        "ui-nested-pause-root-framing",
        Command::SetFraming {
            node: document.root().clone(),
            framing: Some(Framing::static_pose(root_pose).map_err(|error| error.to_string())?),
        },
    )?;
    let before_document = store.snapshot().map_err(|error| error.to_string())?;
    drop(store);

    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path.clone()))]);
    d.key_modified(egui::Key::O, egui::Modifiers::COMMAND)?;
    d.wait_for("Reopen nested live-group project", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.path == path)
            && !app.service.is_busy()
            && !app.importing()
    })?;
    d.command("sequence")?;
    d.key(egui::Key::Home)?;
    d.settled()?;
    let baseline = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("Nested fixture did not reopen")?;
    if baseline.document.nodes() != before_document.nodes() {
        return Err("Reopened nested fixture differs from its typed setup".into());
    }
    let before_nodes = baseline.document.nodes().clone();
    let before_bindings = baseline.document.audio_bindings().clone();
    let before_duration = baseline.plan.duration().frames();

    // User navigation to boundary 17, then the actual command bar operation.
    d.chord(&[
        egui::Key::G,
        egui::Key::G,
        egui::Key::Num1,
        egui::Key::Num7,
        egui::Key::L,
    ])?;
    d.settled()?;
    d.check(
        "Keyboard navigation reaches exact nested project boundary 17",
        d.app().sequence_cursor == 17,
        json!(17),
        json!(d.app().sequence_cursor),
    )?;
    let before_revision = d.revision();
    d.command("hold 11f")?;
    d.changed(&before_revision)?;
    d.settled()?;

    let after = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("Pause commit has no workspace")?;
    let root_children = after
        .document
        .children(after.document.root())
        .cloned()
        .collect::<Vec<_>>();
    let outer_children = after.document.children(&outer).cloned().collect::<Vec<_>>();
    let inner_children = after.document.children(&inner).cloned().collect::<Vec<_>>();
    let hold = inner_children
        .iter()
        .find(|node| matches!(&after.document.nodes()[*node].kind, NodeKind::Hold { .. }))
        .cloned()
        .ok_or("Keyboard insertion did not create a Hold inside the inner Sequence")?;
    let NodeKind::Hold { recipe } = &after.document.nodes()[&hold].kind else {
        unreachable!("the nested Hold search checked its kind")
    };
    let source_asset = match &after.document.nodes()[&source].kind {
        NodeKind::Source { source } => match &source.video {
            SourceVideo::Stream { asset, .. } => asset,
            _ => return Err("Nested-pause fixture source is not a video stream".into()),
        },
        kind => {
            return Err(format!(
                "Original changed kind during fixture setup: {kind:?}"
            ));
        }
    };
    let video_index = after
        .sources
        .get(source_asset)
        .and_then(|registered| registered.video_index.as_ref())
        .ok_or("Nested fixture lost its measured source index")?;
    let expected_frame = baseline
        .plan
        .picture(ProjectFrame(16))
        .map_err(|error| error.to_string())?
        .picture
        .select_source_frame(video_index)
        .map_err(|error| error.to_string())?;
    let expected_video = HoldVideo::Freeze {
        asset: source_asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: expected_frame.pts,
            time_base: video_index.time_base(),
        },
    };
    let sample = after
        .plan
        .picture(ProjectFrame(17))
        .map_err(|error| error.to_string())?;
    let scopes = sample
        .framing
        .iter()
        .map(|scope| (scope.instance.node.clone(), scope.pose))
        .collect::<Vec<_>>();
    let expected_scopes = vec![
        (hold.clone(), None),
        (inner.clone(), Some(inner_pose)),
        (outer.clone(), Some(outer_pose)),
        (after.document.root().clone(), Some(root_pose)),
    ];
    d.check(
        "Keyboard pause is nested, freezes the exact left picture, and leaves live owners visible once",
        after.plan.duration().frames() == before_duration + 11
            && root_children == vec![outer.clone()]
            && outer_children == vec![inner.clone()]
            && inner_children.len() == 3
            && inner_children[1] == hold
            && recipe.duration.frames() == 11
            && recipe.audio == HoldAudio::Silence
            && recipe.video == expected_video
            && recipe
                .picture_context
                .as_ref()
                .is_some_and(|context| context.canvases.len() == 1
                    && context.canvases[0].layers == vec![Some(source_pose)])
            && sample.picture_context.as_deref() == recipe.picture_context.as_ref()
            && scopes == expected_scopes
            && d.app().sequence_cursor == 17
            && d.app().selected_beat.as_ref() == Some(&outer),
        json!({
            "nested_under": ["Outer live group", "Inner live group"],
            "duration_delta": 11,
            "hold_audio": "silence",
            "cursor": 17,
            "visible_selection": "Outer live group",
            "framing_scopes": expected_scopes,
        }),
        json!({
            "root_children": root_children,
            "outer_children": outer_children,
            "inner_children": inner_children,
            "hold_video": recipe.video,
            "picture_context": recipe.picture_context,
            "framing_scopes": scopes,
            "cursor": d.app().sequence_cursor,
            "selected": d.app().selected_beat,
        }),
    )?;
    d.capture("Nested silent pause inside live framing groups")?;

    // History treats insertion as one atomic edit before group navigation
    // exposes the new Hold for direct inspection and editing.
    let inserted_nodes = after.document.nodes().clone();
    let inserted_bindings = after.document.audio_bindings().clone();
    let inserted_revision = d.revision();
    d.key(egui::Key::U)?;
    d.changed(&inserted_revision)?;
    d.settled()?;
    let undone = d.app().workspace.as_ref().unwrap().document.clone();
    d.check(
        "One undo removes the nested pause and restores the exact node and binding state",
        undone.nodes() == &before_nodes
            && undone.audio_bindings() == &before_bindings
            && undone.duration().unwrap().frames() == before_duration,
        json!({"nodes": "fixture state", "audio_bindings": "fixture state", "frames": before_duration}),
        json!({"nodes_restored": undone.nodes() == &before_nodes, "audio_bindings_restored": undone.audio_bindings() == &before_bindings, "frames": undone.duration().unwrap().frames()}),
    )?;
    let undo_revision = d.revision();
    d.key_modified(
        egui::Key::Z,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )?;
    d.changed(&undo_revision)?;
    d.settled()?;
    let redone = d.app().workspace.as_ref().unwrap().document.clone();
    d.check(
        "One redo restores the committed nested pause and audio bindings",
        redone.nodes() == &inserted_nodes && redone.audio_bindings() == &inserted_bindings,
        json!({"nodes": "inserted state", "audio_bindings": "inserted state"}),
        json!({"nodes_restored": redone.nodes() == &inserted_nodes, "audio_bindings_restored": redone.audio_bindings() == &inserted_bindings}),
    )?;
    d.capture("Nested pause after redo")?;
    navigate_and_edit(d, &outer, &inner, &hold)
}

fn navigate_and_edit(
    d: &mut Driver<'_>,
    outer: &NodeId,
    inner: &NodeId,
    hold: &NodeId,
) -> Result<(), String> {
    let before = d.app().workspace.as_ref().unwrap().document.clone();
    d.key(egui::Key::Enter)?;
    d.settled()?;
    check_scope(
        d,
        "Enter opens Outer at the same project boundary",
        std::slice::from_ref(outer),
        inner,
        17,
    )?;
    d.key(egui::Key::Enter)?;
    d.settled()?;
    let groups = [outer.clone(), inner.clone()];
    check_scope(
        d,
        "Enter opens Inner and selects the pause at its boundary",
        &groups,
        hold,
        17,
    )?;
    d.check(
        "The nested Hold exposes its duration inspector action",
        d.rect("Change duration…  ·  Enter").is_ok(),
        json!("Change duration…  ·  Enter"),
        d.widgets(),
    )?;
    d.capture("Nested Hold selected with group breadcrumbs")?;

    // Open the real inspector parameter field, then use native select-all and
    // submit. No harness mutation selects the Hold or changes authored state.
    let before_revision = d.revision();
    d.key_modified(egui::Key::Tab, egui::Modifiers::SHIFT)?;
    d.check(
        "Shift-Tab focuses the nested Hold inspector",
        d.app().pane == Pane::Inspector,
        json!("Inspector"),
        d.snapshot(),
    )?;
    d.key(egui::Key::Enter)?;
    d.check(
        "Inspector Enter opens the duration command instead of group navigation",
        d.app().command_open && d.app().sequence_scope.groups() == groups,
        json!({"command_open": true, "groups": groups}),
        d.snapshot(),
    )?;
    d.key_modified(egui::Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Set the nested Hold duration through its inspector command field",
        vec![
            egui::Event::Text("hold-duration 17f".into()),
            key_event(egui::Key::Enter, egui::Modifiers::NONE, true),
            key_event(egui::Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before_revision)?;
    let resized = d.app().workspace.as_ref().unwrap().document.clone();
    let resized_hold = &resized.nodes()[hold];
    d.check(
        "Nested duration editing changes only the Hold and keeps live parents intact",
        matches!(&resized_hold.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 17)
            && resized.duration().unwrap().frames() == before.duration().unwrap().frames() + 6
            && resized.nodes().len() == before.nodes().len()
            && before
                .nodes()
                .iter()
                .all(|(id, node)| id == hold || resized.nodes().get(id) == Some(node)),
        json!({"hold_frames": 17, "duration_delta": 6, "other_nodes": "unchanged"}),
        json!({"hold": resized_hold, "frames": resized.duration().unwrap().frames()}),
    )?;
    check_scope(
        d,
        "Duration commit retains the entered scope and selected Hold",
        &groups,
        hold,
        17,
    )?;
    let resized_revision = d.revision();
    d.key(egui::Key::U)?;
    d.changed(&resized_revision)?;
    let undone = d.app().workspace.as_ref().unwrap().document.clone();
    d.check(
        "Scoped duration undo restores exact nodes and audio bindings",
        undone.nodes() == before.nodes() && undone.audio_bindings() == before.audio_bindings(),
        json!("state before duration edit"),
        json!({"nodes_restored": undone.nodes() == before.nodes(), "bindings_restored": undone.audio_bindings() == before.audio_bindings()}),
    )?;
    check_scope(
        d,
        "Duration undo retains the entered scope and selected Hold",
        &groups,
        hold,
        17,
    )?;
    let undo_revision = d.revision();
    d.key_modified(
        egui::Key::Z,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )?;
    d.changed(&undo_revision)?;
    let redone = d.app().workspace.as_ref().unwrap().document.clone();
    d.check(
        "Scoped duration redo restores exact nodes and audio bindings",
        redone.nodes() == resized.nodes() && redone.audio_bindings() == resized.audio_bindings(),
        json!("state after duration edit"),
        json!({"nodes_restored": redone.nodes() == resized.nodes(), "bindings_restored": redone.audio_bindings() == resized.audio_bindings()}),
    )?;
    check_scope(
        d,
        "Duration redo retains the entered scope and selected Hold",
        &groups,
        hold,
        17,
    )?;

    let camera_revision = d.revision();
    let entry = d.app().presentation.diagnostic_snapshot();
    d.chord(&[egui::Key::Comma, egui::Key::F])?;
    d.wait_for("Camera opens for the selected nested Hold", |app| {
        app.camera.is_some()
    })?;
    d.chord(&[egui::Key::Num3, egui::Key::Backspace])?;
    d.check(
        "Camera owns Backspace without leaving the nested group or committing",
        d.app().camera.is_some()
            && d.app().sequence_scope.groups() == groups
            && d.revision() == camera_revision,
        json!({"camera_open": true, "groups": groups, "revision": camera_revision}),
        d.snapshot(),
    )?;
    d.key(egui::Key::L)?;
    d.settled()?;
    let draft = d.app().presentation.diagnostic_snapshot();
    d.check(
        "Camera previews the nested Hold without an authored edit",
        d.revision() == camera_revision
            && d.app().camera.is_some()
            && draft["decoded_framing"] != entry["decoded_framing"]
            && draft["geometry_revision"] == draft["decoded_geometry_revision"],
        json!({"revision": camera_revision, "preview": "changed submitted framing"}),
        draft,
    )?;
    d.key(egui::Key::Enter)?;
    d.changed(&camera_revision)?;
    let framed = d.app().workspace.as_ref().unwrap().document.clone();
    let mut expected_hold = resized.nodes()[hold].clone();
    expected_hold.framing = framed.nodes()[hold].framing.clone();
    d.check(
        "Camera commits only the nested Hold framing and retains its captured picture recipe",
        d.app().camera.is_none()
            && framed.nodes()[hold].framing.is_some()
            && framed.nodes()[hold] == expected_hold
            && framed.nodes().len() == resized.nodes().len()
            && resized
                .nodes()
                .iter()
                .all(|(id, node)| id == hold || framed.nodes().get(id) == Some(node))
            && framed.audio_bindings() == resized.audio_bindings(),
        json!({"framing_owner": hold, "captured_recipe_and_other_nodes": "unchanged"}),
        json!({"hold": framed.nodes()[hold], "cursor": d.app().sequence_cursor}),
    )?;
    check_scope(
        d,
        "Camera commit retains the nested Hold context",
        &groups,
        hold,
        17,
    )?;
    d.capture("Nested Hold Camera result with live parent groups")?;

    d.key(egui::Key::Backspace)?;
    d.settled()?;
    check_scope(
        d,
        "Backspace returns to Outer and selects the exited Inner group",
        std::slice::from_ref(outer),
        inner,
        17,
    )?;
    d.key(egui::Key::Backspace)?;
    d.settled()?;
    check_scope(
        d,
        "Backspace returns to Your edit without moving the project cursor",
        &[],
        outer,
        17,
    )?;
    d.capture("Returned to Your edit at the nested pause")?;

    // A group endpoint resolves to an enclosing Sequence in core. The native
    // scope guard must refuse it explicitly instead of inserting elsewhere.
    d.key(egui::Key::Enter)?;
    d.key(egui::Key::Enter)?;
    d.key(egui::Key::Home)?;
    d.settled()?;
    let endpoint = d.app().workspace.as_ref().unwrap().document.clone();
    let endpoint_revision = d.revision();
    d.check(
        "Home reaches the entered group's absolute start without leaving it",
        d.app().sequence_scope.groups() == groups && d.app().sequence_cursor == 0,
        json!({"groups": groups, "cursor": 0}),
        json!({"groups": d.app().sequence_scope.groups(), "cursor": d.app().sequence_cursor}),
    )?;
    d.command("hold 3f")?;
    d.settled()?;
    let current = d.app().workspace.as_ref().unwrap().document.clone();
    let guidance = d
        .app()
        .error
        .as_deref()
        .or(d.app().project_error.as_deref())
        .unwrap_or("")
        .to_owned();
    d.check(
        "A group-endpoint pause is refused with Backspace guidance and no edit",
        d.revision() == endpoint_revision
            && current == endpoint
            && d.app().sequence_scope.groups() == groups
            && d.app().sequence_cursor == 0
            && guidance.contains("Backspace"),
        json!({"revision": endpoint_revision, "document": "unchanged", "guidance": "Backspace"}),
        json!({"revision": d.revision(), "document_unchanged": current == endpoint, "guidance": guidance}),
    )?;
    d.capture("Group endpoint refusal explains how to return")?;
    d.key(egui::Key::Backspace)?;
    d.settled()?;
    check_scope(
        d,
        "Backspace follows endpoint guidance into the parent scope",
        std::slice::from_ref(outer),
        inner,
        0,
    )?;
    d.key(egui::Key::Backspace)?;
    d.settled()?;
    check_scope(
        d,
        "Endpoint return reaches Your edit without changing history",
        &[],
        outer,
        0,
    )?;
    d.check(
        "Group navigation leaves the refused revision unchanged",
        d.revision() == endpoint_revision,
        json!(endpoint_revision),
        json!(d.revision()),
    )
}

fn check_scope(
    d: &mut Driver<'_>,
    label: &str,
    groups: &[NodeId],
    selected: &NodeId,
    cursor: u64,
) -> Result<(), String> {
    let workspace = d.app().workspace.as_ref().unwrap();
    let document = &workspace.document;
    let parent = groups.last().unwrap_or(document.root());
    let expected_rows = document.children(parent).cloned().collect::<Vec<_>>();
    let actual_rows = d
        .app()
        .beat_rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    let mut breadcrumbs = vec!["Your edit".to_owned()];
    breadcrumbs.extend(groups.iter().map(|id| document.nodes()[id].label.clone()));
    let widgets = d.widgets();
    let visible_breadcrumbs = breadcrumbs.iter().all(|label| {
        widgets.as_array().is_some_and(|widgets| {
            widgets.iter().any(|widget| {
                widget["label"] == *label
                    && widget["role"] == "Button"
                    && widget["hidden"] == false
                    && !widget["rect"].is_null()
            })
        })
    });
    d.check(
        label,
        d.app().sequence_scope.groups() == groups
            && d.app().selected_beat.as_ref() == Some(selected)
            && d.app().sequence_cursor == cursor
            && actual_rows == expected_rows
            && visible_breadcrumbs,
        json!({"groups": groups, "selected": selected, "cursor": cursor, "rows": expected_rows, "breadcrumbs": breadcrumbs}),
        json!({"groups": d.app().sequence_scope.groups(), "selected": d.app().selected_beat, "cursor": d.app().sequence_cursor, "rows": actual_rows, "breadcrumbs_visible": visible_breadcrumbs}),
    )
}

fn apply(
    store: &mut ProjectStore,
    revision: &str,
    command: Command,
) -> Result<ProjectDocument, String> {
    let document = store.snapshot().map_err(|error| error.to_string())?;
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        command,
        new_revision: RevisionId::new(revision).map_err(|error| error.to_string())?,
    };
    store.commit(&request).map_err(|error| error.to_string())?;
    store.snapshot().map_err(|error| error.to_string())
}
