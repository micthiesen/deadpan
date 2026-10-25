//! Specification §6.4 `:zoom 1.35 target=face:2 curve=step` and §7.6 numbered
//! detected regions, through the production command line, project service,
//! face-detection job and shared picture plan.
//!
//! `faces` uses the explicit proposal seam (`Backend::ScriptedProposals`):
//! the first detection runs the real installed `deadpan-track picture-proposals (Faces)`
//! worker with Apple Vision on the replay fixture, which shows no face; later
//! detections replace only the worker's reported faces. Picture resolution
//! against the qualified index, the verified Original copy, the host's face
//! admission, the stale-context guard and the one-Compound save are real.

use std::sync::Arc;
use std::time::Duration;

use deadpan_analysis::NormalizedRect;
use deadpan_cli::faces::DetectedFace;
use deadpan_core::{ExactRatio, Framing, FramingValue, TargetId};
use egui::Key;

use super::*;
use crate::project::targets::{Backend, ProposalRun, ProposalScript};
use deadpan_cli::picture_proposals::ProposalSelection;

mod camera_picker;

/// Two proposals: a face left of center and a smaller one right of center.
fn proposed() -> Vec<DetectedFace> {
    vec![
        DetectedFace {
            region: NormalizedRect::new(0.15, 0.3, 0.2, 0.35).unwrap(),
            confidence: 0.82,
        },
        DetectedFace {
            region: NormalizedRect::new(0.62, 0.25, 0.16, 0.28).unwrap(),
            confidence: 0.77,
        },
    ]
}

pub(super) fn backend() -> Backend {
    let scripted = |delay| ProposalRun::Proposals {
        selection: ProposalSelection::Faces,
        faces: Some(proposed()),
        salient_regions: None,
        delay: Duration::from_millis(delay),
    };
    let camera = |faces, delay| ProposalRun::Proposals {
        selection: ProposalSelection::Both,
        faces: Some(faces),
        salient_regions: Some(Vec::new()),
        delay: Duration::from_millis(delay),
    };
    Backend::ScriptedProposals(Arc::new(ProposalScript::new([
        ProposalRun::Worker,
        scripted(400),
        scripted(600),
        scripted(0),
        camera(Vec::new(), 0),
        camera(camera_picker::proposed(), 600),
        camera(camera_picker::proposed(), 400),
        camera(camera_picker::proposed(), 0),
    ])))
}

fn framing(d: &Driver<'_>) -> Option<Framing> {
    let node = d.app().selected_beat.clone()?;
    d.app().workspace.as_ref()?.document.nodes()[&node]
        .framing
        .clone()
}

fn target_count(d: &Driver<'_>) -> usize {
    d.app()
        .workspace
        .as_ref()
        .map_or(0, |workspace| workspace.document.targets().len())
}

/// The command's detection has concluded and been consumed.
fn concluded(app: &DeadpanApp) -> bool {
    app.targets.proposal_zoom.is_none()
        && app
            .targets
            .proposals()
            .is_some_and(|job| job.outcome.is_some())
        && !app.service.is_busy()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shot analysis stored for the Original", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;

    // The real Vision worker finds no face in the fixture: a clear refusal.
    let start = d.revision();
    d.command("zoom 1.35 target=face:2 curve=step")?;
    d.wait_for("Real face detection concluded", concluded)?;
    d.settled()?;
    let faces = d.app().targets.proposals().cloned();
    d.check(
        "The real detector finds no face in the fixture and :zoom target=face:2 refuses without an edit",
        d.revision() == start
            && target_count(d) == 0
            && framing(d).is_none()
            && matches!(
                faces.as_ref().and_then(|job| job.outcome.as_ref()),
                Some(crate::project::targets::ProposalOutcome::Found(found)) if found.faces.as_ref().is_some_and(Vec::is_empty) && found.salient_regions.is_none()
            )
            && d.app().error.as_deref().is_some_and(|error| {
                error.starts_with("No faces were found in this picture.")
                    && error.contains("No edit was made")
            }),
        json!({"revision":"unchanged","targets":0,"faces":[],"error":"No faces were found in this picture. No edit was made."}),
        json!({"revision_changed":d.revision() != start,"targets":target_count(d),"faces":format!("{:?}", faces.map(|job| job.outcome)),"error":d.app().error}),
    )?;
    d.capture("No face in the picture")?;

    // Two proposals arrive in the background; the editor keeps running.
    d.command("zoom 1.35 target=face:2 curve=step")?;
    d.wait_for("Detection running", |app| {
        app.targets.proposal_zoom.is_some()
    })?;
    let waiting = d.app().message.clone();
    d.check(
        "Detection runs in the background and says nothing changes until the face is found",
        d.revision() == start
            && waiting
                .as_deref()
                .is_some_and(|message| message == "Finding Face 2 in the displayed picture… Nothing changes unless that proposal is found.")
            && target_count(d) == 0 && framing(d).is_none(),
        json!({"revision":"unchanged","targets":0,"framing":null,"message":"Finding Face 2 in the displayed picture… Nothing changes unless that proposal is found."}),
        json!({"revision_changed":d.revision() != start,"message":waiting}),
    )?;
    d.capture("Finding faces")?;
    d.changed(&start)?;
    let saved = framing(d);
    let id = TargetId::new("face-1").unwrap();
    let target = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.document.targets().get(&id).cloned());
    let expected = deadpan_analysis::target_region(&proposed()[1].region);
    d.check(
        "face:2 saves the second face from the left as a target and follows it at 1.35x",
        matches!(&saved, Some(Framing { value: FramingValue::Follow { target, scale, .. }, .. })
            if *target == id && *scale == ExactRatio::new(27, 20).unwrap())
            && target.as_ref().is_some_and(|target| {
                target.label == "Face 2"
                    && target.region == expected.into()
                    && target.samples.is_empty()
                    && target.provenance.is_none()
            })
            && d.app().message.as_deref().is_some_and(|message| {
                message.starts_with("Framing saved: 1.350× following Face 2")
                    && message.contains("one Undo removes both")
            }),
        json!({"framing":"Follow face-1 at 1.35","target":{"label":"Face 2","region":format!("{expected:?}")},"message":"Framing saved: 1.350× following Face 2. …one Undo removes both."}),
        json!({"framing":format!("{saved:?}"),"target":format!("{target:?}"),"message":d.app().message}),
    )?;
    let widgets = d.widgets().to_string();
    d.check(
        "The inspector lists the face target and names the follow",
        widgets.contains("Face 2 · 1.35×") && widgets.contains("Region · not tracked"),
        json!(["Face 2 · 1.35×", "Region · not tracked"]),
        json!({"follow":widgets.contains("Face 2 · 1.35×"),"summary":widgets.contains("Region · not tracked")}),
    )?;
    d.capture("Following Face 2")?;
    let framed = d.revision();
    d.key(Key::U)?;
    d.changed(&framed)?;
    d.check(
        "One Undo removes both the face target and the framing",
        d.revision() != framed && target_count(d) == 0 && framing(d).is_none(),
        json!({"targets":0,"framing":null}),
        json!({"targets":target_count(d),"framing":format!("{:?}", framing(d))}),
    )?;

    // Navigation actively cancels this command's exact proposal job. It does
    // not wait for a stale completion to produce an error.
    let before = d.revision();
    let document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No face cancellation workspace")?
        .document
        .clone();
    let cursor = d.app().sequence_cursor;
    let source_cursor = d.app().source_cursor;
    let scope = d.app().sequence_scope.clone();
    let selected = d.app().selected_beat.clone();
    d.command("zoom 1.35 target=face:1")?;
    d.wait_for(
        "Face command has an active detection before navigation",
        |app| {
            app.targets.proposal_zoom.is_some()
                && app
                    .targets
                    .proposals()
                    .is_some_and(|job| job.outcome.is_none())
        },
    )?;
    let pending = d
        .app()
        .targets
        .proposals()
        .ok_or("No active face proposal job")?
        .clone();
    d.key(Key::L)?;
    d.wait_for("Actively cancelled face detection concludes", concluded)?;
    d.settled()?;
    let cancelled = d.app().targets.proposals().cloned();
    let unchanged = d
        .app()
        .workspace
        .as_ref()
        .is_some_and(|workspace| workspace.document == document);
    d.check(
        "Navigation actively cancels the exact face request without an error or authored edit",
        d.revision() == before && unchanged
            && target_count(d) == 0 && framing(d).is_none()
            && d.app().sequence_cursor == cursor + 1 && d.app().source_cursor == source_cursor
            && d.app().sequence_scope == scope && d.app().selected_beat == selected
            && d.app().targets.proposal_zoom.is_none() && d.app().camera.is_none()
            && d.app().camera_pending.is_none() && d.app().error.is_none()
            && cancelled.as_ref().is_some_and(|job|
                job.ticket == pending.ticket && job.session == pending.session
                    && job.revision == pending.revision && job.asset == pending.asset
                    && job.pts == pending.pts && job.selection == ProposalSelection::Faces
                    && matches!(job.outcome, Some(crate::project::targets::ProposalOutcome::Cancelled))),
        json!({"document":"unchanged","framing":null,"targets":0,"Edit":cursor+1,"Original":source_cursor,"outcome":"Cancelled","pending":false,"error":null}),
        json!({"revision_changed":d.revision() != before,"document_unchanged":unchanged,"targets":target_count(d),"framing":framing(d),"cursor":d.app().sequence_cursor,"source_cursor":d.app().source_cursor,"job":format!("{cancelled:?}"),"error":d.app().error}),
    )?;
    d.key(Key::H)?;
    d.settled()?;

    // face:N beyond the proposals refuses and says how many there are.
    d.command("zoom 1.35 target=face:3")?;
    d.wait_for("Out-of-range detection concluded", concluded)?;
    d.settled()?;
    d.check(
        "face:3 of two faces refuses with the count and makes no edit",
        d.revision() == before
            && target_count(d) == 0
            && d.app().error.as_deref()
                == Some("face:3 is out of range: this picture has 2 faces (face:1–face:2), numbered left to right. No edit was made."),
        json!({"error":"face:3 is out of range: this picture has 2 faces (face:1–face:2), numbered left to right. No edit was made."}),
        json!({"revision_changed":d.revision() != before,"targets":target_count(d),"error":d.app().error}),
    )?;
    d.capture("face:3 out of range")?;

    // A malformed face number never starts detection.
    d.command("zoom 1.35 target=face:0")?;
    d.settled()?;
    d.check(
        "target=face:0 is refused by the command line",
        d.revision() == before
            && d.app().targets.proposal_zoom.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("face:N numbers the faces")),
        json!({"error":"face:N numbers the faces in the picture from 1 to 64, left to right."}),
        json!({"error":d.app().error}),
    )?;
    camera_picker::run(d)
}
