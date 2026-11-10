//! `:splice` with its destination inside play 2 through native keys: refine
//! the destination into a beat, audition Before and Proposed, Enter commits
//! that play only, and Undo restores it.

use super::copy_paste::enter_play;
use super::*;

fn draft<'a>(d: &'a Driver<'_>) -> Result<&'a crate::preview::splice::Draft, String> {
    d.app()
        .splice
        .as_ref()
        .ok_or_else(|| "Place slice is not open".into())
}

fn wait_ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("The one-play slice proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .splice
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
    })
}

pub(super) fn run(
    d: &mut Driver<'_>,
    repeat: &NodeId,
    before: &[i64],
    original: &ProjectDocument,
) -> Result<(), String> {
    // The register holds play 2's right part (copied by copy_paste).
    let right = before[1] - 10;
    enter_play(d, 2)?;
    let edited = d.revision();
    d.key(Key::S)?;
    d.changed(&edited)?;
    idle(d)?;
    let split = d.app().sequence_cursor;
    let entry = d.revision();
    d.command("splice")?;
    wait_ready(d)?;
    let opened = draft(d)?
        .prepared_for_check()
        .cloned()
        .ok_or("not prepared")?;
    d.check(
        ":splice inside play 2 opens a one-play proposal at the split seam without an edit",
        draft(d)?.proposal_for_check().occurrence.is_some()
            && opened.range.start().0 == i64::try_from(split).unwrap_or(-1)
            && d.revision() == entry,
        json!({"occurrence":true,"insertion":split,"revision":"unchanged"}),
        crate::preview::harness::splice::state(d),
    )?;
    // Three frames later is strictly inside the right part.
    for _ in 0..3 {
        d.key(Key::L)?;
    }
    wait_ready(d)?;
    let ready = draft(d)?
        .prepared_for_check()
        .cloned()
        .ok_or("not prepared")?;
    let proposed_lengths = {
        let document = &ready.snapshot.document;
        let NodeKind::Repeat {
            child, iterations, ..
        } = &document.nodes()[repeat].kind
        else {
            return Err("Expected the Repeat".into());
        };
        let durations = document
            .durations_shared()
            .map_err(|error| error.to_string())?;
        (0..iterations.len())
            .filter_map(|index| iterations.at(index))
            .map(|iteration| {
                let root = document
                    .overrides()
                    .get(repeat)
                    .and_then(|plays| plays.get(&iteration))
                    .unwrap_or(child);
                durations[root].frames()
            })
            .collect::<Vec<_>>()
    };
    let inside = i64::try_from(split).unwrap_or(-1) + 3;
    d.check(
        "l refines the destination three frames inside play 2's right part; the proposal grows that play only",
        ready.range.start().0 == inside
            && ready.range.duration().frames() == right
            && proposed_lengths == [before[0], before[1] + right, before[2]]
            && matches!(draft(d)?.proposal_for_check().destination, crate::project::splice::Destination::Interior { .. })
            && d.revision() == entry,
        json!({"insertion":inside,"frames":right,"plays":[before[0], before[1] + right, before[2]]}),
        json!({"insertion":ready.range.start().0,"frames":ready.range.duration().frames(),"plays":proposed_lengths,"state":crate::preview::harness::splice::state(d)}),
    )?;
    d.key(Key::M)?;
    wait_ready(d)?;
    let refusal = draft(d)?.error_for_check().map(str::to_owned);
    d.check(
        "m refuses Move for a one-play destination readably and keeps the Copy proposal",
        refusal
            .as_deref()
            .is_some_and(|error| error.contains("Move cannot target one Repeat play"))
            && draft(d)?.proposal_for_check().operation == crate::project::splice::Operation::Copy,
        json!({"error":"Move cannot target one Repeat play…","operation":"Copy"}),
        crate::preview::harness::splice::state(d),
    )?;
    d.key(Key::B)?;
    wait_ready(d)?;
    let compared = draft(d)?.before_for_check();
    d.key(Key::B)?;
    wait_ready(d)?;
    d.check(
        "b compares Before and Proposed without an edit",
        compared && !draft(d)?.before_for_check() && d.revision() == entry,
        json!({"before":true,"then":"proposed"}),
        crate::preview::harness::splice::state(d),
    )?;
    d.key(Key::Space)?;
    wait_ready(d)?;
    let playing = d.app().transport.is_some();
    d.key(Key::Space)?;
    wait_ready(d)?;
    d.check(
        "Space auditions the proposed play and pauses it at its heard sample",
        playing && d.app().transport.is_none() && draft(d)?.position.is_some(),
        json!({"played":true,"paused":true}),
        crate::preview::harness::splice::state(d),
    )?;
    d.capture("Place slice proposal inside play 2's right part")?;
    d.key(Key::Enter)?;
    d.changed(&entry)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    d.check(
        "Enter places the copy inside play 2 only and the inspector selects it at its start",
        lengths == [before[0], before[1] + right, before[2]]
            && d.app().splice.is_none()
            && selected.as_ref() == Some(&ready.node)
            && d.app().sequence_cursor == u64::try_from(inside).unwrap_or(0)
            && d.app().error.is_none(),
        json!({"plays":[before[0], before[1] + right, before[2]],"selected":ready.node,"Edit":inside}),
        json!({"plays":lengths,"selected":selected,"Edit":d.app().sequence_cursor,"error":d.app().error}),
    )?;
    painted(d, "This play 2/3")?;
    d.capture("Committed copy inside play 2 selected in the inspector")?;
    for _ in 0..2 {
        let edited = d.revision();
        d.key(Key::U)?;
        d.changed(&edited)?;
        idle(d)?;
    }
    d.check(
        "Two Undos remove the placement and the split",
        play_lengths(d, repeat)? == before && document(d)?.overrides() == original.overrides(),
        json!({"plays":before}),
        json!({"plays":play_lengths(d, repeat)?}),
    )
}
