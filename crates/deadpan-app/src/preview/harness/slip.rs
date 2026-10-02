//! Real qualified Slip previews, decoder/GPU admission and native input batches.

use super::*;
use crate::project::slip::Prepared;
use deadpan_core::{
    Command, CommandRequest, ExactRatio, Framing, FramingCurve, FramingPose, ProjectDocument,
    RevisionId, SourceSlipClamp,
};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

const HEADING: &str = "Slip preview keyboard controls";
const AMOUNT: &str = "Signed Slip amount in project frames, for example +5f";
const APPLY: &str = "Apply Slip · Enter";
const CANCEL: &str = "Cancel · Esc";
const BEATS: &str = "Current group beat outline pane";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Slip uses the real qualified fixture, project service, decoder and offscreen Metal submissions. Controlled delivery delays retain actual service/decoder replies. Physical display presentation, OS IME delivery, audio audition and the remaining In/Out/Roll Trim operations are outside this scenario.".into());
    fixture(d)?;
    preview_and_cancel(d)?;
    native_input(d)?;
    late_reply(d)?;
    commit_and_history(d)?;
    rejected_contexts(d)?;
    nested_partition(d)
}

fn fixture(d: &mut Driver<'_>) -> Result<(), String> {
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
        Key::Escape,
    ])?;
    d.check(
        "Fixture copies exact Original ordinals [10,24)",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..24)),
        json!([10, 24]),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    let before = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&before)?;
    let workspace = d.app().workspace.as_ref().ok_or("No Slip fixture")?.clone();
    let selected = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Pasted Source not selected")?;
    let path = workspace.path.clone();
    close_writer(d)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).map_err(string)?;
    let pose = |x, scale| -> Result<FramingPose, String> {
        Ok(FramingPose {
            center_x: ExactRatio::new(x, 10).map_err(string)?,
            scale: ExactRatio::new(scale, 10).map_err(string)?,
            ..FramingPose::identity()
        })
    };
    // Fixture setup uses typed commands with the writer closed. Both retained
    // Source motion and live root motion must survive every later Slip.
    apply(
        &mut store,
        "slip-replay-source-framing",
        Command::SetFraming {
            node: selected.clone(),
            framing: Some(
                Framing::creep(pose(4, 12)?, pose(6, 15)?, FramingCurve::Smoothstep)
                    .map_err(string)?,
            ),
        },
    )?;
    apply(
        &mut store,
        "slip-replay-root-framing",
        Command::SetFraming {
            node: workspace.document.root().clone(),
            framing: Some(
                Framing::creep(pose(5, 10)?, pose(4, 11)?, FramingCurve::Linear).map_err(string)?,
            ),
        },
    )?;
    drop(store);
    reopen(d, path)?;
    d.command("sequence")?;
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    d.settled()?;
    d.check(
        "Qualified framed moment is selected at Edit 3",
        d.app().selected_beat.as_ref() == Some(&selected) && d.app().sequence_cursor == 3,
        json!({"selected":selected,"cursor":3}),
        d.snapshot(),
    )?;
    exact_picture(d, 13, false)
}

fn preview_and_cancel(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    let framing = d.app().presentation.diagnostic_snapshot()["decoded_framing"].clone();
    d.app_mut().feedback.hold_preview = true;
    d.command("slip +5f")?;
    d.wait_for(
        "Actual proposed decoder reply withheld before GPU submission",
        |app| app.feedback.held_reply.is_some(),
    )?;
    d.check(
        "A prepared proposal cannot Apply before its picture is submitted",
        prepared(d)?.resolution.applied_delta_frames == 5
            && !draft(d)?.ready_for_check()
            && d.rect(APPLY).is_err()
            && *document(d)? == saved,
        json!({"prepared":5,"apply_enabled":false,"revision":saved.revision_id()}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Enter while the actual proposed picture is pending cannot author history",
        d.app().slip.is_some() && *document(d)? == saved,
        json!(saved.revision_id()),
        d.snapshot(),
    )?;
    let held = d.app_mut().feedback.held_reply.take();
    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = held;
    wait_ready(d)?;
    exact_picture(d, 18, true)?;
    d.check(
        "Source and live ancestor framing stay in their output clocks during Slip",
        d.app().presentation.diagnostic_snapshot()["decoded_framing"] == framing
            && editor(d) == entry
            && *document(d)? == saved,
        json!({"framing":framing,"editor":entry}),
        d.snapshot(),
    )?;
    d.capture("Slip proposed +5f with actual composed Original frame 18")?;
    d.key(Key::B)?;
    wait_before(d)?;
    exact_picture(d, 13, false)?;
    d.check(
        "Before compares the same inspected output frame and disables Apply",
        draft(d)?.before_for_check()
            && draft(d)?.inspection_for_check() == ProjectFrame(3)
            && d.rect(APPLY).is_err()
            && editor(d) == entry,
        json!({"before":true,"inspection":3,"apply_enabled":false}),
        d.snapshot(),
    )?;
    d.capture("Slip Before comparison at the identical Edit frame")?;
    d.key(Key::B)?;
    wait_ready(d)?;
    field(d, "-100f")?;
    wait_ready(d)?;
    d.check(
        "Exact report distinguishes requested overshoot from the applied handle clamp",
        prepared(d)?.resolution.requested_delta_frames == -100
            && prepared(d)?.resolution.applied_delta_frames == -10
            && prepared(d)?.resolution.clamp == Some(SourceSlipClamp::PictureStart),
        json!({"requested":-100,"applied":-10,"clamp":"picture_start"}),
        state(d),
    )?;
    exact_picture(d, 3, true)?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.capture("Slip handle clamp at compact size")?;
    wait_ready(d)?;
    visible(
        d,
        "Requested -100f · Applied -10f · Picture start handle reached",
    )?;
    d.click(HEADING)?;
    d.key(Key::L)?;
    wait_ready(d)?;
    d.check(
        "One reverse key immediately leaves the clamped handle",
        prepared(d)?.resolution.applied_delta_frames == -9,
        json!(-9),
        state(d),
    )?;
    exact_picture(d, 4, true)?;
    let events = [
        (Key::L, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::H, Modifiers::NONE),
        (Key::L, Modifiers::SHIFT),
    ]
    .into_iter()
    .flat_map(|(key, modifiers)| {
        [
            key_event(key, modifiers, true),
            key_event(key, modifiers, false),
        ]
    })
    .collect();
    d.events(
        "Whole native batch l, l, h, Shift-l changes the requested amount exactly once",
        events,
    )?;
    wait_ready(d)?;
    d.check(
        "Batched keys survive footer retries and coalesce without losing steps",
        prepared(d)?.resolution.applied_delta_frames == 2
            && editor(d) == entry
            && *document(d)? == saved,
        json!({"applied":2,"editor":entry,"unchanged_revision":saved.revision_id()}),
        d.snapshot(),
    )?;
    exact_picture(d, 15, true)?;
    d.key(Key::O)?;
    wait_ready(d)?;
    exact_picture(d, 25, true)?;
    d.key(Key::I)?;
    wait_ready(d)?;
    exact_picture(d, 12, true)?;
    d.check(
        "First/last inspection never moves either editor cursor or the selected beat",
        editor(d) == entry && draft(d)?.inspection_for_check() == ProjectFrame(0),
        json!(entry),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.settled()?;
    d.check(
        "Cancel restores the exact entry editor and entire saved document",
        editor(d) == entry && *document(d)? == saved,
        json!({"editor":entry,"revision":saved.revision_id()}),
        d.snapshot(),
    )?;
    exact_picture(d, 13, false)
}

fn native_input(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.command("slip +5f")?;
    wait_ready(d)?;
    field(d, "+3f")?;
    d.key(Key::Enter)?;
    wait_ready(d)?;
    d.check(
        "Enter in the amount field edits native text without applying",
        d.app().slip.is_some()
            && prepared(d)?.resolution.applied_delta_frames == 3
            && *document(d)? == saved,
        json!({"open":true,"applied":3,"saved":saved.revision_id()}),
        d.snapshot(),
    )?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Synthetic native IME preedit owns Enter",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "+4f".into(),
                active_range_chars: Some(0..3),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME preedit cannot apply or navigate the editor",
        d.app().slip.is_some()
            && d.app().ime_composing
            && *document(d)? == saved
            && editor(d) == entry,
        json!({"composition":true,"saved":saved.revision_id()}),
        d.snapshot(),
    )?;
    d.events(
        "Synthetic native IME commit and Enter share one event batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("+4f".into())),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME completion cannot leak Enter into Apply",
        d.app().slip.is_some() && !d.app().ime_composing && *document(d)? == saved,
        json!({"composition":false,"saved":saved.revision_id()}),
        d.snapshot(),
    )?;
    field(d, "hld")?;
    d.chord(&[Key::H, Key::L, Key::D, Key::Enter])?;
    d.check(
        "Invalid native text retires the old proposal and prevents editor shortcuts",
        draft(d)?.error_for_check().is_some()
            && d.rect(APPLY).is_err()
            && *document(d)? == saved
            && editor(d) == entry,
        json!({"apply_enabled":false,"editor":entry}),
        d.snapshot(),
    )?;
    field(d, "+2f")?;
    wait_ready(d)?;
    d.harness.get_by_label(CANCEL).focus();
    d.step("Focus native Cancel without assigning an egui ID", false)?;
    let amount = prepared(d)?.resolution.applied_delta_frames;
    d.chord(&[Key::H, Key::L])?;
    d.check(
        "A focused native Cancel button owns letters instead of changing Slip",
        prepared(d)?.resolution.applied_delta_frames == amount && *document(d)? == saved,
        json!(amount),
        state(d),
    )?;
    d.key(Key::Enter)?;
    closed(d)?;
    d.check(
        "Native button Enter cancels and leaves no revision",
        *document(d)? == saved && editor(d) == entry,
        json!({"editor":entry,"saved":saved.revision_id()}),
        d.snapshot(),
    )?;
    d.command("slip 0f")?;
    d.wait_for(
        "No-op report uses the committed base without a proposed snapshot",
        |app| {
            !app.service.is_busy()
                && app
                    .slip
                    .as_ref()
                    .and_then(|draft| draft.prepared_for_check())
                    .is_some()
                && !app.presentation.loading()
                && !app.presentation.needs_render()
        },
    )?;
    d.check(
        "Zero movement is visible and cannot create a revision",
        prepared(d)?.snapshot.is_none()
            && prepared(d)?.resolution.applied_delta_frames == 0
            && d.rect(APPLY).is_err()
            && *document(d)? == saved,
        json!({"snapshot":null,"apply_enabled":false}),
        d.snapshot(),
    )?;
    exact_picture(d, 13, false)?;
    d.click(HEADING)?;
    d.key(Key::Enter)?;
    d.check(
        "No-op Enter leaves the draft open without history",
        d.app().slip.is_some() && *document(d)? == saved,
        json!(saved.revision_id()),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    closed(d)
}

fn late_reply(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.app_mut().feedback.hold_project_updates = true;
    d.command("slip +6f")?;
    d.wait_for(
        "Real Slip proposal finishes while service delivery is withheld",
        |app| !app.service.is_busy(),
    )?;
    let delayed = d
        .app()
        .service
        .take_update()
        .ok_or("No real Slip proposal update to delay")?;
    d.check(
        "Withheld update contains the actual admitted proposal",
        delayed
            .slip
            .as_ref()
            .is_some_and(|reply| reply.result.is_ok()),
        json!(true),
        json!(delayed.slip.is_some()),
    )?;
    d.key(Key::Escape)?;
    d.app_mut().feedback.hold_project_updates = false;
    d.app_mut().feedback.release_project_update = Some(delayed);
    closed(d)?;
    d.check(
        "Late real service success cannot reopen a cancelled draft or change selection",
        d.app().slip.is_none() && *document(d)? == saved && editor(d) == entry,
        json!({"open":false,"editor":entry}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_preview = true;
    d.command("slip +6f")?;
    d.wait_for(
        "Actual proposed picture finishes while decode delivery is withheld",
        |app| app.feedback.held_reply.is_some(),
    )?;
    d.key(Key::Escape)?;
    let late = d.app_mut().feedback.held_reply.take();
    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = late;
    closed(d)?;
    exact_picture(d, 13, false)?;
    d.check(
        "Cancelled proposal pixels cannot replace the restored committed picture",
        d.app().slip.is_none() && *document(d)? == saved && editor(d) == entry,
        json!({"open":false,"saved":saved.revision_id()}),
        d.snapshot(),
    )
}

fn commit_and_history(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.command("slip +5f")?;
    wait_ready(d)?;
    let candidate = prepared(d)?
        .snapshot
        .clone()
        .ok_or("Missing nonzero Slip snapshot")?;
    d.click(HEADING)?;
    d.events(
        "Apply with a held Enter repeat in the same native event batch",
        vec![
            key_event(Key::Enter, Modifiers::NONE, true),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: Modifiers::NONE,
            },
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.changed(saved.revision_id().as_str())?;
    d.check(
        "Apply saves the exact displayed proposal once and keeps both cursors and selection",
        d.app().slip.is_none() && *document(d)? == *candidate.document && editor(d) == entry,
        json!({"revision":candidate.document.revision_id(),"editor":entry}),
        d.snapshot(),
    )?;
    exact_picture(d, 18, false)?;
    d.capture("Saved Slip at the unchanged editor cursor")?;
    let committed = document(d)?.clone();
    d.key(Key::U)?;
    d.changed(committed.revision_id().as_str())?;
    d.check(
        "One Undo restores every authored node and sample binding before Slip",
        document(d)?.nodes() == saved.nodes()
            && document(d)?.audio_bindings() == saved.audio_bindings()
            && document(d)?.sounds() == saved.sounds()
            && document(d)?.revision_id() != saved.revision_id(),
        json!({"exact_structure_restored":true,"fresh_revision":true}),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.check(
        "One Redo restores the saved Slip with a fresh revision",
        document(d)?.nodes() == committed.nodes()
            && document(d)?.audio_bindings() == committed.audio_bindings()
            && document(d)?.revision_id() != committed.revision_id(),
        json!("exact saved content, fresh revision"),
        d.snapshot(),
    )?;
    let redone = d.revision();
    d.key(Key::U)?;
    d.changed(&redone)?;
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    d.settled()
}

fn rejected_contexts(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    d.command("source")?;
    d.command("slip +5f")?;
    d.check(
        "Original command entry refuses Slip without modifying the edit",
        d.app().slip.is_none()
            && *document(d)? == saved
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Select a picture beat in Your edit")),
        json!("explicit refusal in Original"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.click(BEATS)?;
    d.key(Key::V)?;
    let selection = d.app().edit_range.clone();
    d.command("slip +5f")?;
    d.check(
        "An empty Visual selection stays distinct from an eligible selected beat",
        d.app().slip.is_none()
            && *document(d)? == saved
            && d.app().edit_range == selection
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Clear the Visual Edit selection")),
        json!("Visual selection retained; no Slip"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.settled()
}

fn nested_partition(d: &mut Driver<'_>) -> Result<(), String> {
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num5, Key::L])?;
    let before_split = d.revision();
    d.key(Key::S)?;
    d.changed(&before_split)?;
    let right = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Split did not select right fragment")?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No split workspace")?
        .clone();
    d.check(
        "Native Split creates the neutral Partition that Slip must retain",
        matches!(
            workspace.document.nodes()[&right].kind,
            NodeKind::Retime {
                purpose: deadpan_core::RetimePurpose::Partition,
                ..
            }
        ),
        json!("neutral Partition"),
        json!(workspace.document.nodes()[&right]),
    )?;
    let path = workspace.path.clone();
    close_writer(d)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).map_err(string)?;
    let group = NodeId::new("slip-replay-group").map_err(string)?;
    apply(
        &mut store,
        "slip-replay-group-revision",
        Command::Group {
            parent: workspace.document.root().clone(),
            start: 0,
            end: 2,
            id: group.clone(),
            label: "Slip fragments".into(),
        },
    )?;
    drop(store);
    reopen(d, path)?;
    d.command("sequence")?;
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Enter, Key::Num5, Key::L])?;
    d.settled()?;
    d.check(
        "Keyboard descent keeps the exact right Partition selected inside its ordinary group",
        d.app().selected_beat.as_ref() == Some(&right)
            && d.app().sequence_scope.groups() == [group.clone()]
            && d.app().sequence_cursor == 5,
        json!({"group":group,"selected":right,"cursor":5}),
        d.snapshot(),
    )?;
    let saved = document(d)?.clone();
    let entry = editor(d);
    let framing = d.app().presentation.diagnostic_snapshot()["decoded_framing"].clone();
    d.command("slip +2f")?;
    wait_ready(d)?;
    exact_picture(d, 17, true)?;
    let result = prepared(d)?;
    d.check(
        "Partition preview resolves its physical Source and retains the inherited camera paths",
        result.resolution.target == right
            && result.resolution.physical_source != right
            && d.app().presentation.diagnostic_snapshot()["decoded_framing"] == framing
            && editor(d) == entry,
        json!({"selected_wrapper":right,"framing":framing,"editor":entry}),
        d.snapshot(),
    )?;
    d.capture("Nested Partition Slip with retained Source and live ancestor camera paths")?;
    d.click(APPLY)?;
    d.changed(saved.revision_id().as_str())?;
    let mut expected_wrapper = saved.nodes()[&right].clone();
    expected_wrapper.audio_editorial_edges = deadpan_core::AudioEditorialEdges {
        start: true,
        end: true,
    };
    d.check(
        "Partition Apply marks both changed audio joins and preserves its wrapper and group selection",
        document(d)?.nodes()[&right] == expected_wrapper && editor(d) == entry,
        json!({"wrapper":expected_wrapper,"editor":entry}),
        json!({"wrapper":document(d)?.nodes()[&right],"editor":editor(d)}),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    d.check(
        "One Undo restores the complete pre-Slip nested document",
        document(d)?.nodes() == saved.nodes()
            && document(d)?.audio_bindings() == saved.audio_bindings(),
        json!("exact nodes and sample bindings"),
        d.snapshot(),
    )
}

fn string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn apply(store: &mut ProjectStore, revision: &str, command: Command) -> Result<(), String> {
    let document = store.snapshot().map_err(string)?;
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).map_err(string)?,
            command,
        })
        .map_err(string)?;
    Ok(())
}

fn close_writer(d: &mut Driver<'_>) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close replay writer before typed fixture setup", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })
}

fn reopen(d: &mut Driver<'_>, path: std::path::PathBuf) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for(
        "Reopen typed Slip fixture through the real service",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.path == path)
        },
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No Slip replay project".into())
}
fn draft<'a>(d: &'a Driver<'_>) -> Result<&'a super::super::slip::Draft, String> {
    d.app()
        .slip
        .as_ref()
        .ok_or_else(|| "Slip preview is not open".into())
}
fn prepared(d: &Driver<'_>) -> Result<Arc<Prepared>, String> {
    draft(d)?
        .prepared_for_check()
        .cloned()
        .ok_or_else(|| "Slip proposal is not ready".into())
}
fn editor(d: &Driver<'_>) -> Value {
    let app = d.app();
    json!({"source_cursor":app.source_cursor,"sequence_cursor":app.sequence_cursor,"selected_beat":app.selected_beat,
        "selected_source":app.selected_source,"scope":app.sequence_scope.groups(),"duration":app.sequence_length(),
        "selection":format!("{:?}",app.edit_selection())})
}
pub(super) fn state(d: &Driver<'_>) -> Value {
    json!(d.app().slip.as_ref().map(|draft| {
        let proposal = draft.proposal_for_check();
        json!({"session":proposal.target.session,"project":proposal.target.project,"base_revision":proposal.target.base_revision,
            "draft":proposal.draft,"change":proposal.change,"target":proposal.target.node,"parent":proposal.target.parent,
            "range":[proposal.target.range.start().0,proposal.target.range.end().0],"requested":proposal.delta_frames,
            "before":draft.before_for_check(),"inspection":draft.inspection_for_check().0,"apply_ready":draft.ready_for_check(),
            "applying":draft.applying_for_check(),"error":draft.error_for_check(),
            "prepared":draft.prepared_for_check().map(|prepared| json!({"revision":prepared.snapshot.as_ref().map(|snapshot|snapshot.document.revision_id()),
                "applied":prepared.resolution.applied_delta_frames,"minimum":prepared.resolution.minimum_delta,
                "maximum":prepared.resolution.maximum_delta,"clamp":format!("{:?}",prepared.resolution.clamp),"physical_source":prepared.resolution.physical_source}))})
    }))
}
fn wait_ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Current proposed Slip picture has reached the GPU at the current raster",
        |app| {
            !app.service.is_busy()
                && app
                    .slip
                    .as_ref()
                    .is_some_and(|draft| draft.ready_for_check())
                && !app.presentation.loading()
                && !app.presentation.needs_render()
        },
    )
}
fn wait_before(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Before picture reaches the same output inspection frame",
        |app| {
            let Some(draft) = &app.slip else { return false };
            let Some(prepared) = draft.prepared_for_check() else {
                return false;
            };
            draft.before_for_check()
                && !app.presentation.loading()
                && !app.presentation.needs_render()
                && app.presentation.displayed_matches(
                    prepared.base.session,
                    prepared.base.document.project_id(),
                    prepared.base.document.revision_id(),
                    &ProjectView::Sequence {
                        frame: draft.inspection_for_check(),
                    },
                )
        },
    )
}
fn closed(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Slip closes and drains its abandon request", |app| {
        app.slip.is_none() && app.slip_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()
}
fn field(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.click(AMOUNT)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Replace native signed Slip amount",
        vec![egui::Event::Text(text.into())],
    )
}
fn visible(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, text);
    d.check(
        "Slip report text is fully painted inside the current viewport",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(text),
        json!(paint),
    )
}
fn exact_picture(d: &mut Driver<'_>, ordinal: u64, proposed: bool) -> Result<(), String> {
    let picture = d
        .app()
        .presentation
        .picture()
        .ok_or("No decoded Slip picture")?;
    let frame = picture.frame.as_ref().ok_or("Slip returned a background")?;
    let metadata = frame.metadata();
    let workspace = d.app().workspace.as_ref().ok_or("No Slip workspace")?;
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .ok_or("No qualified video index")?;
    let index = source.video_index.as_ref().ok_or("No qualified index")?;
    let expected = index
        .frames()
        .get(usize::try_from(ordinal).map_err(string)?)
        .ok_or("Fixture ordinal out of bounds")?;
    let state = d.app().presentation.diagnostic_snapshot();
    let inspection = d.app().slip.as_ref().map_or_else(
        || {
            i64::try_from(d.app().sequence_cursor)
                .map(ProjectFrame)
                .map_err(string)
        },
        |draft| Ok(draft.inspection_for_check()),
    )?;
    let identity_matches = if proposed {
        let prepared = prepared(d)?;
        let snapshot = prepared
            .snapshot
            .as_ref()
            .ok_or("No proposed picture identity")?;
        d.app()
            .presentation
            .stable_proposed_ticket(
                snapshot.session,
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &snapshot.content,
                inspection,
            )
            .is_some()
    } else {
        d.app().presentation.displayed_matches(
            workspace.session,
            workspace.document.project_id(),
            workspace.document.revision_id(),
            &ProjectView::Sequence { frame: inspection },
        )
    };
    let passed = picture.id == SourceFrameId(ordinal)
        && d.app().presentation.displayed_source_frame() == Some(picture.id)
        && metadata.pts.ticks == expected.pts
        && metadata.pts.time_base == index.time_base()
        && state["displayed"] == state["decoded"]
        && state["geometry_revision"] == state["decoded_geometry_revision"]
        && state["error"].is_null()
        && identity_matches;
    d.check("Submitted Slip picture has the independent expected ordinal and exact decoder PTS",
        passed,json!({"ordinal":ordinal,"pts":expected.pts,"time_base":index.time_base(),"proposed":proposed}),
        json!({"presentation":state,"decoder_ordinal":picture.id,"decoder_pts":metadata.pts}))
}
