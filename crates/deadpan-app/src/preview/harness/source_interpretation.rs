//! Real commands, revision changes and corrected GPU pictures without a native window.

use super::*;
use egui::Key;

fn selection(d: &Driver<'_>) -> Value {
    let app = d.app();
    json!({"view":format!("{:?}", app.view), "pane":format!("{:?}", app.pane),
        "source":app.source_cursor, "edit":app.sequence_cursor, "beat":app.selected_beat,
        "range":app.selected_edit_range(), "visual":app.edit_range.active})
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.settled()?;
    let baseline = d.app().workspace.clone().ok_or("No project")?;
    let asset = original_asset(&baseline).ok_or("No Original")?.clone();
    d.command("source")?;
    d.chord(&[Key::L, Key::L, Key::L])?;
    d.settled()?;
    let original_selection = selection(d);
    let original_pixels = super::group_pixels::fingerprint(d)?;
    let revision = d.revision();
    d.command("source-interpret")?;
    d.check(
        "Inspecting interpretation reports the current values without editing",
        d.revision() == revision
            && d.app()
                .message
                .as_ref()
                .is_some_and(|m| m.contains("range=auto chroma=auto")),
        json!("automatic interpretation; unchanged revision"),
        d.snapshot(),
    )?;
    d.command("source-interpret range=full chroma=center")?;
    d.changed(&revision)?;
    d.settled()?;
    let corrected = d.app().workspace.clone().ok_or("No corrected project")?;
    let corrected_pixels = super::group_pixels::fingerprint(d)?;
    let choice = deadpan_core::SourceInterpretation {
        range: Some(deadpan_core::SourceColorRange::Full),
        chroma_location: Some(deadpan_core::SourceChromaLocation::Center),
    };
    d.check("Correcting the Original changes its displayed pixels and label while retaining clocks, pane, beat and immutable media",
        selection(d) == original_selection && corrected.document.source_interpretations().get(&asset) == Some(&choice)
            && corrected.document.assets() == baseline.document.assets()
            && corrected.document.nodes() == baseline.document.nodes()
            && corrected.sources[&asset].receipt == baseline.sources[&asset].receipt
            && (original_pixels.is_none() || original_pixels != corrected_pixels)
            && d.app().presentation.displayed_label().is_some_and(|label| label.contains("Source range corrected to full") && label.contains("chroma position corrected to center")),
        json!({"selection":original_selection,"changed_pixels":true}),
        json!({"selection":selection(d),"original_pixels":original_pixels,"corrected_pixels":corrected_pixels,"caption":d.app().presentation.displayed_label()}))?;
    d.capture("Corrected Original and discoverable interpretation control")?;
    d.click("Interpret picture…  :source-interpret")?;
    d.check(
        "The Original control opens the current values in native command entry",
        d.app().command_open && d.app().command == "source-interpret range=full chroma=center",
        json!("source-interpret range=full chroma=center"),
        json!(d.app().command),
    )?;
    d.key(Key::Escape)?;

    // Retained Visual selection stays valid through this source-only revision.
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::L, Key::L, Key::V, Key::L, Key::L])?;
    let before_range = selection(d);
    let revision = d.revision();
    d.command("source-interpret chroma=top-left")?;
    d.changed(&revision)?;
    d.settled()?;
    d.check(
        "A chroma-only correction retains the existing range choice and Visual selection",
        selection(d) == before_range
            && d.app().selected_edit_range().is_some()
            && d.app()
                .workspace
                .as_ref()
                .unwrap()
                .document
                .source_interpretations()[&asset]
                .range
                == Some(deadpan_core::SourceColorRange::Full),
        before_range,
        selection(d),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    super::transcript::model_offer_visible(d)?;

    // A newer commit while text is open cannot supply a new target or revision.
    d.key(Key::Colon)?;
    let workspace = d.app().workspace.clone().unwrap();
    d.app()
        .service
        .submit(ProjectRequest::InterpretSource {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset.clone(),
            interpretation: None,
        })
        .map_err(|e| e.to_string())?;
    d.changed(&workspace.document.revision_id().to_string())?;
    let reset_revision = d.revision();
    d.events(
        "Submit correction captured before a concurrent revision",
        vec![
            egui::Event::Text("source-interpret range=full".into()),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "A stale command refuses without modifying the newer revision",
        d.revision() == reset_revision
            && d.app()
                .error
                .as_ref()
                .is_some_and(|e| e.contains("project changed")),
        json!("stale command refused"),
        d.snapshot(),
    )?;
    d.command("source")?;
    d.settled()?;
    let reset_pixels = super::group_pixels::fingerprint(d)?;
    d.check(
        "Reset restores the same Original pixels at the retained Original cursor",
        original_pixels == reset_pixels
            && d.app()
                .workspace
                .as_ref()
                .unwrap()
                .document
                .source_interpretations()
                .is_empty(),
        json!(original_pixels),
        json!(reset_pixels),
    )?;
    let revision = d.revision();
    d.command("source-interpret reset")?;
    d.check(
        "Resetting automatic interpretation again creates no history",
        d.revision() == revision,
        json!(revision),
        json!(d.revision()),
    )?;
    let package = d.app().workspace.as_ref().unwrap().path.clone();
    d.command("close")?;
    d.wait_for("Project closed", |app| app.workspace.is_none())?;
    d.key(Key::Colon)?;
    // Opening is asynchronous; command entry has already captured absence.
    d.app()
        .service
        .submit(ProjectRequest::Open(package))
        .map_err(|e| e.to_string())?;
    d.wait_for(
        "Project reopened while interpretation entry is pending",
        |app| app.workspace.is_some() && !app.service.is_busy(),
    )?;
    let reopened_revision = d.revision();
    d.events(
        "Submit interpretation entered without a source",
        vec![
            egui::Event::Text("source-interpret range=full".into()),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "A late project open cannot supply a source absent at command entry",
        d.revision() == reopened_revision
            && d.app()
                .error
                .as_ref()
                .is_some_and(|e| e.contains("ready Original first")),
        json!("missing captured source refused"),
        d.snapshot(),
    )?;
    Ok(())
}
