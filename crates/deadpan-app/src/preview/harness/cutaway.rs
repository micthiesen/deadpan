//! Specification §31 step 7: a picture-only cutaway from a copied Original
//! moment over part of a beat, with timing and sound unchanged.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let duration = d.app().sequence_length();
    // Copy Original pictures 90..96 into register r.
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::Num9, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num6, Key::L])?;
    d.key_modified(Key::Quote, egui::Modifiers::SHIFT)?;
    d.key(Key::R)?;
    d.key(Key::Y)?;
    d.wait_for("Original moment copied", |app| {
        !app.service.is_busy() && !app.copied.is_pending() && app.copied.original().is_some()
    })?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num1, Key::Num0, Key::L])?;
    let revision = d.revision();
    d.command("cutaway register=r audio=keep")?;
    d.changed(&revision)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let host = d.app().beat_rows.first().ok_or("No beat")?.id.clone();
    let cutaways = workspace.document.nodes()[&host].cutaways.clone();
    let shown = |frame: i64| -> Result<String, String> {
        Ok(format!(
            "{:?}",
            workspace
                .plan
                .picture(ProjectFrame(frame))
                .map_err(|e| e.to_string())?
                .picture
        ))
    };
    let index = workspace
        .sources
        .values()
        .find_map(|source| source.video_index.clone())
        .ok_or("No picture index")?;
    let picture = |frame: i64| -> Result<u64, String> {
        let sample = workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        sample
            .picture
            .select_source_frame(&index)
            .map(|selected| selected.identity.0)
            .map_err(|e| e.to_string())
    };
    let (before, first, last, held, after) = (
        picture(29)?,
        picture(30)?,
        picture(35)?,
        picture(39)?,
        picture(40)?,
    );
    d.check(
        "A cutaway shows the copied pictures over the selected range without changing timing",
        cutaways.len() == 1
            && cutaways[0].range.start().0 == 30
            && cutaways[0].range.end().0 == 40
            && d.app().sequence_length() == duration
            && (before, first, last, held, after) == (29, 90, 95, 95, 40),
        json!({"pictures":[29, 90, 95, 95, 40],"frames":duration}),
        json!({"pictures":[before, first, last, held, after],"frames":d.app().sequence_length(),"cutaways":cutaways.len(),"inside":shown(32)?}),
    )?;
    let painted = scenarios::text_paint_visibility(d, "Cutaways");
    d.check(
        "The inspector lists the beat's cutaways",
        !painted.is_empty(),
        json!("Cutaways row"),
        json!(painted),
    )?;
    d.capture("Cutaway over part of the beat")?;
    let placed = d.revision();
    d.key(Key::U)?;
    d.changed(&placed)?;
    let restored = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()[&host]
        .cutaways
        .is_empty();
    d.check(
        "One undo removes the cutaway",
        restored,
        json!({"cutaways":0}),
        d.snapshot(),
    )?;
    Ok(())
}
