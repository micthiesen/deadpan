//! Production ig/ag ownership, copied provenance and exact Place continuation.

use super::*;
use crate::project::splice::{Destination, Prepared};
use SemanticTextObject::{AroundGroup, InnerGroup};
use deadpan_core::{
    FrameRange, ProjectFrame, SemanticObjectSelection, SemanticTextObject, SemanticVisualSelection,
    SliceCaptureSelection,
};

const LABEL: &str = "Object group";

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    visual(d, 20, true)?;
    let before = d.revision();
    d.command("group name=\"Object group\"")?;
    d.changed(&before)?;
    let group = grouped(d, LABEL, 20, 20)?;
    let grouped_document = document(d)?.clone();
    copies(d, &group)?;
    selections(d, &group)?;
    place(d, &group, &grouped_document)?;
    cuts_and_empty_paste(d, &group, &grouped_document)?;
    recorded(d, &group, &grouped_document)?;
    outside(d)?;
    undo(d, baseline)?;
    d.check(
        "Object replay restores the complete protected baseline with no remaining authored history",
        same_document(document(d)?, baseline)? && !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        object_state(d),
    )
}

fn copies(d: &mut Driver<'_>, group: &NodeId) -> Result<(), String> {
    for inside in [false, true] {
        if inside {
            d.key(Key::Enter)?;
            d.settled()?;
            d.key(Key::L)?;
            d.settled()?;
        }
        for kind in [InnerGroup, AroundGroup] {
            let before = document(d)?.clone();
            let before_bank = bank(d)?;
            let entry_scope = d.app().sequence_scope.clone();
            let entry_cursor = d.app().sequence_cursor;
            let entry_child = d.app().selected_beat.clone();
            if kind == InnerGroup {
                choose_a(d)?;
            }
            d.chord(&[
                Key::Y,
                if kind == InnerGroup { Key::I } else { Key::A },
                Key::G,
            ])?;
            idle(d)?;
            check_capture(d, group, kind, &before)?;
            let saved = bank(d)?;
            let named = RegisterName::new('a').map_err(|error| error.to_string())?;
            d.check(
                "yig/yag preserve independent cursor, explicit beat, scope and authored revision",
                document(d)? == &before && d.app().sequence_scope == entry_scope
                    && d.app().sequence_cursor == entry_cursor && d.app().selected_beat == entry_child
                    && saved.version == before_bank.version + 1
                    && d.app().copied.selected_override().is_none()
                    && (kind != InnerGroup || saved.entries.get(&named) == saved.entries.get(&RegisterName::unnamed())),
                json!({"inside":inside,"object":kind,"cursor":entry_cursor,"bank_version":before_bank.version+1}),
                object_state(d),
            )?;
        }
    }
    Ok(())
}

fn selections(d: &mut Driver<'_>, group: &NodeId) -> Result<(), String> {
    let before = document(d)?.clone();
    d.chord(&[Key::V, Key::I, Key::G])?;
    d.settled()?;
    check_object(d, group, InnerGroup, true)?;
    d.key(Key::V)?;
    let finished = d.app().edit_range.clone();
    d.key(Key::H)?;
    d.settled()?;
    d.check(
        "Finishing vig retains its exact group ownership while an independent frame motion moves the cursor",
        d.app().edit_range == finished && d.app().sequence_cursor == 39
            && d.app().selected_edit_range() == Some(range(20, 40))
            && document(d)? == &before,
        json!({"object":"inner_group","extending":false,"cursor":39}), object_state(d),
    )?;
    d.key(Key::Escape)?;
    d.chord(&[Key::V, Key::I, Key::G, Key::H])?;
    d.settled()?;
    d.check(
        "Moving an extending vig converts ownership to an oriented Time selection",
        d.app().capture_visual_selection()?
            == Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(20),
                head: ProjectFrame(39),
                extending: true,
            })
            && d.app().edit_range.object().is_none()
            && document(d)? == &before,
        json!({"type":"time","anchor":20,"head":39,"extending":true}),
        object_state(d),
    )?;
    d.key(Key::Escape)?;
    d.chord(&[Key::V, Key::A, Key::G])?;
    d.settled()?;
    check_object(d, group, AroundGroup, true)?;
    d.capture("Whole-group object selected from inside its ordinary scope")
}

fn place(
    d: &mut Driver<'_>,
    group: &NodeId,
    grouped_document: &ProjectDocument,
) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = d.app().edit_range.clone();
    let scope = d.app().sequence_scope.clone();
    let child = d.app().selected_beat.clone();
    let cursor = d.app().sequence_cursor;
    let before_bank = bank(d)?;
    // Fault injection changes only this captured request's eligibility. The
    // Object selection must not fall through to a time-range paste.
    let mut unavailable = d.app().capture_placement_target()?;
    unavailable.macro_capture = Err("Injected unavailable object context".into());
    let handled = d.app_mut().record_macro_paste(false, &Ok(unavailable));
    d.check(
        "A failed captured Object context refuses paste without a time-range fallback",
        handled && !d.app().macros.is_pending() && !d.app().service.is_busy()
            && d.app().error.as_deref() == Some("Injected unavailable object context")
            && d.app().edit_range == entry && document(d)? == &saved && bank(d)? == before_bank,
        json!({"refused":true,"authored_and_registers_unchanged":true,"injection":"captured request eligibility only"}),
        object_state(d),
    )?;
    for commit in [false, true] {
        d.command("splice")?;
        // The copy starts with Original frame 20 at the retained seam 40.
        ready(d, 40, 20)?;
        d.key(Key::R)?;
        // Exact ag replacement starts at Edit 20 with the same source frame.
        ready(d, 20, 20)?;
        let proposed = prepared(d)?;
        let selected = SemanticObjectSelection {
            kind: AroundGroup,
            group: group.clone(),
        };
        let destination = d
            .app()
            .splice
            .as_ref()
            .ok_or("Missing Place draft")?
            .proposal_for_check()
            .destination
            .clone();
        d.check(
            "Place r previews exact ag replacement without saving or losing the captured scope",
            matches!(destination, Destination::Object { selection } if selection == selected)
                && proposed.object.as_ref() == Some(&selected)
                && proposed.removed == Some(range(20, 40)) && proposed.range == range(20, 40)
                && proposed.navigation_parent == *group && proposed.parent == *saved.root()
                && proposed.continuation_scope.groups().is_empty()
                && !proposed.snapshot.document.nodes().contains_key(group)
                && document(d)? == &saved && bank(d)? == before_bank
                && d.app().sequence_scope == scope && d.app().sequence_cursor == cursor,
            json!({"removed":[20,40],"inserted":[20,40],"final_scope":"root","saved_unchanged":true}),
            object_state(d),
        )?;
        let caption = "Replace whole group: Your edit / Object group · Edit [20..40)";
        // Galley-backed Label widgets expose their text as an accessibility
        // value, while buttons queried through Driver::rect use labels.
        let accessible = d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            (access.label().as_deref() == Some(caption)
                || access.value().as_deref() == Some(caption))
                && !access.is_hidden()
        });
        d.check(
            "Place names the exact group and range being replaced",
            accessible,
            json!(caption),
            object_state(d),
        )?;
        painted(d, caption)?;
        if !commit {
            d.key(Key::F)?;
            proposed_picture(d, 20, 20)?;
            d.capture("Unsaved exact whole-group replacement with captured Object ownership")?;
            d.key(Key::Escape)?;
            d.wait_for("Object Place proposal is abandoned", |app| {
                app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
            })?;
            d.settled()?;
            d.check(
                "Cancel restores the exact Object selection, scope, cursor, child and copied bank",
                d.app().edit_range == entry
                    && d.app().sequence_scope == scope
                    && d.app().sequence_cursor == cursor
                    && d.app().selected_beat == child
                    && document(d)? == &saved
                    && bank(d)? == before_bank,
                json!({"object_restored":true,"revision":saved.revision_id()}),
                object_state(d),
            )?;
        } else {
            let before = d.revision();
            d.key(Key::Enter)?;
            d.changed(&before)?;
            idle(d)?;
            d.check(
                "Committing an inside ag Place proposal publishes its prepared outer-scope continuation",
                d.app().splice.is_none() && document(d)? == proposed.snapshot.document.as_ref()
                    && d.app().sequence_scope == proposed.continuation_scope
                    && d.app().sequence_scope.groups().is_empty()
                    && d.app().selected_beat.as_ref() == Some(&proposed.continuation_node)
                    && d.app().sequence_cursor == 20 && !d.app().edit_range.has_bounds()
                    && bank(d)? == before_bank,
                json!({"scope":"root","selected":proposed.continuation_node,"Edit":20}), object_state(d),
            )?;
            undo(d, grouped_document)?;
        }
    }
    Ok(())
}

fn cuts_and_empty_paste(
    d: &mut Driver<'_>,
    group: &NodeId,
    grouped_document: &ProjectDocument,
) -> Result<(), String> {
    outside(d)?;
    at(d, 20)?;
    d.key(Key::Enter)?;
    d.settled()?;
    let before = d.revision();
    d.chord(&[Key::D, Key::A, Key::G])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "dag from inside removes its exact group and continues at the following outer child",
        !document(d)?.nodes().contains_key(group)
            && d.app().sequence_scope.groups().is_empty()
            && d.app().sequence_cursor == 20
            && d.app().sequence_length() == 100
            && d.app().selected_beat.as_ref()
                == grouped_document.children(grouped_document.root()).last()
            && !d.app().edit_range.has_bounds(),
        json!({"scope":"root","Edit":20,"frames":100}),
        object_state(d),
    )?;
    check_capture(d, group, AroundGroup, grouped_document)?;
    undo(d, grouped_document)?;
    at(d, 20)?;
    d.key(Key::Enter)?;
    d.settled()?;
    let before = d.revision();
    d.chord(&[Key::D, Key::I, Key::G])?;
    d.changed(&before)?;
    idle(d)?;
    let empty = document(d)?.clone();
    d.check(
        "dig from inside retains an explicit empty group and its navigation scope",
        empty.nodes().contains_key(group)
            && empty.children(group).next().is_none()
            && d.app().sequence_scope.groups() == [group.clone()]
            && d.app().scope_start == 20
            && d.app().scope_end == 20
            && d.app().sequence_cursor == 20
            && d.app().selected_beat.is_none()
            && d.app().sequence_length() == 100,
        json!({"group":group,"scope":[20,20],"selected":null}),
        object_state(d),
    )?;
    check_capture(d, group, InnerGroup, grouped_document)?;
    let cut_bank = bank(d)?;
    d.chord(&[Key::V, Key::I, Key::G])?;
    d.settled()?;
    check_object(d, group, InnerGroup, true)?;
    d.check(
        "Empty vig remains an Object target rather than absent selection",
        d.app().edit_range.has_bounds() && d.app().selected_edit_range() == Some(range(20, 20)),
        json!({"type":"object","range":[20,20]}),
        object_state(d),
    )?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "p into empty ig inserts the copied forest at slot zero and keeps inside navigation",
        d.app().sequence_scope.groups() == [group.clone()]
            && document(d)?.children(group).count() == 1
            && d.app().selected_beat.as_ref() == document(d)?.children(group).next()
            && d.app().sequence_cursor == 20
            && d.app().scope_end == 40
            && d.app().sequence_length() == 120
            && !d.app().edit_range.has_bounds()
            && bank(d)? == cut_bank,
        json!({"scope":[20,40],"frames":120,"Visual":false}),
        object_state(d),
    )?;
    undo(d, &empty)?;
    undo(d, grouped_document)
}

fn recorded(
    d: &mut Driver<'_>,
    group: &NodeId,
    grouped_document: &ProjectDocument,
) -> Result<(), String> {
    outside(d)?;
    at(d, 20)?;
    let before = d.revision();
    d.chord(&[Key::Q, Key::M])?;
    d.chord(&[Key::Y, Key::I, Key::G])?;
    idle(d)?;
    d.chord(&[Key::V, Key::A, Key::G, Key::V])?;
    d.settled()?;
    check_object(d, group, AroundGroup, false)?;
    d.key(Key::Q)?;
    d.wait_for("Object macro is saved", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    idle(d)?;
    let saved = bank(d)?;
    let expected = [
        SemanticInstruction::Yank {
            selector: SemanticSelector::TextObject { object: InnerGroup },
            register: RegisterName::unnamed(),
        },
        SemanticInstruction::BeginSelection,
        SemanticInstruction::SelectObject {
            object: AroundGroup,
        },
        SemanticInstruction::FinishSelection,
    ];
    d.check(
        "Recording stores object kinds and finished Visual intent without old node identities",
        matches!(saved.entries.get(&RegisterName::new('m').map_err(|error| error.to_string())?).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if program.instructions() == expected)
            && d.revision() == before && same_document(document(d)?, grouped_document)?,
        json!(expected), object_state(d),
    )?;
    d.key(Key::Escape)?;
    at(d, 22)?;
    let input = vec![
        key_event(Key::Num2, Modifiers::SHIFT, true),
        Event::Text("@".into()),
        key_event(Key::Num2, Modifiers::SHIFT, false),
        key_event(Key::M, Modifiers::NONE, true),
        key_event(Key::M, Modifiers::NONE, false),
    ];
    d.events(
        "Replay semantic group objects from a fresh independent cursor",
        input,
    )?;
    idle(d)?;
    check_object(d, group, AroundGroup, false)?;
    let captured_document = document(d)?.clone();
    check_capture(d, group, InnerGroup, &captured_document)?;
    d.check(
        "Bank-only object macro replay leaves authored history unchanged and restores the finished object at its new resolution",
        d.revision() == before && same_document(document(d)?, grouped_document)?
            && d.app().sequence_scope.groups().is_empty() && d.app().sequence_cursor == 40
            && bank(d)?.version == saved.version + 1,
        json!({"revision":before,"cursor":40,"bank_version":saved.version+1}), object_state(d),
    )?;
    d.key(Key::Escape)
}

fn check_object(
    d: &mut Driver<'_>,
    group: &NodeId,
    kind: SemanticTextObject,
    extending: bool,
) -> Result<(), String> {
    let expected = SemanticVisualSelection::Object {
        selection: SemanticObjectSelection {
            kind,
            group: group.clone(),
        },
        extending,
    };
    let actual = d.app().capture_visual_selection()?;
    let label = d.app().edit_range_label();
    let cue = if kind == InnerGroup {
        "Group contents"
    } else {
        "Whole group"
    };
    d.check(
        "ig/ag retain typed ownership and expose the matching visible cue",
        actual == Some(expected.clone())
            && [EditorKey::PasteAfter, EditorKey::PasteBefore]
                .into_iter()
                .all(|key| {
                    d.rect(&format!("Replace object  {}", d.app().editor_key(key)))
                        .is_ok()
                })
            && label.as_ref().is_some_and(|label| label.starts_with(cue))
            && scenarios::text_paint_visibility(d, cue)
                .iter()
                .any(|part| part["fully_visible"] == true),
        json!(expected),
        object_state(d),
    )
}

fn check_capture(
    d: &mut Driver<'_>,
    group: &NodeId,
    kind: SemanticTextObject,
    captured_document: &ProjectDocument,
) -> Result<(), String> {
    let durable = bank(d)?;
    let value = durable
        .entries
        .get(&RegisterName::unnamed())
        .ok_or("Missing unnamed object copy")?;
    let RegisterValue::Edited { slice } = value.as_ref() else {
        return Err("Object copy did not produce Edited contents".into());
    };
    let Content::Edited(runtime) = d
        .app()
        .copied
        .content()
        .ok_or("Missing runtime object copy")?
    else {
        return Err("Object runtime copy is not Edited contents".into());
    };
    let (parent, expected, bounds, scope, labels) = if kind == InnerGroup {
        let children = captured_document
            .children(group)
            .cloned()
            .collect::<Vec<_>>();
        (
            group.clone(),
            SliceCaptureSelection::Children {
                first: children.first().ok_or("Missing first contents")?.clone(),
                last: children.last().ok_or("Missing last contents")?.clone(),
            },
            range(20, 40),
            vec![group.clone()],
            vec!["Your edit".to_owned(), LABEL.to_owned()],
        )
    } else {
        (
            captured_document.root().clone(),
            SliceCaptureSelection::Child {
                node: group.clone(),
            },
            range(0, 120),
            vec![],
            vec!["Your edit".to_owned()],
        )
    };
    let valid = slice.parent() == &parent
        && slice.selection() == &expected
        && slice.range() == range(20, 40)
        && runtime.slice().as_ref() == slice.as_ref()
        && runtime.bounds() == bounds
        && runtime.scope().groups() == scope
        && runtime.source_path() == labels
        && runtime.id().source_revision == *slice.revision_id()
        && runtime.child_label() == (kind == AroundGroup).then_some(LABEL);
    let actual = json!({"selection":slice.selection(),"parent":slice.parent(),"bounds":runtime.bounds(),"scope":runtime.scope().groups(),"source_path":runtime.source_path(),"label":runtime.child_label(),"timing":slice.capture_timing()});
    d.check("Object copy publishes exact durable content and its historical runtime parent/path metadata", valid,
        json!({"parent":parent,"selection":expected,"bounds":bounds,"scope":scope,"path":labels}), actual)
}

fn outside(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().edit_range.has_bounds() {
        d.key(Key::Escape)?;
    }
    for _ in 0..deadpan_core::MAX_DOCUMENT_DEPTH {
        if d.app().sequence_scope.groups().is_empty() {
            return Ok(());
        }
        d.key(Key::Backspace)?;
        d.settled()?;
    }
    Err("Object replay could not return to root scope".into())
}
fn ready(d: &mut Driver<'_>, frame: i64, source: u64) -> Result<(), String> {
    d.wait_for("Exact Object Place proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .splice
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
    })?;
    proposed_picture(d, frame, source)
}

fn proposed_picture(d: &mut Driver<'_>, frame: i64, source: u64) -> Result<(), String> {
    let prepared = prepared(d)?;
    let id = d
        .app()
        .splice
        .as_ref()
        .ok_or("Missing Object Place draft")?
        .proposal_for_check()
        .id
        .clone();
    let expected_content = deadpan_playback::ContentIdentity::Proposed {
        base_revision: id.base_revision.clone(),
        draft: id.draft,
        change: id.change,
    };
    let snapshot = &prepared.snapshot;
    d.check(
        "Object Place picture belongs to the current proposal and its saved base",
        snapshot.session == id.session
            && snapshot.document.project_id() == &id.project
            && snapshot.document.project_id() == document(d)?.project_id()
            && snapshot.document.revision_id() != &id.base_revision
            && snapshot.content == expected_content
            && document(d)?.revision_id() == &id.base_revision,
        json!({"session":id.session,"base_revision":id.base_revision,"content":format!("{expected_content:?}")}),
        json!({"session":snapshot.session,"revision":snapshot.document.revision_id(),"content":format!("{:?}", snapshot.content)}),
    )?;
    let matches = |app: &DeadpanApp| {
        app.splice
            .as_ref()
            .is_some_and(|draft| draft.ready_for_check() && draft.proposal_for_check().id == id)
            && app
                .presentation
                .stable_proposed_ticket(
                    snapshot.session,
                    snapshot.document.project_id(),
                    snapshot.document.revision_id(),
                    &expected_content,
                    ProjectFrame(frame),
                )
                .is_some()
            && app.presentation.displayed_source_frame() == Some(SourceFrameId(source))
    };
    d.wait_for(
        "Exact proposed Object picture is decoded and submitted",
        matches,
    )?;
    let picture = d.app().presentation.diagnostic_snapshot();
    d.check(
        "Object Place requested, decoded and displayed picture match its exact proposal and source frame",
        matches(d.app()) && picture["error"].is_null(),
        json!({"revision":snapshot.document.revision_id(),"content":format!("{expected_content:?}"),"Edit":frame,"source_frame":source}),
        picture,
    )
}
fn prepared(d: &Driver<'_>) -> Result<Arc<Prepared>, String> {
    d.app()
        .splice
        .as_ref()
        .and_then(|draft| draft.prepared_for_check())
        .cloned()
        .ok_or("Missing Object Place preview".into())
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn object_state(d: &Driver<'_>) -> Value {
    let mut value = state(d);
    value["object"] = json!({"visual":d.app().capture_visual_selection().ok().flatten(),"scope":d.app().sequence_scope.groups(),"cursor":d.app().sequence_cursor,"selected":d.app().selected_beat,"label":d.app().edit_range_label()});
    value
}
