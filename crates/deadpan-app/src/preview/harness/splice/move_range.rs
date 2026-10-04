//! Real Move proposals, local comparison clocks and one durable range selection.

use super::*;
use crate::preview::copied::Content;
use crate::project::slice::Captured;
use crate::project::splice::Operation;
use deadpan_core::NodeKind;
use edited::{accepted, cancel, goto, motion, range, select, undo, wait_edited_endpoints};

mod groups;
mod input;
mod receipt;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    d.key(Key::Escape)?;
    d.check(
        "Move replay starts with one full 120-frame Original in the ordinary root",
        d.app().sequence_scope.groups().is_empty()
            && d.app().sequence_length() == 120
            && document(d)?
                .nodes()
                .values()
                .filter(|node| matches!(node.kind, NodeKind::Source { .. }))
                .count()
                == 1,
        json!({"frames":120,"source_owners":1,"scope":"root"}),
        d.snapshot(),
    )?;
    forward(d)?;
    reverse(d)?;
    rejected_and_toggles(d)?;
    nearby_and_terminal(d)?;
    groups::run(d)
}

fn copy(d: &mut Driver<'_>, start: u64, out: u64) -> Result<Arc<Captured>, String> {
    select(d, start, out)?;
    d.key(Key::Y)?;
    d.wait_for("Capture a current-revision Edit range for Move", |app| {
        !app.service.is_busy()
            && !app.copied.is_pending()
            && matches!(app.copied.content(), Some(Content::Edited(copied))
                if copied.slice().range().start() == ProjectFrame(start as i64)
                    && copied.slice().range().end() == ProjectFrame(out as i64)
                    && app.workspace.as_ref().is_some_and(|workspace|
                        copied.slice().revision_id() == workspace.document.revision_id()))
    })?;
    accepted(d)
}

fn open(d: &mut Driver<'_>, destination: u64) -> Result<(), String> {
    goto(d, destination)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::M)?;
    wait_ready(d)
}

fn forward(d: &mut Driver<'_>) -> Result<(), String> {
    let copied = copy(d, 20, 30)?;
    let saved = document(d)?.clone();
    // Keep an unrelated finished selection visible while the Move receipt is
    // later delivered ahead of its workspace.
    select(d, 40, 45)?;
    d.key(Key::V)?;
    motion(d, 15, true)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::M)?;
    wait_ready(d)?;
    wait_edited_endpoints(d, 20, 30)?;
    let exact = prepared(d)?;
    let movement = exact.movement.as_ref().ok_or("Move has no site metadata")?;
    d.check(
        "Three cuts in one Source preview a Move without adding time or a copied wrapper",
        draft(d)?.proposal_for_check().operation == Operation::Move
            && exact.range == range(50, 60)?
            && movement.source_before == range(20, 30)?
            && movement.destination_before == ProjectFrame(60)
            && movement.removal_after == ProjectFrame(20)
            && exact.plan.duration().frames() == 120
            && !matches!(
                exact.snapshot.document.nodes()[&exact.node].kind,
                NodeKind::Sequence { .. }
            )
            && *document(d)? == saved,
        json!({"source":[20,30],"inserted":[50,60],"removal":20,"duration":120}),
        state(d),
    )?;
    d.key(Key::S)?;
    wait_picture(d, 30, "Showing proposed edit frame 21")?;
    let id = draft(d)?.proposal_for_check().id.clone();
    d.key(Key::B)?;
    wait_picture(d, 20, "Showing sequence frame 21")?;
    d.key(Key::B)?;
    d.key(Key::F)?;
    wait_picture(d, 20, "Showing proposed edit frame 51")?;
    d.key(Key::H)?;
    wait_picture(d, 59, "Showing proposed edit frame 50")?;
    d.key(Key::L)?;
    motion(d, 9, true)?;
    wait_picture(d, 29, "Showing proposed edit frame 60")?;
    d.key(Key::L)?;
    wait_picture(d, 60, "Showing proposed edit frame 61")?;
    d.check(
        "Both local sites and picture inspection share one immutable Move proposal",
        draft(d)?.proposal_for_check().id == id && Arc::ptr_eq(&prepared(d)?, &exact),
        json!({"unchanged_proposal":true,"permutation":[[0,20],[30,60],[20,30],[60,120]]}),
        state(d),
    )?;
    input::run(d)?;
    layout(d)?;
    local_audition(d, Key::S, (20, 30), (20, 20), (0, 60), (0, 50))?;
    local_audition(d, Key::F, (60, 60), (50, 60), (30, 120), (20, 120))?;
    d.key(Key::F)?;
    receipt::commit(d, &copied, &saved)
}

fn reverse(d: &mut Driver<'_>) -> Result<(), String> {
    copy(d, 70, 80)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    open(d, 10)?;
    let exact = prepared(d)?;
    d.check(
        "A backward Move retains the pre-edit destination and final removal join",
        exact.range == range(10, 20)?
            && exact.movement.as_ref().is_some_and(|movement| {
                movement.destination_before == ProjectFrame(10)
                    && movement.removal_after == ProjectFrame(80)
            }),
        json!({"inserted":[10,20],"removal":80}),
        state(d),
    )?;
    d.key(Key::F)?;
    wait_picture(d, 70, "Showing proposed edit frame 11")?;
    d.key(Key::H)?;
    wait_picture(d, 9, "Showing proposed edit frame 10")?;
    motion(d, 10, true)?;
    wait_picture(d, 79, "Showing proposed edit frame 20")?;
    d.key(Key::L)?;
    wait_picture(d, 10, "Showing proposed edit frame 21")?;
    d.key(Key::S)?;
    wait_picture(d, 80, "Showing proposed edit frame 81")?;
    d.key(Key::B)?;
    wait_picture(d, 70, "Showing sequence frame 71")?;
    d.key(Key::B)?;
    local_audition(d, Key::S, (70, 80), (80, 80), (10, 120), (20, 120))?;
    local_audition(d, Key::F, (10, 10), (10, 20), (0, 70), (0, 80))?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    committed(d, &exact)?;
    undo(d, &saved)
}

fn committed(d: &mut Driver<'_>, exact: &Prepared) -> Result<(), String> {
    d.check(
        "Move commits the exact preview and selects its entire finished result interval",
        d.app().splice.is_none() && *document(d)? == *exact.snapshot.document
            && d.app().selected_beat.as_ref() == Some(&exact.node)
            && d.app().selected_edit_range() == Some(exact.range)
            && !d.app().edit_range.active
            && d.app().sequence_cursor == exact.range.start().0 as u64,
        json!({"revision":exact.snapshot.document.revision_id(),"range":[exact.range.start().0,exact.range.end().0],"first_child":exact.node}), d.snapshot(),
    )
}

fn rejected_and_toggles(d: &mut Driver<'_>) -> Result<(), String> {
    let copied = copy(d, 20, 30)?;
    let saved = document(d)?.clone();
    for destination in [25, 20, 30] {
        goto(d, destination)?;
        d.command("splice")?;
        wait_ready(d)?;
        d.key(Key::M)?;
        rejected(d)?;
        wait_edited_endpoints(d, 20, 30)?;
        d.key(Key::I)?;
        wait_picture(d, 20, "Showing copied Edit frame 21")?;
        d.key(Key::O)?;
        wait_picture(d, 29, "Showing copied Edit frame 30")?;
        d.check(
            "An own-interior or exact no-op Move keeps source endpoints without enabling commit",
            !draft(d)?.ready_for_check() && d.rect(APPLY).is_err()
                && (destination == 25 || draft(d)?.error_for_check() == Some("Already at this position; no change"))
                && *document(d)? == saved && Arc::ptr_eq(&accepted(d)?, &copied),
            json!({"destination":destination,"apply_enabled":false,"endpoints":[20,30],"saved_unchanged":true}), state(d),
        )?;
        if destination == 20 {
            d.capture(
                "No-op Move retains source endpoints and disables Apply with an explicit reason",
            )?;
        }
        d.key(Key::Enter)?;
        d.check(
            "Enter on a rejected Move cannot save a split or a revision",
            *document(d)? == saved && d.app().splice.is_some(),
            json!(saved.revision_id()),
            d.snapshot(),
        )?;
        cancel(d)?;
    }

    select(d, 60, 70)?;
    d.key(Key::V)?;
    motion(d, 20, true)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::R)?;
    wait_ready(d)?;
    d.check(
        "Replace keeps the independently captured selection",
        prepared(d)?.removed == Some(range(60, 70)?),
        json!([60, 70]),
        state(d),
    )?;
    d.key(Key::M)?;
    wait_ready(d)?;
    let moved = prepared(d)?;
    d.check(
        "Replace to Move restores insertion at 90 instead of adopting replacement In 60",
        moved.removed.is_none()
            && moved.range == range(80, 90)?
            && moved
                .movement
                .as_ref()
                .is_some_and(|movement| movement.destination_before == ProjectFrame(90)),
        json!({"destination":90,"inserted":[80,90]}),
        state(d),
    )?;
    d.key(Key::R)?;
    wait_ready(d)?;
    d.check(
        "Replacement always returns to Copy",
        draft(d)?.proposal_for_check().operation == Operation::Copy
            && prepared(d)?.removed == Some(range(60, 70)?)
            && prepared(d)?.movement.is_none(),
        json!("Copy replacement"),
        state(d),
    )?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.check(
        "Move to Copy keeps the independent insertion destination and copied register",
        prepared(d)?.range == range(90, 100)?
            && prepared(d)?.removed.is_none()
            && Arc::ptr_eq(&accepted(d)?, &copied),
        json!({"copy_inserted":[90,100]}),
        state(d),
    )?;
    cancel(d)?;

    goto(d, 90)?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    undo(d, &saved)?;
    goto(d, 60)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::M)?;
    rejected(d)?;
    d.check(
        "Undo cannot revive a historical capture's authority to Move",
        draft(d)?
            .error_for_check()
            .is_some_and(|error| error.contains("older revision"))
            && copied.slice().revision_id() != document(d)?.revision_id()
            && d.rect(APPLY).is_err(),
        json!("same authored document, fresh revision, stale Move rejected"),
        state(d),
    )?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.check(
        "Historical Copy remains usable after Move rejects its revision",
        prepared(d)?.movement.is_none() && prepared(d)?.range == range(60, 70)?,
        json!("Copy still ready"),
        state(d),
    )?;
    cancel(d)?;
    copy(d, 20, 30)?;
    open(d, 60)?;
    d.check(
        "A fresh current-revision yank restores Move",
        prepared(d)?.movement.is_some(),
        json!("Move ready"),
        state(d),
    )?;
    cancel(d)?;
    copy(d, 110, 120)?;
    open(d, 10)?;
    d.key(Key::S)?;
    terminal_compare(d, 110)?;
    cancel(d)
}

fn terminal_compare(d: &mut Driver<'_>, before_boundary: i64) -> Result<(), String> {
    let total = prepared(d)?.plan.duration().frames();
    let end = prepared(d)?
        .snapshot
        .document
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(total))
        .map_err(|error| error.to_string())?;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    d.key(Key::Space)?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let ended = delivery(d, Phase::Ended, end, generation)?;
    inject(
        d,
        ended,
        "Deliver completed Move audition at exact terminal sample B(N)",
    )?;
    d.check(
        "Ended Move audition retains the exact terminal sample",
        d.app().transport.is_none() && draft(d)?.position == Some(end),
        json!(end.0),
        state(d),
    )?;
    for before in [true, false] {
        d.key(Key::B)?;
        let frame = if before { before_boundary } else { total };
        let sample = prepared(d)?
            .snapshot
            .document
            .presentation_basis()
            .frame_rate
            .audio_boundary(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        d.check(
            "Paused terminal comparison maps the exact join boundary without retreating one sample",
            d.app().transport.is_none()
                && draft(d)?.before_for_check() == before
                && draft(d)?.position == Some(sample)
                && draft(d)?.cursor == frame as u64,
            json!({"before":before,"sample":sample.0,"cursor":frame}),
            state(d),
        )?;
    }
    d.app_mut().feedback.simulate_playback = simulated;
    Ok(())
}

fn rejected(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Wait for the revision-bound Move rejection", |app| {
        !app.service.is_busy()
            && app
                .splice
                .as_ref()
                .is_some_and(|draft| draft.error_for_check().is_some() && !draft.ready_for_check())
    })
}

fn nearby_and_terminal(d: &mut Driver<'_>) -> Result<(), String> {
    copy(d, 20, 30)?;
    let saved = document(d)?.clone();
    for (destination, saved_limits, proposed_limits, inserted) in [
        (31, (0, 31), (0, 21), (21, 31)),
        (19, (19, 120), (29, 120), (19, 29)),
    ] {
        open(d, destination)?;
        let removal = if destination < 20 { 30 } else { 20 };
        local_audition(
            d,
            Key::S,
            (20, 30),
            (removal, removal),
            saved_limits,
            proposed_limits,
        )?;
        let (saved_limits, proposed_limits) = if destination < 20 {
            ((0, 20), (0, 30))
        } else {
            ((30, 120), (20, 120))
        };
        local_audition(
            d,
            Key::F,
            (destination as i64, destination as i64),
            inserted,
            saved_limits,
            proposed_limits,
        )?;
        cancel(d)?;
        d.check(
            "Nearby join inspection and cancellation leave every authored field unchanged",
            *document(d)? == saved,
            json!(saved.revision_id()),
            d.snapshot(),
        )?;
    }
    copy(d, 0, 10)?;
    open(d, 120)?;
    d.key(Key::F)?;
    motion(d, 10, true)?;
    wait_picture(d, 9, "Showing proposed edit frame 120")?;
    d.check(
        "Move inspection keeps terminal boundary 120 distinct from displayed picture 119",
        draft(d)?.cursor == 120,
        json!({"cursor":120,"displayed_source":9}),
        state(d),
    )?;
    paint_text(d, "Requested terminal Edit boundary 120")?;
    d.key(Key::B)?;
    wait_picture(d, 119, "Showing sequence frame 120")?;
    d.check(
        "Before at a terminal Move join retains the terminal cursor",
        draft(d)?.cursor == 120 && d.app().transport.is_none(),
        json!(120),
        state(d),
    )?;
    cancel(d)
}

fn local_audition(
    d: &mut Driver<'_>,
    site: Key,
    saved: (i64, i64),
    proposed: (i64, i64),
    saved_limits: (i64, i64),
    proposed_limits: (i64, i64),
) -> Result<(), String> {
    let exact = prepared(d)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    let entry = editor(d);
    let rate = exact.snapshot.document.presentation_basis().frame_rate;
    let b = |frame| {
        rate.audio_boundary(ProjectFrame(frame))
            .map_err(|error| error.to_string())
    };
    let lead = d.app().audition_context.lead.0;
    let follow = d.app().audition_context.follow.0;
    let window = |affected: (i64, i64), limits: (i64, i64)| -> Result<_, String> {
        Ok((
            AudioSample((b(affected.0)?.0 - lead).max(b(limits.0)?.0)),
            AudioSample((b(affected.1)?.0 + follow).min(b(limits.1)?.0)),
        ))
    };
    let expected_proposed = window(proposed, proposed_limits)?;
    let expected_saved = window(saved, saved_limits)?;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    if draft(d)?.before_for_check() {
        d.key(Key::B)?;
    }
    d.key(site)?;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Move loop did not start")?;
    d.check(
        "Move audition prepares only the active local join with the other site as a context cap",
        run.window().looping() && run.window().start() == expected_proposed.0
            && run.window().end() == expected_proposed.1 && run.content == exact.snapshot.content,
        json!({"site":format!("{site:?}"),"proposed_samples":[expected_proposed.0.0,expected_proposed.1.0]}), d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    // Both windows include this suffix. The offset is from each absolute join,
    // not a claim that independent root sounds move with the Source.
    let heard = AudioSample(b(proposed.1)?.0 + 137);
    let mapped = AudioSample(b(saved.1)?.0 + 137);
    if heard >= expected_proposed.1 || mapped >= expected_saved.1 {
        return Err("Move replay needs 137 samples of local suffix context".into());
    }
    let update = delivery(d, Phase::Playing, heard, generation)?;
    inject(
        d,
        update.clone(),
        "Inject heard Move suffix delivery at an exact sample offset",
    )?;
    d.key(Key::B)?;
    d.check(
        "Running Move comparison preserves playback and exact site-relative suffix offset",
        d.app().transport.as_ref().is_some_and(|run| {
            run.content == ContentIdentity::Committed
                && run.sample == mapped
                && run.window().start() == expected_saved.0
                && run.window().end() == expected_saved.1
        }),
        json!({"saved_samples":[expected_saved.0.0,expected_saved.1.0],"heard":mapped.0}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.key(Key::B)?;
    d.check(
        "Paused Move comparison stays paused and maps back to the exact sample",
        d.app().transport.is_none() && draft(d)?.position == Some(heard),
        json!(heard.0),
        state(d),
    )?;
    inject(
        d,
        update,
        "Deliver stale Move audio after comparison and pause",
    )?;
    d.check(
        "Late Move delivery cannot resume audio or change saved editor state",
        d.app().transport.is_none()
            && draft(d)?.position == Some(heard)
            && editor(d) == entry
            && draft(d)?.proposal_for_check().id == id,
        json!({"paused":true,"proposal_unchanged":true,"editor":entry}),
        state(d),
    )?;
    d.app_mut().feedback.simulate_playback = simulated;
    Ok(())
}

fn layout(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::F)?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing Move viewport")?
            .inner_rect = Some(rect);
        d.step(
            "Paint both local Move sites and endpoints at the requested viewport",
            true,
        )?;
        wait_edited_endpoints(d, 20, 30)?;
        wait_picture(d, 20, "Showing proposed edit frame 51")?;
        for label in [
            "UNSAVED · Move · Linked picture + sound",
            "Copy instead · m",
            "Insertion join · f",
            "Removal join  s",
            "Loop this join · Shift Space",
            APPLY,
            CANCEL,
        ] {
            paint_text(d, label)?;
        }
        for label in [
            "Removal · s · Saved [20..30) to Proposed [20..20)",
            "Insertion · f · Saved [60..60) to Proposed [50..60)",
        ] {
            // WidgetInfo::Label exposes its text as the accessibility value,
            // unlike the control labels used by Driver::rect.
            let rows = d
                .harness
                .root()
                .children_recursive()
                .filter_map(|node| {
                    let access = node.accesskit_node();
                    (access.role() == egui::accesskit::Role::Label
                        && access.value().as_deref() == Some(label)
                        && !access.is_hidden()
                        && !access.is_disabled()
                        && access.bounding_box().is_some())
                    .then(|| node.rect())
                })
                .collect::<Vec<_>>();
            let [row] = rows.as_slice() else {
                return Err(format!(
                    "Expected one visible enabled Move timeline {label:?}, found {}",
                    rows.len()
                ));
            };
            let row = *row;
            let viewport = d.harness.ctx.content_rect();
            let visible_paint = scenarios::text_paint_visibility(d, label);
            let backgrounds = d.harness.output().shapes.iter().filter_map(|clipped| {
                let egui::Shape::Rect(painted) = &clipped.shape else { return None; };
                (painted.fill.is_opaque() && painted.rect.min.distance(row.min) <= 0.5
                    && painted.rect.max.distance(row.max) <= 0.5).then(|| json!({
                        "clip":[clipped.clip_rect.left(),clipped.clip_rect.top(),clipped.clip_rect.right(),clipped.clip_rect.bottom()],
                        "fully_visible":clipped.clip_rect.contains_rect(row) && viewport.contains_rect(painted.rect),
                    }))
            }).collect::<Vec<_>>();
            d.check(
                "Each Move timeline row and its complete accessible region remain painted inside the viewport",
                row.is_positive() && row.height() >= 44.0 && viewport.contains_rect(row)
                    && !backgrounds.is_empty() && backgrounds.iter().all(|paint| paint["fully_visible"] == true)
                    && !visible_paint.is_empty() && visible_paint.iter().all(|paint| paint["fully_visible"] == true),
                json!({"label":label,"minimum_height":44,"fully_visible":true}),
                json!({"row":[row.left(),row.top(),row.right(),row.bottom()],
                    "viewport":[viewport.left(),viewport.top(),viewport.right(),viewport.bottom()],
                    "backgrounds":backgrounds,"text":visible_paint}),
            )?;
        }
        endpoint_painted(d, "First included copied Edit picture, frame 21")?;
        endpoint_painted(d, "Last included copied Edit picture, frame 30")?;
        viewer_painted(d)?;
        d.capture(&format!(
            "Move local removal and insertion contexts at {width} by {height}"
        ))?;
    }
    Ok(())
}
