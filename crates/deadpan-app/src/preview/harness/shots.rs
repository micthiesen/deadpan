//! Automatic shot detection of the Original, shown under its card.
//!
//! The replay fixture's Original is scanned by the real background job: every
//! picture is decoded from the verified snapshot and measured, and the project
//! service saves the analysis outside history. The rail then shows the shot
//! count under the Original card.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.wait_for("Shots found and saved", |app| {
        app.shots.status_name() == "ready"
            && app
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let shots = workspace.shot_analysis.as_ref().ok_or("No shot analysis")?;
    let pictures = d.app().source_length();
    let count = shots.analysis.boundaries().len() + usize::from(shots.analysis.pictures() > 0);
    let label = if count == 1 {
        format!("{pictures} frames · 1 shot")
    } else {
        format!("{pictures} frames · {count} shots")
    };
    let painted = scenarios::text_paint_visibility(d, &label);
    d.check(
        "Every Original picture is measured and the shot count appears under the Original card without an edit",
        shots.analysis.pictures() as u64 == pictures
            && d.revision() == revision
            && !workspace.can_undo
            && !painted.is_empty()
            && painted.iter().all(|paint| paint["fully_visible"] == true),
        json!({"pictures":pictures,"revision":revision,"label":label,"can_undo":false}),
        json!({"pictures":shots.analysis.pictures(),"revision":d.revision(),"label":painted,"can_undo":workspace.can_undo}),
    )?;
    d.capture("Shot count under the Original card")?;
    shots_in_your_edit(d)?;
    d.report.skipped.push("The replay fixture is a single shot; detection accuracy on cuts is covered by the CLI on synthesized multi-shot clips (docs/SHOT_DETECTION.md).".into());
    Ok(())
}

/// `]s`, `[s` and `diS` in Your edit against a synthetic analysis with cuts at
/// Original pictures 40 and 80.
fn shots_in_your_edit(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let shots = workspace.shot_analysis.as_ref().ok_or("No shot analysis")?;
    let pictures = shots.analysis.pictures();
    let changes = (0..pictures)
        .map(|picture| match picture {
            0 => [0, 0, 0],
            40 | 80 => [120, 200, 120],
            // The picture after a cut still differs from the one before it.
            41 | 81 => [1, 1, 120],
            _ => [1, 1, 1],
        })
        .collect();
    let analysis =
        deadpan_analysis::ShotAnalysis::from_changes(changes).map_err(|e| e.to_string())?;
    let submitted = d.app_mut().submit(ProjectRequest::SaveShotAnalysis {
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
    })?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let runs: Vec<(i64, i64)> = d
        .app_mut()
        .edit_analysis()?
        .shots()
        .ok_or("shots are not projected")?
        .iter()
        .map(|run| (run.range.start().0, run.range.end().0))
        .collect();
    d.chord(&[Key::CloseBracket, Key::S])?;
    d.settled()?;
    let first = d.app().sequence_cursor;
    d.chord(&[Key::CloseBracket, Key::S])?;
    d.settled()?;
    let second = d.app().sequence_cursor;
    d.chord(&[Key::OpenBracket, Key::S])?;
    d.settled()?;
    let back = d.app().sequence_cursor;
    d.check(
        "]s and [s move the Edit cursor between shot starts",
        submitted
            && runs.len() == 3
            && first == runs[1].0 as u64
            && second == runs[2].0 as u64
            && back == runs[1].0 as u64,
        json!({"shots":3,"first":runs.get(1).map(|r| r.0),"second":runs.get(2).map(|r| r.0)}),
        json!({"runs":runs,"first":first,"second":second,"back":back,"message":d.app().message}),
    )?;
    let duration = |d: &Driver<'_>| -> Result<i64, String> {
        Ok(d.app()
            .workspace
            .as_ref()
            .ok_or("No project")?
            .plan
            .duration()
            .frames())
    };
    let before = duration(d)?;
    let revision = d.revision();
    d.key(Key::D)?;
    d.key(Key::I)?;
    d.key_modified(Key::S, egui::Modifiers::SHIFT)?;
    d.changed(&revision)?;
    d.settled()?;
    let after: Vec<(i64, i64)> = d
        .app_mut()
        .edit_analysis()?
        .shots()
        .ok_or("shots are not projected")?
        .iter()
        .map(|run| (run.range.start().0, run.range.end().0))
        .collect();
    let removed = runs[1].1 - runs[1].0;
    d.check(
        "diS cuts exactly the middle shot and the shots after it follow the edit",
        before - duration(d)? == removed
            && after == [runs[0], (runs[2].0 - removed, runs[2].1 - removed)]
            && d.app().message.as_deref() == Some("Cut shot."),
        json!({"removed":removed,"shots":[runs[0], (runs[2].0 - removed, runs[2].1 - removed)],"message":"Cut shot."}),
        json!({"removed":before - duration(d)?,"shots":after,"message":d.app().message}),
    )?;
    d.capture("Shot cut from Your edit")?;
    Ok(())
}
