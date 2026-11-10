//! `yy` in play 2 and `p` in play 3 through native keys: the copy holds play
//! 2's own beat, the paste changes play 3 only, and Undo restores it.

use super::*;

fn enter_play(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    d.chord(&[Key::G, Key::G])?;
    idle(d)?;
    d.key(Key::Enter)?;
    d.command(&format!("scope play {play}"))?;
    idle(d)?;
    for _ in 0..10 {
        d.key(Key::L)?;
    }
    idle(d)
}

/// The default copy's occurrence plays and length, when it is an Edit copy.
fn copied(d: &Driver<'_>) -> Option<(usize, i64)> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, content)| match content {
            crate::preview::copied::Content::Edited(copied) if register == '"' => Some((
                copied.slice().plays().len(),
                copied.slice().duration().frames(),
            )),
            _ => None,
        })
}

pub(super) fn run(
    d: &mut Driver<'_>,
    repeat: &NodeId,
    before: &[i64],
    original: &ProjectDocument,
    shared_node: &deadpan_core::BeatNode,
) -> Result<(), String> {
    let shared = match &original.nodes()[repeat].kind {
        NodeKind::Repeat { child, .. } => child.clone(),
        _ => return Err("Expected the Repeat".into()),
    };
    // Play 2's contents are one beat; split it so its right part has a group.
    enter_play(d, 2)?;
    let edited = d.revision();
    d.key(Key::S)?;
    d.changed(&edited)?;
    idle(d)?;
    let right = before[1] - 10;
    let unedited = d.revision();
    d.key(Key::Y)?;
    d.key(Key::Y)?;
    d.wait_for(
        "The one-play copy is saved to the default register",
        |app| {
            !app.service.is_busy()
                && !app.macros.is_pending()
                && !app.copied.is_pending()
                && app.copied.entries().any(|(register, content)| {
                    register == '"'
                        && matches!(content, crate::preview::copied::Content::Edited(copied)
                            if !copied.slice().plays().is_empty())
                })
        },
    )?;
    idle(d)?;
    let copy = copied(d);
    d.check(
        "yy inside play 2 copies that play's right part into the default register without an edit",
        copy == Some((1, right)) && d.revision() == unedited && d.app().error.is_none(),
        json!({"plays":1,"frames":right,"revision":"unchanged"}),
        json!({"copy":copy,"error":d.app().error}),
    )?;
    // Play 3 is still one whole beat: there is no group to paste into.
    enter_play(d, 3)?;
    let unedited = d.revision();
    d.key(Key::P)?;
    d.wait_for("The whole-contents paste reports its refusal", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.error.is_some()
    })?;
    idle(d)?;
    let refusal = d.app().error.clone();
    d.check(
        "p on play 3's whole contents refuses readably without an edit",
        refusal
            .as_deref()
            .is_some_and(|error| error.contains("whole contents"))
            && d.revision() == unedited,
        json!({"error":"…whole contents…","revision":"unchanged"}),
        json!({"error":refusal}),
    )?;
    d.app_mut().error = None;
    let edited = d.revision();
    d.key(Key::S)?;
    d.changed(&edited)?;
    idle(d)?;
    let split_end = before[0] + before[1] + before[2];
    let edited = d.revision();
    d.key(Key::P)?;
    d.changed(&edited)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let third = |document: &ProjectDocument| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
            return None;
        };
        let play = iterations.at(2)?;
        let root = document.overrides().get(repeat)?.get(&play)?;
        Some(document.children(root).cloned().collect::<Vec<_>>())
    };
    let siblings = third(document(d)?).unwrap_or_default();
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    let pasted =
        (siblings.len() == 3 && selected.as_ref() == siblings.get(2)).then_some(siblings.len());
    d.check(
        "p inside play 3 pastes play 2's copy after its right part there only and selects it",
        lengths == [before[0], before[1], before[2] + right]
            && pasted == Some(3)
            && d.app().sequence_cursor == u64::try_from(split_end).unwrap_or(0)
            && document(d)?.nodes()[&shared] == *shared_node
            && d.app().error.is_none(),
        json!({"plays":[before[0], before[1], before[2] + right],"siblings":3,"Edit":split_end}),
        json!({"plays":lengths,"siblings":pasted,"Edit":d.app().sequence_cursor,"error":d.app().error}),
    )?;
    painted(d, "This play 3/3")?;
    d.capture("Play 2's copied right part pasted after play 3's right part")?;
    for _ in 0..3 {
        let edited = d.revision();
        d.key(Key::U)?;
        d.changed(&edited)?;
        idle(d)?;
    }
    d.check(
        "Three Undos remove the paste and both splits; the copy remains",
        play_lengths(d, repeat)? == before
            && document(d)?.overrides() == original.overrides()
            && copied(d) == Some((1, right)),
        json!({"plays":before,"copy":[1, right]}),
        json!({"plays":play_lengths(d, repeat)?,"copy":copied(d)}),
    )
}
