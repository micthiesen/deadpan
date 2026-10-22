//! Production router/service replay for attached-sound endpoint grouping.
//! Qualified private fixture, synthetic input, and no audio/PCM qualification claim.
use super::*;
use deadpan_core::{
    AudioSample, BeatSound, FrozenAudioLayout, HoldVideo, ProjectDocument, SoundClockJournal,
    SoundId,
};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};

mod fixture;
const NAME: &str = "Sound endpoints \"six\"";
const ORIGINAL_CURSOR: u64 = 31;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    idle(d)?;
    let previous = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No qualified workspace")?
        .path
        .clone();
    let main = document(d)?.clone();
    let original = d.app().source_cursor;
    let size = d.harness.ctx.content_rect().size();
    let workspace = d.app().workspace.as_ref().ok_or("No qualified workspace")?;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|e| e.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("No Original first picture")?;
    let index = workspace
        .sources
        .get(asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No qualified Original picture index")?;
    let measured = picture
        .picture
        .select_source_frame(index)
        .map_err(|e| e.to_string())?;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = scratch.path().join("sound-group-endpoints.deadpan");
    fixture::create(&previous, &path, freeze)?;
    let result = (|| {
        open(d, &path, ORIGINAL_CURSOR)?;
        exercise(d)
    })();
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private sound endpoint project writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        open(d, &previous, original)?;
        resize(d, size.x, size.y)?;
        d.check(
            "Sound endpoint replay restores the main workspace and independent Original cursor",
            equivalent(document(d)?, &main)? && d.app().source_cursor == original,
            json!({"document":"restored","Original":original}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let retained = scratch.keep();
        d.report.skipped.push(format!(
            "Sound endpoint cleanup failed; retained private fixture at {}",
            retained.display(),
        ));
    }
    d.report.skipped.push(
        "Production input events are synthetic. Picture identity runs through the real project/media path; composed readback is visual-mode only. This scenario does not audition, prepare canonical PCM, or qualify listening.".into(),
    );
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; sound endpoint cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn exercise(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let sound = SoundId::new("ui-sound-group-event").map_err(|e| e.to_string())?;
    let owner = baseline
        .beat_sounds()
        .keys()
        .next()
        .ok_or("Missing complete sound owner")?
        .clone();
    let event = baseline.beat_sounds()[&owner][&sound].clone();
    let prior = baseline.audio_bindings().sound_clocks()[&owner][&sound].clone();
    let bank = reader(d)?.registers().map_err(|e| e.to_string())?;
    d.check(
        "Qualified fixture has one complete 120-frame owner, finite ten-frame event, and prior owner-local journal",
        baseline.duration().map_err(|e| e.to_string())?.frames() == 121
            && baseline.node_duration(&owner).map_err(|e| e.to_string())?.frames() == 120
            && event.mapping.duration_frames(deadpan_core::FrameDuration::new(120).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())? == deadpan_core::ExactRatio::integer(10)
            && event.offset == AudioSample(37)
            && prior.scope() == &owner
            && prior.clocks().len() == 1
            && prior.clocks()[0].timing() == &fixture::prefix_timing()?
            && prior.scope() != baseline.root(),
        json!({"frames":121,"owner_frames":120,"event_frames":10,"offset":37,"scope":"owner","prior_clocks":1}),
        d.snapshot(),
    )?;
    let pixels = pictures(d)?;
    d.check(
        "Picture checkpoints cover both grouping seams and terminal Source support",
        pixels
            .iter()
            .map(|(_, source, _)| *source)
            .collect::<Vec<_>>()
            == [3, 4, 9, 10, 119],
        json!({"Edit":[4,5,10,11,120],"Original":[3,4,9,10,119]}),
        json!(pixels),
    )?;
    at(d, 5)?;
    visual(d, 6)?;
    let revision = d.revision();
    let command = format!(
        "group name={}",
        serde_json::to_string(NAME).map_err(|e| e.to_string())?
    );
    // Driver::command enters the real colon field and sends native text/Enter.
    d.command(&command)?;
    d.changed(&revision)?;
    idle(d)?;
    let group = assert_group(d, 5, &event, &prior, &baseline)?;
    let grouped = document(d)?.clone();
    let expected = FrozenAudioLayout::capture(&baseline).map_err(|e| e.to_string())?;
    let reference = &grouped
        .audio_bindings()
        .sound_clocks()
        .values()
        .next()
        .unwrap()[&sound]
        .clocks()[1];
    d.check(
        "All endpoint aliases share the exact complete pre-edit lattice",
        grouped.audio_bindings().timings()[reference.timing()] == expected,
        json!("one complete original pre-edit layout"),
        json!(reference.timing()),
    )?;
    d.check(
        "Grouping preserves the durable register bank and independent Original cursor",
        reader(d)?.registers().map_err(|e| e.to_string())? == bank
            && d.app().source_cursor == ORIGINAL_CURSOR,
        json!({"bank":"unchanged","Original":ORIGINAL_CURSOR}),
        d.snapshot(),
    )?;
    compare_pictures(d, &pixels)?;
    at(d, 5)?;
    resize(d, 960.0, 640.0)?;
    painted(d, NAME)?;
    if scenarios::text_paint_visibility(d, "name group").is_empty() {
        painted(d, "all editor keys")?;
    } else {
        painted(d, "name group")?;
        painted(d, ",g")?;
    }
    scenarios::footer_anchored(
        d,
        "Endpoint group key teaching remains above the compact footer",
    )?;
    d.capture("Sound endpoint group with intact picture and key teaching at 960x640")?;
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "Enter enters the ordinary six-frame group at its exact global Edit range",
        d.app().sequence_scope.groups() == [group.clone()]
            && d.app().scope_start == 5
            && d.app().scope_end == 11
            && document(d)? == &grouped
            && d.app().source_cursor == ORIGINAL_CURSOR,
        json!({"group":group,"range":[5,11],"Original":ORIGINAL_CURSOR}),
        d.snapshot(),
    )?;
    painted(d, NAME)?;
    if scenarios::text_paint_visibility(d, "parent").is_empty() {
        painted(d, "all editor keys")?;
    } else {
        painted(d, "parent")?;
    }
    d.capture("Sound endpoint group breadcrumb and parent key at 960x640")?;
    d.key(Key::Backspace)?;
    d.settled()?;
    d.check(
        "Backspace returns to the same selected group without changing sound clocks",
        d.app().sequence_scope.groups().is_empty()
            && d.app().selected_beat.as_ref() == Some(&group)
            && document(d)? == &grouped,
        json!(group),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    redo(d, &grouped)?;
    undo(d, &baseline)?;

    // Dot must resolve new Visual endpoints, not reuse the old [5,11) crop.
    at(d, 20)?;
    visual(d, 6)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    assert_group(d, 20, &event, &prior, &baseline)?;
    compare_pictures(d, &pixels)?;
    d.check(
        "Fresh Visual dot retains the independent Original cursor and register bank",
        d.app().source_cursor == ORIGINAL_CURSOR
            && reader(d)?.registers().map_err(|e| e.to_string())? == bank,
        json!({"Original":ORIGINAL_CURSOR,"bank":"unchanged"}),
        d.snapshot(),
    )?;
    undo(d, &baseline)
}

fn assert_group(
    d: &mut Driver<'_>,
    start: u64,
    event: &BeatSound,
    prior: &SoundClockJournal,
    before: &ProjectDocument,
) -> Result<NodeId, String> {
    let group = d.app().selected_beat.clone().ok_or("No selected Group")?;
    let document = document(d)?;
    let sound = SoundId::new("ui-sound-group-event").map_err(|e| e.to_string())?;
    let mut timings = std::collections::BTreeSet::new();
    let complete = document.beat_sounds().iter().all(|(owner, events)| {
        let journal = &document.audio_bindings().sound_clocks()[owner][&sound];
        if journal.clocks().len() != 2 {
            return false;
        }
        timings.insert(journal.clocks()[1].timing().clone());
        events.len() == 1
            && &events[&sound] == event
            && document
                .node_duration(owner)
                .is_ok_and(|duration| duration.frames() == 120)
            && document.nodes()[owner].audio_treatments
                == fixture::treatment([3000, -1500, 1000]).unwrap()
            && journal.scope() == owner
            && journal.clocks()[0] == prior.clocks()[0]
            && journal.clocks()[1].scope() == prior.scope()
            && journal.clocks()[1].owner() == prior.scope()
    });
    d.check(
        "Endpoint group preserves complete independent sound recipes and chronological clocks on three aliases",
        document.children(document.root()).any(|node| node == &group)
            && document.nodes()[&group].label == NAME
            && matches!(document.nodes()[&group].kind, NodeKind::Sequence { .. })
            && document.node_duration(&group).map_err(|e| e.to_string())?.frames() == 6
            && document.duration().map_err(|e| e.to_string())?.frames() == 121
            && document.beat_sounds().len() == 3 && complete && timings.len() == 1
            && document.sounds() == before.sounds() && document.sound_routes() == before.sound_routes()
            && reader(d)?.snapshot().map_err(|e| e.to_string())? == *document
            && d.app().sequence_cursor == start && !d.app().edit_range.has_bounds(),
        json!({"group_frames":6,"Edit":start,"total":121,"sound_aliases":3,"clocks_per_alias":2,"new_shared_tables":1}),
        d.snapshot(),
    )?;
    Ok(group)
}

fn open(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for(
        "Open qualified sound endpoint workspace through project service",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path == path)
                && !app.service.is_busy()
        },
    )?;
    d.command("source")?;
    d.chord(&[Key::G, Key::G])?;
    if original > 0 {
        motion(d, original)?;
    }
    at(d, 0)
}

fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.command("sequence")?;
    if d.app().edit_range.has_bounds() {
        d.key(Key::Escape)?;
    }
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        motion(d, frame)?;
    }
    d.settled()?;
    d.check(
        "Production motion lands at exact Edit frame with Sequence focus",
        d.app().sequence_cursor == frame && d.app().pane == Pane::Sequence,
        json!(frame),
        d.snapshot(),
    )
}

fn motion(d: &mut Driver<'_>, count: u64) -> Result<(), String> {
    let mut keys = count
        .to_string()
        .chars()
        .map(|digit| Key::from_name(&digit.to_string()).ok_or("Invalid motion digit".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    keys.push(Key::L);
    d.chord(&keys)
}

fn visual(d: &mut Driver<'_>, count: u64) -> Result<(), String> {
    d.key(Key::V)?;
    motion(d, count)?;
    d.key(Key::V)?;
    d.settled()
}

fn pictures(d: &mut Driver<'_>) -> Result<Vec<(u64, u64, Option<String>)>, String> {
    let mut result = Vec::new();
    for frame in [4, 5, 10, 11, 120] {
        at(d, frame)?;
        let source = d
            .app()
            .presentation
            .displayed_source_frame()
            .ok_or("Missing displayed Original picture at endpoint checkpoint")?
            .0;
        result.push((frame, source, super::group_pixels::fingerprint(d)?));
    }
    Ok(result)
}

fn compare_pictures(
    d: &mut Driver<'_>,
    expected: &[(u64, u64, Option<String>)],
) -> Result<(), String> {
    let actual = pictures(d)?;
    d.check(
        "Endpoint grouping preserves exact picture support across both seams and final picture",
        actual == expected
            && (d.options.mode != RunMode::Visual
                || actual.iter().all(|(_, _, hash)| hash.is_some())),
        json!(expected),
        json!(actual),
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or("No sound endpoint workspace".into())
}

fn reader(d: &Driver<'_>) -> Result<ProjectStore, String> {
    ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No workspace")?.path,
        AccessMode::ReadOnly,
    )
    .map_err(|e| e.to_string())
}

fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|e| e.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|e| e.to_string())?)
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Sound endpoint semantic receipt fully admitted", |app| {
        !app.service.is_busy()
            && !app.macros.is_pending()
            && !app.copied.is_pending()
            && !app.repeat_queue.active()
    })?;
    d.settled()
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Undo restores complete endpoint authored state",
        equivalent(document(d)?, expected)?,
        json!(expected.revision_id()),
        d.snapshot(),
    )
}

fn redo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Redo restores complete endpoint authored state",
        equivalent(document(d)?, expected)?,
        json!(expected.revision_id()),
        d.snapshot(),
    )
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("No replay viewport")?
        .inner_rect = Some(rect);
    d.step("Paint compact sound endpoint workspace", true)
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let parts = scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Sound endpoint key or label is fully painted: {label}"),
        !parts.is_empty() && parts.iter().all(|part| part["fully_visible"] == true),
        json!("within viewport and clip"),
        json!(parts),
    )
}
