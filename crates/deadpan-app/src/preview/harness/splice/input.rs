//! Whole native input batches and distinct slots sharing an Edit boundary.

use std::collections::BTreeMap;

use deadpan_core::{
    BeatNode, Command, CommandRequest, NodeKind, ProjectDocument, RevisionId, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let mut failures = Vec::new();
    batched_count(d, &[Key::Num1, Key::Num1, Key::L], 41, &mut failures)?;
    batched_count(d, &[Key::Num1, Key::Num2, Key::H], 18, &mut failures)?;
    empty_seam_fixture(d)?;
    seam_commit(d, false, &mut failures)?;
    seam_commit(d, true, &mut failures)?;
    cancellation_batch(d, false, &mut failures)?;
    cancellation_batch(d, true, &mut failures)?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn batched_count(
    d: &mut Driver<'_>,
    keys: &[Key],
    expected: i64,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    let saved = document(d)?.clone();
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::D)?;
    d.check(
        "Batched count regression starts at an interior Destination boundary",
        prepared(d)?.range.start() == ProjectFrame(30)
            && matches!(
                draft(d)?.proposal_for_check().destination,
                Destination::Interior { .. }
            ),
        json!({"destination":30,"focus":"Destination"}),
        state(d),
    )?;
    // Unlike Driver::chord, this preserves all presses in one native batch.
    // Include releases as the platform does; neither duplicate digits nor the
    // final motion may be swallowed by widget consumption or layout retries.
    let events = keys
        .iter()
        .flat_map(|key| {
            [
                key_event(*key, Modifiers::NONE, true),
                key_event(*key, Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events(
        &format!("Deliver one native slice key batch {keys:?}"),
        events,
    )?;
    wait_ready(d)?;
    record(failures, d.check(
        &format!("One native batch {keys:?} applies its entire count and motion once"),
        prepared(d)?.range.start() == ProjectFrame(expected)
            && *document(d)? == saved
            && matches!(draft(d)?.proposal_for_check().destination, Destination::Interior { .. }),
        json!({"destination":expected,"saved_revision":saved.revision_id(),"batch":format!("{keys:?}")}),
        state(d),
    ));
    d.key(Key::Escape)?;
    d.wait_for("Cancel the batched-count draft without saving", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Batched count inspection leaves the complete saved document unchanged",
        *document(d)? == saved,
        json!(saved.revision_id()),
        json!(document(d)?.revision_id()),
    )
}

fn empty_seam_fixture(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("No slice input fixture")?;
    let path = workspace.path.clone();
    let original = workspace.document.clone();
    let children = root_children(&original)?;
    if children.len() != 1
        || !matches!(original.nodes()[&children[0]].kind, NodeKind::Source { .. })
    {
        return Err("Slice input replay requires the restored full Original Source".into());
    }
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Close the service writer before adding empty seam fixtures",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)
        .map_err(|error| format!("Open slice input fixture: {error}"))?;
    for (name, leading) in [
        ("slice-input-leading-empty", true),
        ("slice-input-trailing-empty", false),
    ] {
        let before = store.snapshot().map_err(|error| error.to_string())?;
        let id = NodeId::new(name).map_err(|error| error.to_string())?;
        let index = if leading {
            0
        } else {
            root_children(&before)?.len()
        };
        store
            .commit(&CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new(format!("{name}-revision"))
                    .map_err(|error| error.to_string())?,
                command: Command::Insert {
                    parent: before.root().clone(),
                    index,
                    subtree: Subtree {
                        root: id.clone(),
                        nodes: BTreeMap::from([(id, BeatNode::sequence(name, vec![]))]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            })
            .map_err(|error| error.to_string())?;
    }
    let fixture = store.snapshot().map_err(|error| error.to_string())?;
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for(
        "Reopen the Original with leading and trailing empty Sequences",
        |app| {
            !app.service.is_busy()
                && !app.importing()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.path == path)
        },
    )?;
    d.check(
        "Empty seam fixture reopens through the real service without changing its source",
        *document(d)? == fixture && fixture.assets() == original.assets(),
        json!({"revision":fixture.revision_id(),"children":root_children(&fixture)?}),
        d.snapshot(),
    )?;
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for(
        "Original copy is durably saved before the next operation",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.settled()?;
    d.check(
        "Reopened seam fixture copies the same qualified Original interval",
        copied(d) == Some(10..24) && *document(d)? == fixture,
        json!({"copied":[10,24],"revision":fixture.revision_id()}),
        d.snapshot(),
    )
}

fn seam_commit(
    d: &mut Driver<'_>,
    trailing: bool,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let before = document(d)?.clone();
    let revision = d.revision();
    let slot = if trailing { 3 } else { 1 };
    let boundary = if trailing {
        before
            .duration()
            .map_err(|error| error.to_string())?
            .frames()
    } else {
        0
    };
    d.command("splice")?;
    wait_ready(d)?;
    if trailing {
        // Slot 2 is before the trailing empty group at the terminal boundary.
        d.chord(&[Key::Num2, Key::J])?;
        wait_ready(d)?;
    }
    d.key(Key::J)?;
    wait_ready(d)?;
    d.check(
        "Next seam selects the distinct slot after an empty Sequence",
        draft(d)?.proposal_for_check().destination == Destination::Slot(slot)
            && prepared(d)?.range.start() == ProjectFrame(boundary),
        json!({"slot":slot,"boundary":boundary,"trailing":trailing}),
        state(d),
    )?;
    d.key(if trailing { Key::L } else { Key::H })?;
    wait_ready(d)?;
    record(
        failures,
        d.check(
            "Clamped frame motion preserves the selected slot after an empty Sequence",
            draft(d)?.proposal_for_check().destination == Destination::Slot(slot)
                && prepared(d)?.range.start() == ProjectFrame(boundary),
            json!({"slot":slot,"boundary":boundary,"trailing":trailing}),
            state(d),
        ),
    );
    let proposal = prepared(d)?;
    let mut expected_children = root_children(&before)?.to_vec();
    expected_children.insert(slot, proposal.node.clone());
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    let committed_revision = d.revision();
    record(failures, d.check(
        "Enter commits the exact after-empty placement without changing Sequence ownership",
        d.app().splice.is_none()
            && *document(d)? == *proposal.snapshot.document
            && root_children(document(d)?)? == expected_children
            && d.app().selected_beat.as_ref() == Some(&proposal.node),
        json!({"children":expected_children,"revision":proposal.snapshot.document.revision_id(),"trailing":trailing}),
        d.snapshot(),
    ));
    d.key(Key::U)?;
    d.changed(&committed_revision)?;
    let mut restored = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    // Durable undo deliberately allocates a new revision. Every authored field
    // must otherwise equal the exact document before this one placement.
    restored["revision_id"] = json!(before.revision_id());
    d.check(
        "One undo restores the complete empty-seam fixture and copied Original",
        restored == serde_json::to_value(&before).map_err(|error| error.to_string())?
            && copied(d) == Some(10..24),
        json!({"children":root_children(&before)?,"copied":[10,24],"trailing":trailing}),
        d.snapshot(),
    )
}

fn cancellation_batch(
    d: &mut Driver<'_>,
    native_button: bool,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    d.command("splice")?;
    wait_ready(d)?;
    focus_with_tab(d, if native_button { CANCEL } else { HEADING })?;
    let keys = [
        Key::U,
        if native_button {
            Key::Enter
        } else {
            Key::Escape
        },
    ];
    let events = keys
        .into_iter()
        .flat_map(|key| {
            [
                key_event(key, Modifiers::NONE, true),
                key_event(key, Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events(
        if native_button {
            "Deliver U then Enter to the focused native slice Cancel button in one batch"
        } else {
            "Deliver U then Escape to the slice heading in one batch"
        },
        events,
    )?;
    d.wait_for(
        "Terminal slice cancellation releases the draft and service",
        |app| app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy(),
    )?;
    d.settled()?;
    record(failures, d.check(
        if native_button {
            "Native Cancel activation cannot replay its earlier U as editor Undo on a layout retry"
        } else {
            "Heading Escape cannot replay its earlier U as editor Undo on a layout retry"
        },
        d.app().splice.is_none() && d.revision() == revision && *document(d)? == saved,
        json!({"revision":revision,"document_unchanged":true,"native_button":native_button}),
        json!({"revision":d.revision(),"document_unchanged":*document(d)? == saved,"state":d.snapshot()}),
    ));
    Ok(())
}

fn root_children(document: &ProjectDocument) -> Result<&[NodeId], String> {
    match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => Ok(children),
        _ => Err("Slice input fixture root is not a Sequence".into()),
    }
}

fn record(failures: &mut Vec<String>, result: Result<(), String>) {
    if let Err(error) = result {
        failures.push(error);
    }
}
