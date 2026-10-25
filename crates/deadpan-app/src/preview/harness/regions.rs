//! Camera object proposals on the original generated blank/mug video through
//! the installed Vision worker. Later edge cases explicitly replace only
//! detector observations with scripted data; they prove no detector capability.
use std::sync::Arc;
use std::time::Duration;

use deadpan_analysis::NormalizedRect;
use deadpan_cli::picture_proposals::{DetectedFace, DetectedSalientRegion, ProposalSelection};
use deadpan_core::{AudioSample, Framing, FramingValue, TargetId};
use egui::{Key, Modifiers};

use super::*;
use crate::project::targets::{
    Backend, ProposalOrigin, ProposalOutcome, ProposalRun, ProposalScript, ProposedTargets,
};

pub(super) fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-track/tests/fixtures/salient-regions.mp4")
}

pub(super) fn audio_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-track/tests/fixtures/salient-audio.wav")
}

/// Ordinary catalog registration and root placement, with no document builder.
fn place_sound(d: &mut Driver<'_>) -> Result<(), String> {
    let fixture = audio_fixture()
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let frames = d.app().sequence_length();
    let nodes = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .nodes()
        .clone();
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    scenarios::choose_sound_interpretation(
        d,
        Some(crate::project::AudioLayoutInterpretation::StereoLeftRight),
    )?;
    d.click("Add sound…  ⌘I")?;
    d.wait_for(
        "Generated audio is qualified in the real sound catalog",
        |app| app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing(),
    )?;
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let asset = d.app().sound_rows[0].0.clone();
    // The stable Original/sounds heading and native j select the first catalog
    // sound even when its row is below the visible rail.
    d.click("Original and sounds pane")?;
    d.key(Key::J)?;
    d.check(
        "The qualified generated WAV is the explicit sound placement target",
        d.app().sound_focused()
            && d.app().selected_sound.as_ref() == Some(&asset)
            && d.app().sequence_cursor == 0,
        json!({"sound":asset,"Edit":0}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("sound-place")?;
    d.changed(&before)?;
    let workspace = d.app().workspace.as_ref().ok_or("No sound workspace")?;
    let event = workspace.document.sounds().values().next();
    let original = original_asset(workspace).ok_or("No Original")?;
    d.check("Native sound placement adds a complete stereo WAV at Edit0 without changing the video-only picture",
        workspace.document.sounds().len() == 1
            && event.is_some_and(|event| event.owner == *workspace.document.root()
                && event.source.asset == asset && event.offset == AudioSample(0)
                && event.source.span.start().ticks == 0
                && event.source.span.end().ticks == 192192)
            && workspace.sources[original].receipt.snapshot().audio().is_none()
            && workspace.document.nodes() == &nodes && d.app().sequence_length() == frames
            && d.app().sequence_cursor == 0,
        json!({"sounds":1,"offset_samples":0,"source_samples":192192,"video_only":true,"picture_frames":frames}),
        json!({"sounds":workspace.document.sounds(),"picture_frames":d.app().sequence_length()}))?;
    super::transcript::focus_your_edit(d)?;
    d.settled()
}

fn audio_snapshot(d: &Driver<'_>) -> Result<Value, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No audio workspace")?;
    let end = workspace
        .plan
        .audio_duration()
        .map_err(|error| error.to_string())?;
    let voice = workspace
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .map_err(|error| error.to_string())?;
    let sounds = workspace.document.sounds().keys().map(|id| {
        let sound = workspace.plan.root_sound(id).map_err(|error| error.to_string())?;
        Ok(json!({"id":id,"event":sound.event(),"audible":sound.audible_samples(),"edges":format!("{:?}",sound.event_edges())}))
    }).collect::<Result<Vec<_>,String>>()?;
    Ok(
        json!({"picture_frames":d.app().sequence_length(),"audio_end":end,
        "voice_spans":voice.spans,"sounds":sounds}),
    )
}

fn mixed(delay: u64) -> ProposalRun {
    // The scripted seam supplies already-canonical wire order, just as a
    // worker must. Deliberately build in reverse first, then order each kind
    // separately; matching geometry across kinds remains two proposals.
    let rectangles = (0..12).rev().map(|index| {
        NormalizedRect::new(
            0.04 + f64::from(index / 2) * 0.15,
            0.12 + f64::from(index % 2) * 0.46,
            0.10,
            0.18,
        )
        .unwrap()
    });
    let mut faces = rectangles
        .clone()
        .map(|region| DetectedFace {
            region,
            confidence: 0.85,
        })
        .collect::<Vec<_>>();
    faces.sort_by(DetectedFace::order);
    let mut salient_regions = rectangles
        .map(|region| DetectedSalientRegion { region })
        .collect::<Vec<_>>();
    salient_regions.sort_by(DetectedSalientRegion::order);
    ProposalRun::Proposals {
        selection: ProposalSelection::Both,
        faces: Some(faces),
        salient_regions: Some(salient_regions),
        delay: Duration::from_millis(delay),
    }
}

pub(super) fn backend() -> Backend {
    Backend::ScriptedProposals(Arc::new(ProposalScript::new([
        ProposalRun::Worker, // real blank
        ProposalRun::Worker, // real mug: accept, then Escape
        ProposalRun::Worker, // real mug: saved entry survives actual proposals
        mixed(600),          // closes before completion
        ProposalRun::Failed {
            reason: "Scripted selected saliency detector failure".into(),
            delay: Duration::ZERO,
        },
        ProposalRun::Proposals {
            selection: ProposalSelection::Faces, // wrong mode for Camera Both
            faces: Some(Vec::new()),
            salient_regions: None,
            delay: Duration::ZERO,
        },
        mixed(400),          // stable digits, ordering, paging
        mixed(0),            // Escape during real target save
        mixed(600),          // cursor change makes completion stale
        mixed(600),          // open Camera loses exact render authority
        mixed(0),            // ordinary f retry after restoring that picture
        ProposalRun::Worker, // explicit region:N remains SalientRegions only
    ])))
}

fn state(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn framing(d: &Driver<'_>) -> Option<Framing> {
    let workspace = d.app().workspace.as_ref()?;
    workspace
        .document
        .nodes()
        .get(d.app().selected_beat.as_ref()?)?
        .framing
        .clone()
}

fn count(d: &Driver<'_>) -> usize {
    d.app()
        .workspace
        .as_ref()
        .map_or(0, |workspace| workspace.document.targets().len())
}

fn target(d: &Driver<'_>, id: &str) -> Option<deadpan_core::AttentionTarget> {
    d.app()
        .workspace
        .as_ref()?
        .document
        .targets()
        .get(&TargetId::new(id).ok()?)
        .cloned()
}

fn found(d: &Driver<'_>) -> Result<ProposedTargets, String> {
    match d
        .app()
        .targets
        .proposals()
        .and_then(|job| job.outcome.as_ref())
    {
        Some(ProposalOutcome::Found(found)) => Ok(found.clone()),
        other => Err(format!("Missing admitted picture proposals: {other:?}")),
    }
}

fn open(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens for region proposals", |app| {
        app.camera.is_some()
    })?;
    d.settled()
}

fn done(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Picture proposal batch concludes", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_some())
            && app
                .camera
                .as_ref()
                .is_none_or(|camera| camera.harness_state()["proposals_pending"] == false)
    })?;
    d.settled()
}

fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Camera continues on the explicitly accepted target revision",
        |app| {
            !app.service.is_busy()
                && app.camera.as_ref().is_some_and(|camera| {
                    let state = camera.harness_state();
                    state["saving"] == false
                        && state["rebasing"] == false
                        && state["phase"] == "Adjust"
                })
        },
    )?;
    d.settled()
}

fn close(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    if d.app().camera.is_some() {
        d.key(Key::Escape)?;
    }
    d.settled()
}

/// Confirm actual final text paint, clip and occlusion, not merely an
/// accessibility node or semantic picker entry. Default replay size is1280x820.
fn visible_proposals(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let current = state(d);
    let entries = current["picker"]
        .as_array()
        .ok_or("No Camera picker for visible proposals")?;
    let paints = entries
        .iter()
        .filter_map(|entry| {
            let number = entry["number"].as_u64()?;
            entry["proposal_kind"].as_str()?;
            let title = entry["label"].as_str()?;
            let text = format!("{number}  {title}");
            let paint = scenarios::text_paint_visibility(d, &text);
            let visible = !paint.is_empty()
                && paint
                    .iter()
                    .all(|part| part["fully_visible"] == true && part["elided"] == false);
            Some(json!({"text":text,"fully_visible":visible,"paint":paint}))
        })
        .collect::<Vec<_>>();
    d.check(
        label,
        !paints.is_empty() && paints.iter().all(|paint| paint["fully_visible"] == true),
        json!("all current-page numbered kind labels fully painted, unelided and unoccluded"),
        json!({"page":current["picker_page"],"labels":paints}),
    )
}

fn select_region(d: &mut Driver<'_>, region: u64) -> Result<(), String> {
    let initial_page = state(d)["picker_page"].clone();
    loop {
        if let Some(number) = state(d)["picker"].as_array().and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["region"] == region)
                .and_then(|entry| entry["number"].as_u64())
        }) {
            let key = match number {
                1 => Key::Num1,
                2 => Key::Num2,
                3 => Key::Num3,
                4 => Key::Num4,
                5 => Key::Num5,
                6 => Key::Num6,
                7 => Key::Num7,
                8 => Key::Num8,
                9 => Key::Num9,
                _ => return Err(format!("Invalid region picker digit {number}")),
            };
            visible_proposals(
                d,
                "Keyboard region choices have visible current-page digits and kind labels",
            )?;
            return d.key(key);
        }
        d.key(Key::Tab)?;
        d.settled()?;
        if state(d)["picker_page"] == initial_page {
            return Err(format!("Region {region} is unreachable by keyboard"));
        }
    }
}

fn durable(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let workspace = d.app().workspace.as_ref().ok_or("No replay workspace")?;
    let store =
        deadpan_store::ProjectStore::open(&workspace.path, deadpan_store::AccessMode::ReadOnly)
            .map_err(|error| error.to_string())?;
    store.validate_full().map_err(|error| error.to_string())?;
    let saved = store.snapshot().map_err(|error| error.to_string())?;
    d.check(
        label,
        saved == *workspace.document,
        json!("SQLite head equals the complete displayed document"),
        json!(saved),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Blank and mug proposal checks use the actual installed Vision worker. The later explicitly scripted ordering, failure, mode, late-result, transient Presentation::render_failed injection and save-cancellation cases qualify guards only. The presentation fault is injected through the existing failure seam; real Camera field previews restore the captured pose/ticket before ordinary f retry. No physical display, detector recall or semantic object recognition is claimed.".into());
    place_sound(d)?;
    let sound = audio_snapshot(d)?;
    d.wait_for("Shot analysis stored for the generated Original", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let start = d.revision();
    open(d)?;
    d.key(Key::F)?;
    done(d)?;
    let blank = found(d)?;
    let runtime = matches!(&blank.origin,
        ProposalOrigin::Worker { faces: Some(faces), salient_regions: Some(regions) }
        if faces.request_revision > 0 && regions.request_revision == 2);
    d.check("Real Both analysis of the blank video picture returns two empty collections without saving",
        runtime && blank.faces.as_ref().is_some_and(Vec::is_empty)
            && blank.salient_regions.as_ref().is_some_and(Vec::is_empty)
            && count(d) == 0 && d.revision() == start
            && state(d)["picker"].as_array().is_some_and(|entries| entries.len() == 5),
        json!({"origin":"worker","face_revision":">0","saliency_revision":2,"faces":0,"regions":0,"targets":0}),
        json!({"found":format!("{blank:?}"),"camera":state(d),"targets":count(d)}))?;
    close(d)?;

    // Edit50 lies in the generated mug section; below, assert the actual
    // decoded ordinal is in [40,80). Request PTS comes from the exact picture.
    d.chord(&[Key::G, Key::G, Key::Num5, Key::Num0, Key::L])?;
    d.settled()?;
    let cursor = (d.app().source_cursor, d.app().sequence_cursor);
    let scope = d.app().sequence_scope.clone();
    let selected = d.app().selected_beat.clone();
    let nodes = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .nodes()
        .clone();
    open(d)?;
    d.key(Key::Plus)?;
    let draft = state(d);
    d.key(Key::F)?;
    done(d)?;
    let mug = found(d)?;
    let source_ordinal = d
        .app()
        .presentation
        .displayed_source_frame()
        .map(|frame| frame.0);
    let mug_runtime = matches!(&mug.origin,
        ProposalOrigin::Worker { faces: Some(faces), salient_regions: Some(regions) }
        if faces.request_revision > 0 && regions.request_revision == 2);
    let region = mug
        .salient_regions
        .as_ref()
        .and_then(|regions| regions.first())
        .ok_or("Real mug worker returned no salient region")?
        .region;
    d.check(
        "The real mug worker finds no faces and proposes a usable non-face region without an edit",
        mug_runtime
            && source_ordinal.is_some_and(|ordinal| (40..80).contains(&ordinal))
            && (0.45..0.7).contains(&region.x())
            && (0.2..0.5).contains(&region.y())
            && (0.2..0.5).contains(&region.width())
            && (0.3..0.65).contains(&region.height())
            && mug.faces.as_ref().is_some_and(Vec::is_empty)
            && mug
                .salient_regions
                .as_ref()
                .is_some_and(|regions| !regions.is_empty())
            && state(d)["phase"] == "TargetPicker"
            && state(d)["draft_pose"] == draft["draft_pose"]
            && state(d)["proposal_origin"]["type"] == "worker"
            && count(d) == 0
            && d.revision() == start,
        json!({"faces":0,"regions":">0","origin":"worker","revision":"unchanged","targets":0}),
        json!({"proposals":format!("{mug:?}"),"camera":state(d)}),
    )?;
    visible_proposals(
        d,
        "Real Both mug proposals show their numbered Region labels in the inspector",
    )?;
    d.capture("Real non-face region picker on the generated mug")?;
    select_region(d, 1)?;
    d.changed(&start)?;
    ready(d)?;
    let accepted = d.revision();
    let expected = deadpan_analysis::target_region(&region);
    let saved = target(d, "region-1").ok_or("Region 1 was not saved")?;
    d.check("A keyboard choice saves exactly one untracked region and preserves scope, cursors and draft scale",
        count(d) == 1 && saved.label == "Region 1" && saved.region == expected.into()
            && saved.samples.is_empty() && saved.provenance.is_none()
            && saved.display_aspect.is_some()
            && framing(d).is_none()
            && d.app().workspace.as_ref().is_some_and(|workspace| workspace.document.nodes() == &nodes)
            && (d.app().source_cursor, d.app().sequence_cursor) == cursor
            && d.app().sequence_scope == scope && d.app().selected_beat == selected
            && state(d)["selected"] == "region-1"
            && state(d)["draft_pose"]["scale"] == draft["draft_pose"]["scale"]
            && audio_snapshot(d)? == sound,
        json!({"target":"region-1","framing":null,"scope_cursors_draft_scale":"unchanged"}),
        json!({"target":saved,"camera":state(d),"cursor":cursor,"snapshot":d.snapshot()}))?;
    d.capture("Real region accepted into the Camera draft")?;
    close(d)?;
    d.check(
        "Escape retains the accepted region and restores exact entry framing",
        d.app().camera.is_none()
            && d.revision() == accepted
            && framing(d).is_none()
            && count(d) == 1,
        json!({"targets":1,"framing":null,"revision":accepted}),
        d.snapshot(),
    )?;
    durable(
        d,
        "The accepted region and escaped framing are durable in SQLite",
    )?;
    d.key(Key::U)?;
    d.changed(&accepted)?;
    d.check(
        "One Undo removes the explicit region acceptance with no hidden framing edit",
        count(d) == 0 && framing(d).is_none(),
        json!({"targets":0,"framing":null}),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.check(
        "Redo restores the exact saved region",
        target(d, "region-1").as_ref() == Some(&saved),
        json!(saved),
        json!(target(d, "region-1")),
    )?;
    durable(d, "The region acceptance Redo is durable")?;

    // Establish an animated Follow through ordinary commands, then accept a
    // second actual detector proposal without replacing that animation.
    open(d)?;
    d.key(Key::F)?;
    let prior_choice = state(d)["picker"][0].clone();
    // F starts another actual Both request; saved entry keeps digit1.
    done(d)?;
    d.check(
        "The real Both completion preserves the saved region identity and digit",
        matches!(found(d)?.origin, ProposalOrigin::Worker { .. })
            && state(d)["picker"][0] == prior_choice
            && prior_choice["saved"] == "region-1"
            && prior_choice["number"] == 1,
        json!(prior_choice),
        state(d)["picker"][0].clone(),
    )?;
    d.key(Key::Num1)?;
    d.key(Key::T)?;
    let before = d.revision();
    d.key(Key::Enter)?;
    d.changed(&before)?;
    d.capture("Real mug region committed as Follow")?;
    let before = d.revision();
    d.command("creep to=1.35")?;
    d.changed(&before)?;
    let animated = framing(d).ok_or("Missing animated Follow")?;
    // The next batches are explicitly scripted guard witnesses.
    scripted(d, &animated)?;
    d.check("All region acceptance, framing, cancellation and Undo/Redo preserve the complete placed sound and audio support",
        audio_snapshot(d)? == sound,
        sound, audio_snapshot(d)?)
}

fn scripted(d: &mut Driver<'_>, animated: &Framing) -> Result<(), String> {
    // This run is delayed, and is intentionally cancelled before its reply.
    let base = d.revision();
    let targets = count(d);
    open(d)?;
    d.key(Key::F)?;
    d.wait_for("Explicit scripted batch is pending", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_none())
    })?;
    close(d)?;
    done(d)?;
    d.check(
        "A late scripted batch cannot reopen Camera or save a target",
        d.app().camera.is_none()
            && d.revision() == base
            && count(d) == targets
            && framing(d).as_ref() == Some(animated),
        json!({"camera":null,"revision":"unchanged","targets":targets}),
        d.snapshot(),
    )?;

    for label in ["Selected detector failure", "Reported mode mismatch"] {
        open(d)?;
        d.key(Key::F)?;
        done(d)?;
        let job = d.app().targets.proposals().cloned();
        d.check(
            label,
            d.revision() == base
                && count(d) == targets
                && state(d)["picker"]
                    .as_array()
                    .is_some_and(|entries| entries.len() == targets + 5)
                && matches!(
                    job.as_ref().and_then(|job| job.outcome.as_ref()),
                    Some(ProposalOutcome::Failed(_))
                )
                && framing(d).as_ref() == Some(animated),
            json!({"failure":true,"saved_and_fixed":"usable","revision":"unchanged"}),
            json!({"job":format!("{job:?}"),"camera":state(d)}),
        )?;
        close(d)?;
    }
    open(d)?;
    d.key(Key::F)?;
    let before = state(d);
    done(d)?;
    let proposals = state(d);
    let entries = proposals["picker"]
        .as_array()
        .ok_or("No proposal entries")?;
    let ordered = entries
        .iter()
        .filter_map(|entry| entry["face"].as_u64())
        .eq(1..=12)
        && entries
            .iter()
            .filter_map(|entry| entry["region"].as_u64())
            .eq(1..=12)
        && entries.iter().position(|entry| entry["region"] == 1)
            > entries.iter().position(|entry| entry["face"] == 12);
    let fixed = before["picker"]
        .as_array()
        .ok_or("No pending fixed entries")?;
    let stable = entries
        .iter()
        .take(targets + 5)
        .eq(fixed.iter().take(targets + 5));
    d.check("Scripted Both batches retain saved/fixed digits and order faces before regions without cross-kind deduplication",
        stable && ordered && entries.len() == targets + 29
            && d.revision() == base && count(d) == targets
            && state(d)["proposal_origin"]["type"] == "scripted",
        json!({"stable_entries":targets + 5,"faces":12,"regions":12,"origin":"scripted"}),
        proposals.clone())?;
    let page = proposals["picker_page"].clone();
    let pose = proposals["draft_pose"].clone();
    d.key(Key::Tab)?;
    d.settled()?;
    let next_page = state(d)["picker_page"].clone();
    visible_proposals(
        d,
        "Tab exposes fully painted numbered Face labels on mixed proposal page two",
    )?;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.settled()?;
    d.check(
        "Tab and Shift Tab page heterogeneous proposals while preserving the animated draft",
        next_page != page
            && state(d)["picker_page"] == page
            && state(d)["draft_pose"] == pose
            && d.revision() == base,
        json!({"paging":"reversible","draft":"unchanged"}),
        state(d),
    )?;
    let expected = found(d)?
        .salient_regions
        .as_ref()
        .and_then(|regions| regions.get(5))
        .map(|region| deadpan_analysis::target_region(&region.region))
        .ok_or("Missing scripted Region 6")?;
    select_region(d, 6)?;
    d.changed(&base)?;
    ready(d)?;
    let target_saved = d.revision();
    d.check(
        "Accepting one scripted region keeps the committed Follow transition untouched until Enter",
        count(d) == targets + 1
            && framing(d).as_ref() == Some(animated)
            && target(d, "region-2").is_some_and(|target| {
                target.label == "Region 6" && target.region == expected.into()
            })
            && state(d)["follow"] == "region-2",
        json!({"targets":targets+1,"draft_follow":"region-2","committed_framing":"unchanged"}),
        json!({"camera":state(d),"framing":framing(d)}),
    )?;
    d.key(Key::Enter)?;
    d.changed(&target_saved)?;
    let committed = framing(d).ok_or("No committed region Follow")?;
    let mut expected = animated.clone();
    if let FramingValue::Follow { target, .. } = &mut expected.value {
        *target = TargetId::new("region-2").unwrap();
    } else {
        return Err("Expected an animated Follow fixture".into());
    }
    d.check("Enter changes only the Follow target and preserves the entire clock, scale, fallback and transition",
        committed == expected && count(d) == targets+1,
        json!(expected), json!(committed))?;
    d.capture("Scripted region committed with the full Follow creep")?;
    durable(d, "The region Follow commit is durable")?;
    let exact_region = target(d, "region-2").ok_or("Missing region before delete")?;
    let framed = d.revision();
    d.command("target-delete region-2")?;
    d.settled()?;
    d.check(
        "Deleting a followed detected region refuses without history",
        d.revision() == framed
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("is used by")),
        json!({"delete":"refused","revision":framed}),
        d.snapshot(),
    )?;
    d.key(Key::U)?;
    d.changed(&framed)?;
    d.check(
        "Undo restores the prior animation while retaining both accepted targets",
        framing(d).as_ref() == Some(animated) && count(d) == targets + 1,
        json!({"framing":"entry animation","targets":targets+1}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("target-delete region-2")?;
    d.changed(&before)?;
    d.check(
        "An unused detected region deletes as one ordinary target edit",
        target(d, "region-2").is_none()
            && count(d) == targets
            && framing(d).as_ref() == Some(animated),
        json!({"targets":targets,"framing":"unchanged"}),
        d.snapshot(),
    )?;
    let deleted = d.revision();
    d.key(Key::U)?;
    d.changed(&deleted)?;
    d.check(
        "Undo restores the deleted exact region geometry",
        target(d, "region-2").as_ref() == Some(&exact_region),
        json!(exact_region),
        json!(target(d, "region-2")),
    )?;
    // Cancellation while an actual target save is outstanding: the acceptance
    // still lands, while Escape closes Camera and leaves entry framing intact.
    open(d)?;
    d.key(Key::F)?;
    done(d)?;
    let before = d.revision();
    let count_before = count(d);
    d.app_mut().feedback.hold_project_updates = true;
    select_region(d, 1)?;
    d.key(Key::Escape)?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&before)?;
    d.check("Escape during an explicit region save leaves Camera closed and retains that acceptance only",
        d.app().camera.is_none() && count(d) == count_before+1 && framing(d).as_ref() == Some(animated),
        json!({"camera":null,"targets":count_before+1,"framing":"unchanged"}), d.snapshot())?;
    durable(
        d,
        "Save cancellation retains the committed region in SQLite",
    )?;

    let base = d.revision();
    let count_before = count(d);
    open(d)?;
    d.key(Key::F)?;
    d.wait_for("Scripted batch pending before navigation", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_none())
    })?;
    close(d)?;
    d.key(Key::L)?;
    done(d)?;
    d.check("A scripted reply after closing Camera and changing the exact displayed picture saves nothing",
        d.app().camera.is_none() && d.revision() == base && count(d) == count_before,
        json!({"revision":"unchanged","targets":count_before}), d.snapshot())?;
    stale_pending_retry(d)?;
    atomic_region(d)
}

/// Script only a transient presentation failure through its existing public
/// failure seam. The original decoded picture/ticket is retained. Detector
/// observations are explicitly scripted; real service completion and Camera's
/// production admission/release path must handle them.
fn stale_pending_retry(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let targets = count(d);
    let entry_framing = framing(d);
    let sound = audio_snapshot(d)?;
    let workspace = d.app().workspace.as_ref().ok_or("No retry workspace")?;
    let session = workspace.session;
    let head = workspace.document.revision_id().clone();
    let cursor =
        ProjectFrame(i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?);
    let ticket = d
        .app()
        .presentation
        .stable_sequence_ticket(session, &head, cursor)
        .ok_or("Retry fixture has no actual submitted stopped picture")?;
    open(d)?;
    let captured = state(d);
    let pose: deadpan_core::FramingPose = serde_json::from_value(captured["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.key(Key::F)?;
    d.wait_for("Scripted Both is pending while Camera stays open", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_none())
            && app
                .camera
                .as_ref()
                .is_some_and(|camera| camera.harness_state()["proposals_pending"] == true)
    })?;
    let pending = d
        .app()
        .targets
        .proposals()
        .ok_or("No pending retry batch")?
        .ticket;
    let entries = state(d)["picker"].clone();
    d.app_mut().presentation.render_failed(
        "Scripted replay: transient GPU presentation failure while proposals finish".into(),
    );
    d.check("An explicitly injected presentation failure revokes the captured ticket without closing Camera",
        d.app().camera.is_some()
            && d.app().presentation.stable_sequence_ticket(session, &head, cursor).is_none()
            && state(d)["proposals_pending"] == true
            && d.revision() == revision,
        json!({"injection":"scripted presentation failure","Camera":"open","stable_ticket":null}),
        json!({"presentation":d.app().presentation.diagnostic_snapshot(),"camera":state(d)}))?;
    // Do not call settled here: its normal error-free presentation contract is
    // deliberately false until the ordinary Camera field preview repairs it.
    d.wait_for(
        "The completed stale batch releases its own pending Camera ticket",
        |app| {
            app.targets
                .proposals()
                .is_some_and(|job| job.ticket == pending && job.outcome.is_some())
                && app
                    .camera
                    .as_ref()
                    .is_some_and(|camera| camera.harness_state()["proposals_pending"] == false)
        },
    )?;
    let rejected = state(d);
    d.check("Real Camera completion rejects stale proposals, clears pending and retains every saved/fixed entry",
        rejected["phase"] == "TargetPicker" && rejected["picker"] == entries
            && rejected["proposal_origin"].is_null()
            && rejected["proposals_status"].as_str().is_some_and(|status| status.contains("captured picture changed"))
            && rejected["draft_pose"] == captured["draft_pose"]
            && d.revision() == revision && count(d) == targets
            && framing(d) == entry_framing && audio_snapshot(d)? == sound
            && matches!(found(d)?.origin, ProposalOrigin::Scripted),
        json!({"batch":pending,"pending":false,"admitted_proposals":0,"entries":"unchanged","revision":"unchanged"}),
        json!({"camera":rejected,"presentation":d.app().presentation.diagnostic_snapshot()}))?;

    // Ordinary f toggles the picker back to Adjust without cancelling Camera.
    // Two normal numeric-field edits then force actual GPU submissions and
    // restore the captured Q32 scale; the full pose/ticket are checked below.
    d.key(Key::F)?;
    d.check("Ordinary f leaves the same captured Camera in Adjust before repairing presentation",
        state(d)["phase"] == "Adjust" && state(d)["revision"] == head.as_str()
            && state(d)["draft_pose"] == captured["draft_pose"]
            && state(d)["follow"] == captured["follow"]
            && state(d)["selected"] == captured["selected"]
            && d.app().workspace.as_ref().is_some_and(|workspace|
                workspace.session == session && workspace.document.revision_id() == &head)
            && d.app().presentation.decoded_ticket() == Some(ticket)
            && d.app().presentation.displayed_sequence() == Some((cursor, ticket))
            && d.app().targets.proposals().is_some_and(|job| job.ticket == pending)
            && d.revision() == revision && framing(d) == entry_framing,
        json!({"phase":"Adjust","session":session,"revision":head,"ticket":format!("{ticket:?}"),"draft":captured["draft_pose"]}),
        json!({"camera":state(d),"presentation":d.app().presentation.diagnostic_snapshot()}))?;
    d.click("Framing scale, percent of input size")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Repair the scripted presentation failure with a normal Camera field preview",
        vec![
            egui::Event::Text("175".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.settled()?;
    let percent = 100.0 * pose.scale.numerator() as f64 / pose.scale.denominator() as f64;
    d.click("Framing scale, percent of input size")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Restore the exact captured Camera scale through the numeric field",
        vec![
            egui::Event::Text(format!("{percent:.20}")),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.settled()?;
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("No repaired stopped picture")?;
    d.click(&label)?;
    d.check("Normal Camera previews restore the complete captured pose and actual stopped ticket without an authored edit",
        state(d)["draft_pose"] == captured["draft_pose"]
            && state(d)["follow"] == captured["follow"]
            && state(d)["selected"] == captured["selected"]
            && d.app().presentation.stable_sequence_ticket(session, &head, cursor) == Some(ticket)
            && d.revision() == revision && framing(d) == entry_framing,
        json!({"ticket":format!("{ticket:?}"),"draft":captured["draft_pose"],"revision":revision}),
        json!({"camera":state(d),"presentation":d.app().presentation.diagnostic_snapshot()}))?;
    d.key(Key::F)?;
    done(d)?;
    d.check("Ordinary f after stale completion starts a fresh batch and admits only its explicitly scripted proposals",
        d.app().targets.proposals().is_some_and(|job| job.ticket > pending && job.outcome.is_some())
            && state(d)["proposals_pending"] == false
            && state(d)["proposal_origin"]["type"] == "scripted"
            && state(d)["picker"].as_array().is_some_and(|entries| entries.len() == targets + 29)
            && d.revision() == revision && count(d) == targets
            && framing(d) == entry_framing && audio_snapshot(d)? == sound,
        json!({"fresh_ticket":format!(">{pending}"),"faces":12,"regions":12,"saved_targets":targets,"revision":"unchanged"}),
        state(d))?;
    d.capture("Fresh region proposals after a stale completed Camera batch")?;
    close(d)
}

// Explicit sampled commands are distinct from Camera's two-step acceptance.
fn atomic_region(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let before = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .clone();
    let prior_ticket = d.app().targets.proposals().map(|job| job.ticket);
    d.app_mut().error = None;
    d.chord(&[Key::Q, Key::Z])?;
    d.command("zoom 1.2 target=region:1 curve=step")?;
    d.settled()?;
    d.check(
        "A sampled region:N target refuses macro recording before detector work or authored edits",
        d.app().macros.recording()
            && d.revision() == revision
            && d.app().targets.proposals().map(|job| job.ticket) == prior_ticket
            && d.app().targets.proposal_zoom.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("record")),
        json!({"detector":"not started","revision":"unchanged","recording":true}),
        d.snapshot(),
    )?;
    // Explicit native discard is valid for an empty recording; Q attempts
    // to save and must correctly refuse an empty instruction list.
    d.command("record-cancel")?;
    d.wait_for(
        "Region macro refusal recording is explicitly discarded",
        |app| !app.macros.recording() && !app.macros.is_pending(),
    )?;
    d.check("Native record-cancel discards the empty refused macro without starting detection or changing the edit",
        d.revision() == revision
            && d.app().targets.proposals().map(|job| job.ticket) == prior_ticket
            && d.app().workspace.as_ref().is_some_and(|workspace|
                workspace.document.targets() == before.targets() && workspace.document.nodes() == before.nodes()),
        json!({"recording":false,"revision":"unchanged","detector":"not started"}), d.snapshot())?;
    d.command("zoom 1.2 target=region:1 curve=step")?;
    d.wait_for("Atomic region command finishes", |app| {
        app.targets.proposal_zoom.is_none()
            && app
                .targets
                .proposals()
                .is_some_and(|job| job.outcome.is_some())
            && !app.service.is_busy()
    })?;
    d.changed(&revision)?;
    let proposals = found(d)?;
    let region = proposals
        .salient_regions
        .as_ref()
        .and_then(|regions| regions.first())
        .ok_or("Real region:N returned no region")?;
    let saved = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .clone();
    let added = saved
        .targets()
        .iter()
        .filter(|(id, _)| !before.targets().contains_key(*id))
        .collect::<Vec<_>>();
    let expected = deadpan_analysis::target_region(&region.region);
    d.check("Real region:N requests only saliency and atomically saves one region plus sampled framing",
        matches!(proposals.origin, ProposalOrigin::Worker { faces: None, salient_regions: Some(_) })
            && proposals.faces.is_none() && added.len() == 1
            && added[0].1.region == expected.into()
            && matches!(framing(d), Some(Framing { value: FramingValue::Follow { target, scale, scale_transition:None, .. }, .. })
                if target == *added[0].0 && scale == deadpan_core::ExactRatio::new(6,5).unwrap()),
        json!({"mode":"salient_regions","targets_added":1,"scale":"6/5","transition":null}),
        json!({"proposals":format!("{proposals:?}"),"added":added,"framing":framing(d)}))?;
    d.capture("Real region command atomically committed")?;
    let committed = d.revision();
    d.key(Key::U)?;
    d.changed(&committed)?;
    d.check(
        "One Undo removes both atomic region:N changes and restores the preceding animation",
        d.app().workspace.as_ref().is_some_and(|workspace| {
            workspace.document.targets() == before.targets()
                && workspace.document.nodes() == before.nodes()
        }),
        json!("exact prior targets and nodes"),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.check(
        "One Redo durably restores the atomic region:N target and framing",
        d.app().workspace.as_ref().is_some_and(|workspace| {
            workspace.document.targets() == saved.targets()
                && workspace.document.nodes() == saved.nodes()
        }),
        json!("exact committed targets and nodes"),
        d.snapshot(),
    )?;
    durable(
        d,
        "The atomic region:N Redo and its history replay validate in SQLite",
    )
}
