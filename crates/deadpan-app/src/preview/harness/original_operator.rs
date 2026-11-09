//! `y` + motion or object in the Original through production keys: the copied
//! half-open moment, register choice, unchanged cursor and refusals.

use super::*;
use crate::preview::copied::Content;
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.command("source")?;
    at(d, 10)?;
    d.key(Key::Y)?;
    d.settled()?;
    let hint = scenarios::text_paint_visibility(d, "l copies forward in frames");
    d.check(
        "Without a selected moment, y waits for an Original motion and teaches its next keys",
        d.app().bindings.pending() == "y"
            && d.app().bindings.operator_pending()
            && !hint.is_empty()
            && hint.iter().all(|paint| paint["fully_visible"] == true),
        json!({"pending":"y","hint":"l copies forward in frames"}),
        json!({"pending":d.app().bindings.pending(),"hint":hint}),
    )?;
    d.capture("Pending Original yank operator")?;
    d.chord(&[Key::Num3, Key::L])?;
    saved(d)?;
    copied(
        d,
        "y3l copies [10,13) and leaves the cursor and selection alone",
        '"',
        10..13,
        10,
        &revision,
    )?;
    d.capture("Original y3l copied")?;

    d.chord(&[Key::Num3, Key::Y, Key::H])?;
    saved(d)?;
    copied(
        d,
        "A count before the operator copies backward: 3yh is [7,10)",
        '"',
        7..10,
        10,
        &revision,
    )?;

    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    d.chord(&[Key::A, Key::Y, Key::Num2, Key::L])?;
    saved(d)?;
    copied(
        d,
        "\"ay2l copies [10,12) into register a",
        'a',
        10..12,
        10,
        &revision,
    )?;
    d.check(
        "A named copy consumes its one-shot register choice",
        d.app().copied.selected().is_none(),
        json!({"selected":null}),
        d.snapshot(),
    )?;

    let length = d.app().source_length();
    d.key(Key::Y)?;
    d.key_modified(Key::G, Modifiers::SHIFT)?;
    saved(d)?;
    copied(
        d,
        "yG copies to the Original end",
        '"',
        10..length,
        10,
        &revision,
    )?;
    d.chord(&[Key::Y, Key::G, Key::G])?;
    saved(d)?;
    copied(
        d,
        "ygg copies from the Original start",
        '"',
        0..10,
        10,
        &revision,
    )?;

    let before = range(d, '"');
    d.chord(&[Key::Num2, Key::Y, Key::Num3, Key::L])?;
    d.settled()?;
    d.check(
        "Two counts refuse without copying",
        d.app().error.as_deref().is_some_and(|error| error.contains("not both"))
            && range(d, '"') == before
            && d.app().source_cursor == 10,
        json!({"error":"Use one count before the operator or before its motion, not both.","copy":before}),
        json!({"error":d.app().error,"copy":range(d, '"'),"cursor":d.app().source_cursor}),
    )?;
    at(d, 0)?;
    d.chord(&[Key::Y, Key::H])?;
    d.settled()?;
    d.check(
        "An empty interval refuses and keeps the previous copy",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("selects no Original time"))
            && range(d, '"') == before
            && d.app().source_cursor == 0,
        json!({"error":"The motion selects no Original time; nothing was copied.","copy":before}),
        json!({"error":d.app().error,"copy":range(d, '"'),"cursor":d.app().source_cursor}),
    )?;

    shots(d)?;
    at(d, 10)?;
    d.chord(&[Key::Y, Key::CloseBracket, Key::S])?;
    saved(d)?;
    copied(
        d,
        "y]s copies to the next detected shot start",
        '"',
        10..40,
        10,
        &revision,
    )?;
    at(d, 50)?;
    d.chord(&[Key::Y, Key::I])?;
    d.key_modified(Key::S, Modifiers::SHIFT)?;
    saved(d)?;
    copied(
        d,
        "yiS copies the shot under the cursor",
        '"',
        40..80,
        50,
        &revision,
    )?;
    d.capture("Original yiS copied")?;

    at(d, 20)?;
    d.chord(&[Key::V, Key::Num2, Key::L, Key::Y])?;
    saved(d)?;
    d.check(
        "With a selected moment y still copies it at once",
        range(d, '"') == Some(20..22) && d.app().bindings.pending().is_empty(),
        json!({"copy":[20,22]}),
        json!({"copy":range(d, '"'),"pending":d.app().bindings.pending()}),
    )?;
    d.key(Key::Escape)?;
    Ok(())
}

fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        for digit in frame.to_string().chars() {
            d.key(Key::from_name(&digit.to_string()).ok_or("Invalid replay count key")?)?;
        }
        d.key(Key::L)?;
    }
    d.settled()
}

fn saved(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Original copy is saved by the project service", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })
}

fn range(d: &Driver<'_>, name: char) -> Option<std::ops::Range<u64>> {
    d.app()
        .copied
        .entries()
        .find_map(|(key, value)| match value {
            Content::Original(copied) if key == name => Some(copied.ordinals.clone()),
            _ => None,
        })
}

fn copied(
    d: &mut Driver<'_>,
    label: &str,
    register: char,
    expected: std::ops::Range<u64>,
    cursor: u64,
    revision: &str,
) -> Result<(), String> {
    let actual = range(d, register);
    d.check(
        label,
        actual.as_ref() == Some(&expected)
            && d.app().source_cursor == cursor
            && d.app().moment.range().is_none()
            && !d.app().moment.active
            && d.app().bindings.pending().is_empty()
            && d.revision() == revision,
        json!({"register":register,"copy":[expected.start,expected.end],"cursor":cursor,"selection":null,"revision":revision}),
        json!({"copy":actual,"cursor":d.app().source_cursor,"selection":d.app().moment.range(),"error":d.app().error,"revision":d.revision()}),
    )
}

/// Synthetic shot cuts at Original pictures 40 and 80, as in the `shots` replay.
fn shots(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shots found and saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let shots = workspace.shot_analysis.as_ref().ok_or("No shot analysis")?;
    let changes = (0..shots.analysis.pictures())
        .map(|picture| match picture {
            0 => [0, 0, 0],
            40 | 80 => [120, 200, 120],
            41 | 81 => [1, 1, 120],
            _ => [1, 1, 1],
        })
        .collect();
    let analysis =
        deadpan_analysis::ShotAnalysis::from_changes(changes).map_err(|e| e.to_string())?;
    d.app_mut().submit(ProjectRequest::SaveShotAnalysis {
        expected_session: workspace.session,
        attempt: 0,
        key: shots.key.clone(),
        analysis: Arc::new(analysis),
    });
    d.wait_for("Synthetic shots saved", |app| {
        app.workspace.as_ref().is_some_and(|workspace| {
            workspace
                .shot_analysis
                .as_ref()
                .is_some_and(|shots| shots.analysis.boundaries() == [40, 80])
        })
    })
}
