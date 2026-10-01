use super::*;
use deadpan_core::{Command, CommandRequest, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    let revision = d.revision();
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L, Key::S])?;
    d.changed(&revision)?;
    let path = d.app().workspace.as_ref().unwrap().path.clone();
    let mut selected = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Missing split suffix")?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Release writer for nested deletion fixture", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let inner = NodeId::new("delete-inner").map_err(|error| error.to_string())?;
    let outer = NodeId::new("delete-outer").map_err(|error| error.to_string())?;
    for id in [&inner, &outer] {
        let document = store.snapshot().map_err(|error| error.to_string())?;
        let children = document
            .children(document.root())
            .cloned()
            .collect::<Vec<_>>();
        let start = children
            .iter()
            .position(|child| child == &selected)
            .ok_or("Missing grouping child")?;
        store
            .commit(&CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("{id}-fixture"))
                    .map_err(|error| error.to_string())?,
                command: Command::Group {
                    parent: document.root().clone(),
                    start,
                    end: start + 1,
                    id: id.clone(),
                    label: format!("{id} group"),
                },
            })
            .map_err(|error| error.to_string())?;
        selected = id.clone();
    }
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Open nested deletion fixture", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    d.command("sequence")?;
    select(d, 30, 60, true)?;
    let saved = document(d)?.clone();
    let selection = d.app().edit_range.clone();
    d.key(Key::D)?;
    d.wait_for(
        "Unsupported composite endpoint is rejected by the project service",
        |app| !app.service.is_busy() && app.project_error.is_some(),
    )?;
    d.check(
        "A partial composite endpoint fails without losing the selected range",
        *document(d)? == saved
            && d.app().edit_range == selection
            && d.app().project_error.is_some(),
        json!("enter group before cutting its interior"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.chord(&[Key::Enter, Key::Enter])?;
    d.check(
        "Deletion enters an ordinary nested group at its absolute origin",
        d.app().sequence_scope.groups() == [outer.clone(), inner.clone()]
            && d.app().scope_start == 20,
        json!({"scope":[outer,inner],"start":20}),
        d.snapshot(),
    )?;
    select(d, 30, 60, false)?;
    let scope = d.app().sequence_scope.clone();
    let revision = d.revision();
    d.key(Key::D)?;
    d.changed(&revision)?;
    d.check(
        "Range deletion preserves its captured nested scope and exact join",
        d.app().sequence_scope == scope
            && d.app().sequence_cursor == 30
            && d.app().sequence_length() == 90
            && d.app().scope_start == 20
            && d.app().scope_end == 90,
        json!({"cursor":30,"scope_start":20,"scope_end":90}),
        d.snapshot(),
    )?;
    picture(d, 60)?;
    d.key(Key::H)?;
    picture(d, 29)?;
    undo(d, &saved)?;

    select(d, 30, 60, true)?;
    d.key(Key::Backspace)?;
    let saved = document(d)?.clone();
    d.key(Key::D)?;
    d.check(
        "Leaving the selected scope discards the range and cannot silently cut its parent",
        d.app().selected_edit_range().is_none()
            && *document(d)? == saved
            && d.app().bindings.pending() == "d",
        json!("selection invalidated, whole-beat prefix only"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.key(Key::Backspace)?;
    select(d, 20, 120, true)?;
    let revision = d.revision();
    d.command("delete")?;
    d.changed(&revision)?;
    d.check(
        "A range may remove a complete nested composite at its seams",
        d.app().sequence_length() == 20
            && !document(d)?.nodes().contains_key(&outer)
            && !document(d)?.nodes().contains_key(&inner)
            && d.app().sequence_cursor == 20,
        json!({"frames":20,"cursor":20,"groups_removed":true}),
        d.snapshot(),
    )?;
    undo(d, &saved)
}
