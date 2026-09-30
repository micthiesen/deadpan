//! Real accepted-media preview through the production Open and Sequence routes.

use std::io::Read;

use deadpan_core::{
    CapturedFraming, GeneratedArtifact, HoldVideo, NodeKind, ProjectId, RevisionId,
};
use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema_version: u32,
    fixture: String,
    project_id: ProjectId,
    revision_id: RevisionId,
    artifact: GeneratedArtifact,
    picture_context: CapturedFraming,
    frames: Vec<ExpectedFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedFrame {
    ordinal: u64,
    pts: i64,
    rgba: [u8; 32],
}

impl Fixture {
    fn read(package: &Path) -> Result<Self, String> {
        let path = package
            .parent()
            .ok_or("Fixture package has no parent")?
            .join("generated-picture-fixture.json");
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| format!("Open generated fixture expectations: {error}"))?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 128 * 1024 {
            return Err("Generated fixture expectations exceed 128 KiB".into());
        }
        let fixture: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if fixture.schema_version != 1
            || fixture.frames.len() != 30
            || fixture.frames.iter().enumerate().any(|(ordinal, frame)| {
                u64::try_from(ordinal).ok() != Some(frame.ordinal)
                    || frame.pts != (i64::try_from(ordinal).unwrap() * 1_001_000 + 15_000) / 30_000
            })
        {
            return Err("Generated replay requires the 30-frame canonical fixture manifest".into());
        }
        Ok(fixture)
    }
}

pub(super) fn preflight(package: &Path) -> Result<(), String> {
    let fixture = Fixture::read(package)?;
    if fixture.fixture != "rgb25_24 sampled to 30 frames at 30000/1001"
        || fixture.project_id.as_str() != "project"
        || fixture.revision_id.as_str() != "ui-generated-ready"
    {
        return Err("Project is not the explicitly exported Generated picture fixture".into());
    }
    // Validate before production Open can acquire a writer or recover attempts.
    // The exporter creates one request/attempt; it must already be terminal.
    let store = deadpan_store::ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| format!("Read-only generated fixture preflight: {error}"))?;
    let document = store.snapshot().map_err(|error| error.to_string())?;
    let expected_hold = document.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Hold { recipe }
            if recipe.duration.frames() == 30
                && recipe.picture_context.as_ref() == Some(&fixture.picture_context)
                && matches!(&recipe.video, HoldVideo::Generated { accepted }
                    if accepted.artifact == fixture.artifact))
    });
    if document.project_id() != &fixture.project_id
        || document.revision_id() != &fixture.revision_id
        || document.nodes().len() != 2
        || document.assets().len() != 2
        || !expected_hold
        || store
            .single_source_state()
            .map_err(|error| error.to_string())?
            .is_some()
        || !store
            .current_generation_requests()
            .map_err(|error| error.to_string())?
            .is_empty()
    {
        return Err(
            "Generated fixture head or operational request state changed; regenerate it".into(),
        );
    }
    let request_id = serde_json::from_value(json!("request")).map_err(|error| error.to_string())?;
    let attempts = store
        .generation_attempts(&request_id, 0, 2)
        .map_err(|error| error.to_string())?;
    if attempts.len() != 1 || !attempts[0].checkpoint.state.is_terminal() {
        return Err("Generated fixture must have exactly one completed terminal attempt".into());
    }
    Ok(())
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No accepted workspace")?
        .clone();
    let fixture = Fixture::read(&workspace.path)?;
    let initial_document = workspace.document.clone();
    let hold = workspace.document.nodes().values().find_map(|node| {
        if let NodeKind::Hold { recipe } = &node.kind {
            Some(recipe)
        } else {
            None
        }
    });
    let expected_hold = hold.is_some_and(|recipe| {
        recipe.duration.frames() == 30
            && recipe.picture_context.as_ref() == Some(&fixture.picture_context)
            && matches!(&recipe.video, HoldVideo::Generated { accepted }
                if accepted.artifact == fixture.artifact)
    });
    d.check(
        "Real retained Generated fixture opens in explicit generic compatibility mode",
        workspace.single_source.is_none()
            && workspace.sources.is_empty()
            && workspace.document.project_id() == &fixture.project_id
            && workspace.document.revision_id() == &fixture.revision_id
            && workspace.document.nodes().len() == 2
            && d.app().sequence_length() == 30
            && expected_hold,
        json!({"fixture":fixture.fixture,"mode":"generic compatibility", "qualified_originals":0,"frames":30,"revision":fixture.revision_id}),
        json!({"snapshot":d.snapshot(),"single_original":workspace.single_source.is_some(),"sources":workspace.sources.len(),"nodes":workspace.document.nodes().len()}),
    )?;
    d.report.skipped.extend([
        "This retained compatibility project contains one real accepted Generated Hold and no Original; it does not qualify single-Original creation or an acceptance UI.".into(),
        "The 4×2 canonical fixture exercises real cold admission, decoding, framing and Metal preview. Its timings do not establish representative-resolution media performance, physical display latency, model inference, audio or export.".into(),
    ]);

    let cold_start = completed_samples(d, "input_to_picture_complete_ms").len();
    d.command("sequence")?;
    d.settled()?;
    let cold = completed_samples(d, "input_to_picture_complete_ms")[cold_start..].to_vec();
    d.check(
        "Cold Generated admission completes one picture from the Sequence input",
        cold.len() == 1,
        json!(1),
        json!(cold.len()),
    )?;
    for elapsed in cold {
        d.metric(
            "generated_cold_input_to_picture_complete_ms",
            elapsed,
            SampleOutcome::Completed,
        );
    }
    d.key(egui::Key::J)?;
    d.settled()?;
    assert_frame(d, &fixture, 0)?;
    provider_visible(d)?;
    scenarios::viewer_visible(d)?;
    d.capture("Accepted Generated frame 0 with retained captured framing")?;
    for ordinal in 1..30 {
        d.key(egui::Key::L)?;
        d.settled()?;
        assert_frame(d, &fixture, ordinal)?;
        if ordinal == 7 {
            d.capture("Accepted Generated frame 7 uses the original sampled allocation")?;
        }
    }
    d.capture("Accepted Generated last frame 29")?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.capture("Generated compatibility workspace at minimum size")?;
    d.settled()?;
    assert_frame(d, &fixture, 29)?;
    provider_visible(d)?;
    scenarios::viewer_visible(d)?;
    d.capture("Generated frame 29 settled at minimum size")?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.capture("Generated compatibility workspace at default size")?;
    d.settled()?;
    assert_frame(d, &fixture, 29)?;
    provider_visible(d)?;
    scenarios::viewer_visible(d)?;

    if d.options.mode == RunMode::Performance {
        for _ in 0..8 {
            d.key(egui::Key::H)?;
            d.settled()?;
            d.key(egui::Key::L)?;
            d.settled()?;
        }
        let cpu_start = completed_samples(d, "input_to_state_ms").len();
        let picture_start = completed_samples(d, "input_to_picture_complete_ms").len();
        for _ in 0..60 {
            d.key(egui::Key::H)?;
            d.settled()?;
            assert_frame(d, &fixture, 28)?;
            d.key(egui::Key::L)?;
            d.settled()?;
            assert_frame(d, &fixture, 29)?;
        }
        let cpu = completed_samples(d, "input_to_state_ms")[cpu_start..].to_vec();
        let picture =
            completed_samples(d, "input_to_picture_complete_ms")[picture_start..].to_vec();
        d.check(
            "Warm Generated navigation records one real completion per input",
            cpu.len() == 120 && picture.len() == 120,
            json!({"inputs":120,"cpu_samples":120,"picture_samples":120}),
            json!({"cpu_samples":cpu.len(),"picture_samples":picture.len()}),
        )?;
        for (name, mut samples, limit) in [
            ("generated_warm_navigation_input_cpu_ms", cpu, 8.0),
            (
                "generated_warm_navigation_input_to_picture_complete_ms",
                picture,
                80.0,
            ),
        ] {
            for elapsed in &samples {
                d.metric(name, *elapsed, SampleOutcome::Completed);
            }
            samples.sort_by(f64::total_cmp);
            let p95 = samples
                .get((samples.len() * 95).div_ceil(100).saturating_sub(1))
                .copied();
            d.check(
                &format!("{name} p95 meets the existing navigation budget"),
                samples.len() == 120 && p95.is_some_and(|value| value < limit),
                json!({"samples":120,"p95_below_ms":limit}),
                json!({"samples":samples.len(),"p95_ms":p95}),
            )?;
        }
    }
    d.check(
        "Opening, decoding, resizing and navigating never edit accepted history",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|current| current.document == initial_document),
        json!(fixture.revision_id),
        json!(d.revision()),
    )
}

fn provider_visible(d: &mut Driver<'_>) -> Result<(), String> {
    for label in [
        "Picture",
        "Accepted AI",
        "Sound",
        "Silence",
        "Change duration…  ·  Enter",
        "NORMAL",
        "SEQUENCE",
        "Focus: Viewer",
    ] {
        let paint = scenarios::text_paint_visibility(d, label);
        d.check(
            "Hold controls and workspace mode/focus are visible without scrolling",
            !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
            json!(label),
            json!(paint),
        )?;
    }
    Ok(())
}

fn assert_frame(d: &mut Driver<'_>, fixture: &Fixture, ordinal: usize) -> Result<(), String> {
    let expected = &fixture.frames[ordinal];
    let picture = d
        .app()
        .presentation
        .picture()
        .ok_or("No decoded generated picture")?;
    let frame = picture
        .frame
        .as_ref()
        .ok_or("Generated preview returned a background")?;
    let metadata = frame.metadata();
    let passed = d.app().view == View::Sequence
        && d.app().sequence_cursor == expected.ordinal
        && picture.id.0 == expected.ordinal
        && d.app().presentation.displayed_source_frame() == Some(picture.id)
        && metadata.width == 4
        && metadata.height == 2
        && metadata.row_stride_bytes == 16
        && metadata.pts.ticks == expected.pts
        && metadata.pts.time_base
            == deadpan_core::SourceTimeBase::new(1, 1000).map_err(|error| error.to_string())?
        && frame.bytes() == expected.rgba
        && picture.canvas == Some((1920, 1080))
        && picture.picture_context.as_deref() == Some(&fixture.picture_context)
        && d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id() == &fixture.revision_id);
    let actual = json!({
        "picture":d.app().presentation.diagnostic_snapshot(),
        "cursor":d.app().sequence_cursor, "ordinal":picture.id.0,
        "pts":metadata.pts, "rgba":frame.bytes(), "canvas":picture.canvas,
        "captured_framing":picture.picture_context.as_deref(),
    });
    d.check(
        &format!("Generated frame {ordinal} retains exact pixels, source time and captured geometry"),
        passed,
        json!({"ordinal":expected.ordinal,"pts":expected.pts,"rgba":expected.rgba,"canvas":[1920,1080],"captured_framing":fixture.picture_context}),
        actual,
    )
}

fn completed_samples(d: &Driver<'_>, name: &str) -> Vec<f64> {
    d.report
        .timings
        .iter()
        .filter(|metric| metric.name == name)
        .flat_map(|metric| &metric.samples)
        .filter(|sample| sample.outcome == SampleOutcome::Completed)
        .map(|sample| sample.elapsed_ms)
        .collect()
}
