//! The Diagnostics panel through real keys: `:diagnostics` opens a labelled
//! dialog beside the picture whose rows are accessible "label: value" text,
//! the values resample at 2 Hz without editor input, editor keys never reach
//! the project while it is open, and Escape closes it with focus returned.

use egui::Key;

use super::*;

fn nodes(d: &Driver<'_>) -> Vec<Value> {
    d.harness
        .root()
        .children_recursive()
        .take(4096)
        .map(|node| {
            let access = node.accesskit_node();
            json!({
                "label": access.label(),
                "value": access.value(),
                "role": format!("{:?}", access.role()),
                "focused": access.is_focused(),
            })
        })
        .collect()
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// The accessible text of the row whose label starts with `prefix`.
fn row(d: &Driver<'_>, prefix: &str) -> Option<String> {
    // AccessKit carries a label widget's text as its label or its value,
    // depending on the role egui assigns.
    nodes(d).iter().find_map(|node| {
        ["label", "value"]
            .into_iter()
            .map(|key| text(node, key))
            .find(|text| text.starts_with(prefix))
    })
}

const ROWS: [&str; 8] = [
    "Underruns: ",
    "Picture preview: ",
    "Device packets: ",
    "Submissions: ",
    "Decoded sources: ",
    "Store history rows: read ",
    "Model workers: ",
    "UI updates: ",
];

fn save_report(d: &mut Driver<'_>) -> Result<(), String> {
    for _ in 0..20 {
        if nodes(d)
            .iter()
            .any(|node| node["focused"] == true && node["label"] == "Save diagnostic report…")
        {
            d.key(Key::Enter)?;
            return d.settled();
        }
        d.key(Key::Tab)?;
    }
    Err("Tab never reached Save diagnostic report".into())
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Counters are this replay process's own observations. No audio device, model worker or physical display runs here, so underrun, device-queue and worker-memory rows stay at their idle values; their recording is covered by deadpan-playback, deadpan-jobs and deadpan-native-process tests.".into(),
    );
    let revision = d.revision();
    d.command("diagnostics")?;
    d.settled()?;
    let all = nodes(d);
    let dialog = all
        .iter()
        .find(|node| node["role"] == "Dialog" && node["label"] == "Diagnostics")
        .cloned();
    let focus = all.iter().find(|node| node["focused"] == true).cloned();
    let rows: Vec<Option<String>> = ROWS.iter().map(|prefix| row(d, prefix)).collect();
    d.check(
        "`:diagnostics` opens a labelled dialog with focus on Close and every counter row accessible",
        d.app().diagnostics.open
            && dialog.is_some()
            && focus
                .as_ref()
                .is_some_and(|focus| text(focus, "label") == "Close  Esc")
            && rows.iter().all(Option::is_some),
        json!({"dialog":"Diagnostics","focus":"Close  Esc","rows":ROWS}),
        json!({"open":d.app().diagnostics.open,"dialog":dialog,"focus":focus,"rows":rows}),
    )?;
    let gpu = d.app().diagnostics.latest.map(|sample| sample.counters.gpu);
    d.check(
        "The displayed Original's picture submissions are counted with a completion latency",
        gpu.is_some_and(|gpu| gpu.submissions > 0 && gpu.completions > 0 && gpu.p95_us.is_some()),
        json!("submissions > 0 with a p95"),
        json!(format!("{gpu:?}")),
    )?;
    d.capture("Diagnostics beside the picture")?;

    let frames = row(d, "UI updates: ");
    let first = d.app().diagnostics.refreshes;
    d.wait_for("Diagnostics resampled twice", |app| {
        app.diagnostics.refreshes >= first + 2
    })?;
    d.step("Resampled values painted", false)?;
    let later = row(d, "UI updates: ");
    d.check(
        "Values resample without input and reach the accessible rows",
        frames.is_some() && later.is_some() && frames != later,
        json!("a changed UI updates row"),
        json!({"before":frames,"after":later}),
    )?;

    // Editor keys belong to the panel while it is open.
    d.chord(&[Key::X, Key::J, Key::D, Key::D])?;
    d.settled()?;
    d.check(
        "Editor keys neither edit nor move while Diagnostics is open",
        d.app().diagnostics.open && d.revision() == revision && d.app().transport.is_none(),
        json!({"open":true,"revision":revision}),
        json!({"open":d.app().diagnostics.open,"revision":d.revision()}),
    )?;
    d.capture("Diagnostics after resampling")?;

    let destination = d.options.output.join("diagnostic-report.json");
    d.app_mut().dialogs = crate::dialogs::Dialogs::scripted(vec![
        (DialogKind::DiagnosticReport, None),
        (DialogKind::DiagnosticReport, Some(destination.clone())),
        (DialogKind::DiagnosticReport, Some(destination.clone())),
    ]);
    save_report(d)?;
    d.check(
        "Cancelling diagnostic export creates no file or edit",
        !destination.exists() && d.revision() == revision,
        json!("no file or edit"),
        json!({"file":destination.exists(),"revision":d.revision()}),
    )?;
    save_report(d)?;
    d.wait_for("Diagnostic report saved", |app| {
        app.diagnostics
            .export_status
            .as_deref()
            .is_some_and(|status| status.starts_with("Saved "))
    })?;
    let saved = std::fs::read(&destination).map_err(|error| error.to_string())?;
    let report: Value = serde_json::from_slice(&saved).map_err(|error| error.to_string())?;
    d.check(
        "Keyboard export writes a bounded structural report without editing",
        saved.len() <= deadpan_cli::diagnostic_export::MAX_BYTES
            && report["schema_version"] == 1
            && report["project"]["status"] == "available"
            && d.revision() == revision,
        json!("bounded versioned report, available project, unchanged revision"),
        json!({"bytes":saved.len(),"project":report["project"],"revision":d.revision()}),
    )?;
    save_report(d)?;
    d.wait_for("Diagnostic destination collision reported", |app| {
        app.diagnostics
            .export_status
            .as_deref()
            .is_some_and(|status| status.contains("DiagnosticAlreadyExists"))
    })?;
    d.check(
        "Existing diagnostic reports are preserved with an actionable error",
        std::fs::read(&destination).map_err(|error| error.to_string())? == saved,
        json!("original report unchanged"),
        json!(d.app().diagnostics.export_status),
    )?;
    d.capture("Diagnostic export completed without user content")?;

    d.key(Key::Escape)?;
    d.settled()?;
    for frame in 0..2 {
        d.step(&format!("Diagnostics closed {frame}"), false)?;
    }
    let refreshes = d.app().diagnostics.refreshes;
    let focus = nodes(d).into_iter().find(|node| node["focused"] == true);
    d.check(
        "Escape closes Diagnostics without an edit and returns focus to the workspace",
        !d.app().diagnostics.open
            && d.revision() == revision
            && row(d, "UI updates: ").is_none()
            && focus.is_some(),
        json!({"open":false,"revision":revision,"focus":"a pane"}),
        json!({"open":d.app().diagnostics.open,"revision":d.revision(),"focus":focus}),
    )?;
    d.step("Closed panel idle", false)?;
    d.check(
        "A closed panel stops sampling",
        d.app().diagnostics.refreshes == refreshes,
        json!(refreshes),
        json!(d.app().diagnostics.refreshes),
    )?;
    d.capture("Diagnostics closed")
}
