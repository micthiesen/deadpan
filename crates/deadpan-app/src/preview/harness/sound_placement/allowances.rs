//! Exact pause scope, command capture and native text through the painted app.

use super::*;
use crate::project::sound::pause_target;

pub(super) fn run(d: &mut Driver<'_>, routed: &SoundId, catalog_label: &str) -> Result<(), String> {
    let at =
        ProjectFrame(i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?);
    let gap = pause_target(
        d.app().workspace.as_ref().ok_or("No workspace")?,
        routed,
        at,
    )?;
    d.check(
        "An inserted pause identifies its issuer but has no inherited sound support",
        !gap.selected_support && !gap.allowed,
        json!({"pause":gap.label,"selected_support":false}),
        d.snapshot(),
    )?;
    rejected(
        d,
        "sound-allow",
        "A pause allowance cannot fill a routed timing gap",
    )?;
    d.check(
        "The gap failure explains the unavailable allowance",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("cannot fill a timing gap")),
        json!("No retained selection, not a missing permission"),
        d.snapshot(),
    )?;

    // A newly authored catalog event really spans this existing pause.
    d.click(catalog_label)?;
    commit(d, "sound-place")?;
    let id = selected(d)?;
    let target = pause_target(d.app().workspace.as_ref().ok_or("No workspace")?, &id, at)?;
    d.check(
        "The new sound and the routed sound identify the same exact current pause",
        target.issuer == gap.issuer && target.selected_support && id != *routed,
        json!({"pause":target.label,"selected_support":true}),
        d.snapshot(),
    )?;
    let before_targets = targets(d);
    let nodes = document(d)?.nodes().clone();
    let recipes = document(d)?.sounds().clone();
    let routes = document(d)?.sound_routes().clone();
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.step("Paint exact pause allowance controls after resize", true)?;
        reveal(d, "Allow this sound in pause", -240.0)?;
        visible(d, ":sound-allow")?;
        d.capture("Exact sound and pause permission before grant")?;
        scenarios::footer_anchored(
            d,
            "Pause allowance inspector preserves the workspace footer",
        )?;
    }
    let before = d.revision();
    d.click("Allow this sound in pause")?;
    d.changed(&before)?;
    check_allowed(d, &id, routed, &target.issuer, true)?;
    d.check(
        "Pointer grant changes only this sound's exact pause permission",
        document(d)?.nodes() == &nodes
            && document(d)?.sounds() == &recipes
            && document(d)?.sound_routes() == &routes
            && targets(d) == before_targets,
        json!({"unchanged_picture_and_recipes":true,"targets":before_targets}),
        d.snapshot(),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.step("Paint the granted pause permission after resize", true)?;
        reveal(d, "Silence this sound in pause", -240.0)?;
        visible(d, ":sound-silence")?;
        d.capture("Allowed sound and its revoke control in the identified pause")?;
        scenarios::footer_anchored(d, "Granted pause permission preserves the workspace footer")?;
        check_allowed(d, &id, routed, &target.issuer, true)?;
    }
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    check_allowed(d, &id, routed, &target.issuer, false)?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    check_allowed(d, &id, routed, &target.issuer, true)?;

    choose(d, &id)?;
    let before = d.revision();
    d.key(Key::Colon)?;
    d.events(
        "Native text retains pause commands and editing letters until submission",
        vec![
            egui::Event::Text("sound-silence dd + ".into()),
            key_event(Key::Escape, Modifiers::NONE, true),
            key_event(Key::Escape, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Cancelling native pause command text does not grant, revoke or delete",
        !d.app().command_open && d.app().command == "sound-silence dd + " && d.revision() == before,
        json!({"revision":before,"unchanged_permission":true}),
        d.snapshot(),
    )?;
    check_allowed(d, &id, routed, &target.issuer, true)?;
    d.key(Key::Colon)?;
    d.events(
        "IME composition reserves Enter during a pause command",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "sound-silence".into(),
                active_range_chars: Some(0..13),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Composition Enter cannot submit a pause permission command",
        d.app().command_open && d.revision() == before,
        json!({"command_open":true,"revision":before}),
        d.snapshot(),
    )?;
    d.events(
        "Finish the composed command as native text",
        vec![egui::Event::Ime(egui::ImeEvent::Commit(
            "sound-silence".into(),
        ))],
    )?;
    d.check(
        "IME commit retains the complete text, open field and captured pause target",
        d.app().command_open
            && d.app().command == "sound-silence"
            && d.app().sound_command_target.is_some()
            && d.revision() == before,
        json!({"command":"sound-silence","command_open":true,"revision":before}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    check_allowed(d, &id, routed, &target.issuer, true)?;
    commit(d, ":SOUND-SILENCE")?;
    check_allowed(d, &id, routed, &target.issuer, false)?;
    commit(d, "sound-allow")?;
    check_allowed(d, &id, routed, &target.issuer, true)?;

    // A completed writer mutation arrives while a pause command is open.
    // The late completion must neither close the field nor supply a fresh target.
    for captured in [true, false] {
        choose(d, &id)?;
        let before = d.revision();
        d.app_mut().feedback.hold_project_updates = true;
        d.command(if captured {
            "sound-gain -3"
        } else {
            "sound-gain -6"
        })?;
        d.wait_for("Pause command's earlier writer mutation finishes", |app| {
            !app.service.is_busy()
        })?;
        if !captured {
            d.click("Browse  :source")?;
        }
        d.key(Key::Colon)?;
        d.events(
            "Type silence before a held sound completion",
            vec![egui::Event::Text("sound-silence".into())],
        )?;
        d.check(
            "Pause command captures the original event or its absence",
            d.app().command_open && d.app().sound_command_target.is_some() == captured,
            json!({"captured":captured}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&before)?;
        let revision = d.revision();
        d.check(
            "Late completion preserves native command entry and its captured scope",
            d.app().command_open && d.app().sound_command_target.is_some() == captured,
            json!({"captured":captured,"revision":revision}),
            d.snapshot(),
        )?;
        d.key(Key::Enter)?;
        d.wait_for("Stale pause command reports rejection", |app| {
            !app.service.is_busy() && app.error.is_some()
        })?;
        d.check(
            "An absent or stale pause command cannot adopt the completion's sound",
            d.revision() == revision && document(d)?.nodes() == &nodes,
            json!({"unchanged_revision":revision}),
            d.snapshot(),
        )?;
        check_allowed(d, &id, routed, &target.issuer, true)?;
    }
    choose(d, &id)?;
    reveal(d, "Silence this sound in pause", -240.0)?;
    visible(d, ":sound-silence")?;
    d.capture("Only the selected sound is permitted in the identified pause")?;
    let before = d.revision();
    d.click("Silence this sound in pause")?;
    d.changed(&before)?;
    check_allowed(d, &id, routed, &target.issuer, false)?;
    Ok(())
}

fn check_allowed(
    d: &mut Driver<'_>,
    id: &SoundId,
    other: &SoundId,
    issuer: &deadpan_core::SoundHoldIssuer,
    expected: bool,
) -> Result<(), String> {
    let allowed = |id| {
        document(d).is_ok_and(|document| {
            document
                .sound_allowances()
                .get(id)
                .is_some_and(|set| set.contains(issuer))
        })
    };
    d.check("The allowance belongs to one sound and one exact pause occurrence",
        allowed(id) == expected && !allowed(other),
        json!({"sound":id,"allowed":expected,"other_sound":other,"other_allowed":false,"issuer":issuer}),
        json!(document(d)?.sound_allowances()))
}
