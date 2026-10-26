//! Explicit Camera proposal acceptance composes with named ranged sampling.
//! Only detector observations are scripted; admission, saving, Camera's scoped
//! continuation, Escape, range construction and framing use production routes.
use super::*;
use crate::project::targets::{Backend, ProposalRun, ProposalScript};
use deadpan_analysis::NormalizedRect;
use deadpan_cli::{faces::DetectedFace, picture_proposals::ProposalSelection};
use deadpan_core::TargetRegion;
use std::{sync::Arc, time::Duration};

pub(super) fn backend() -> Backend {
    let batch = |faces| ProposalRun::Proposals {
        selection: ProposalSelection::Both,
        faces: Some(faces),
        salient_regions: Some(Vec::new()),
        delay: Duration::ZERO,
    };
    Backend::ScriptedProposals(Arc::new(ProposalScript::new([
        // The parent fixture opens f once to correct its existing point.
        // Keep that picker identical; only this module needs a positive face.
        batch(Vec::new()),
        batch(vec![DetectedFace {
            region: NormalizedRect::new(0.6, 0.4, 0.2, 0.2).unwrap(),
            confidence: 0.9,
        }]),
    ])))
}

fn accept_face(d: &mut Driver<'_>) -> Result<(), String> {
    let number = camera(d)["picker"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["face"] == 1))
        .and_then(|entry| entry["number"].as_u64())
        .ok_or("Scoped face proposal has no visible picker digit")?;
    d.key(match number {
        1 => Key::Num1,
        2 => Key::Num2,
        3 => Key::Num3,
        4 => Key::Num4,
        5 => Key::Num5,
        6 => Key::Num6,
        7 => Key::Num7,
        8 => Key::Num8,
        9 => Key::Num9,
        _ => return Err("Invalid scoped proposal picker digit".into()),
    })
}

pub(super) fn run(d: &mut Driver<'_>, baseline: &crate::project::Workspace) -> Result<(), String> {
    ranges::select(d, 2, 2, 6, 24)?;
    d.key(Key::Escape)?;
    local(d, 23)?;
    let scope = d
        .app()
        .scoped_target()?
        .ok_or("No proposal concrete Play")?;
    let before = workspace(d)?;
    open_camera(d)?;
    // The earlier picker may cancel before its empty scripted batch is
    // consumed. One explicit close/reopen then reaches the repeating positive
    // batch without making timing assumptions about that cancellation.
    for attempt in 0..2 {
        d.key(Key::F)?;
        d.wait_for("Scoped Camera admits scripted face observations", |app| {
            app.targets
                .proposals()
                .is_some_and(|job| job.outcome.is_some())
                && app
                    .camera
                    .as_ref()
                    .is_some_and(|camera| camera.harness_state()["faces_pending"] == false)
        })?;
        d.settled()?;
        if camera(d)["picker"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| entry["face"] == 1))
        {
            break;
        }
        if attempt == 0 {
            d.key(Key::F)?;
        }
    }
    d.check(
        "Scoped Camera detection alone retains both concrete Plays and makes no authored edit",
        workspace(d)?.document == before.document
            && camera(d)["proposal_origin"]["type"] == "scripted"
            && camera(d)["phase"] == "TargetPicker"
            && camera(d)["picker"].as_array().is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| entry["face"] == 1 && entry["available"] == true)
            })
            && d.app().scoped_target()?.as_ref() == Some(&scope)
            && d.app().sequence_cursor == 187,
        json!({"origin":"scripted observations","Edit":187,"scope":scope.target,"presentation":scope.presentation,"write":false}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    accept_face(d)?;
    d.changed(&revision)?;
    continued(d)?;
    let accepted = workspace(d)?;
    let (id, target) = accepted
        .document
        .targets()
        .iter()
        .find(|(id, _)| !before.document.targets().contains_key(*id))
        .ok_or("Explicit scoped face selection did not save a new target")?;
    let id = id.clone();
    let target = target.clone();
    let index = accepted.sources[&target.asset]
        .video_index
        .as_ref()
        .ok_or("No detected target index")?;
    let expected_region: deadpan_core::TargetGeometry = TargetRegion {
        center: [700_000, 500_000],
        size: [200_000, 200_000],
    }
    .into();
    let mut target_only = json!(accepted.document);
    target_only["revision_id"] = json!(before.document.revision_id());
    target_only["targets"] = json!(before.document.targets());
    let mut continued_scope = scope.clone();
    continued_scope.revision = accepted.document.revision_id().clone();
    d.check("Explicit scoped face acceptance saves only the 70-percent target and continues Camera on the same concrete Play",
        target_only == json!(before.document)
            && accepted.document.targets().len() == before.document.targets().len()+1
            && target.region == expected_region
            && target.label == "Face 1"
            && target.span.start().ticks == index.frames()[15].pts
            && target.samples.is_empty() && target.corrections.is_empty()
            && camera(d)["selected"] == id.as_str()
            && camera(d)["phase"] == "Adjust"
            && d.app().scoped_target()?.as_ref() == Some(&continued_scope),
        json!({"new_target":id,"region":expected_region,"span_starts_Original_ordinal":15,
            "document_change":"one saved target only","scope":scope.target,"presentation":scope.presentation}),json!({"target":target,"camera":camera(d),"scope":d.app().scoped_target()?.map(|target|target.target)}))?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check("Camera Escape keeps the explicitly accepted face while leaving every authored framing and isolation edge unchanged",
        d.app().camera.is_none() && workspace(d)?.document == accepted.document
            && d.app().scoped_target()?.as_ref() == Some(&continued_scope),
        json!({"target":id,"framing":"unchanged","overrides":"unchanged"}),d.snapshot())?;

    // Backward selection leaves the stopped head before this target exists.
    // Named sampling must use Out-1=187, not head=170 or the picker at head.
    let captured = ranges::select(d, 2, 2, 24, 6)?;
    let head = accepted
        .plan
        .picture(ProjectFrame(170))
        .map_err(|e| e.to_string())?;
    let head_before_span = head.picture.follow_point().is_some_and(|(asset, point)| {
        asset == &target.asset
            && point
                .ticks
                .compare(ExactRatio::integer(target.span.start().ticks))
                .is_lt()
    });
    d.check("The named detected target is ineligible at the stopped Time head but available at the requested Out-1 sample",
        head_before_span && d.app().sequence_cursor==170
            && target.span.start().ticks==index.frames()[15].pts,
        json!({"head":170,"sample":187,"target_start_Original_ordinal":15,"head_Original_position":"13/3"}),
        json!({"head":head.picture.follow_point(),"target_span":target.span,"Edit":d.app().sequence_cursor}))?;
    edit(
        d,
        &format!("creep from=1.5 to=2.2 curve=linear target={id}"),
    )?;
    effect(
        d,
        &accepted,
        &captured,
        &Case {
            root: 164,
            start: 6,
            end: 24,
            creep: true,
            point: 70,
        },
    )?;
    let authored = workspace(d)?;
    history(d, &accepted, &authored)?;
    undo(d, &accepted)?;
    d.check("One durable framing Undo removes both isolated branches while retaining the separately accepted detected target",
        workspace(d)?.document.targets().get(&id)==Some(&target)
            && equivalent(&workspace(d)?.document,&accepted.document),
        json!({"target":id,"framing":"restored","isolated_branches":0}),d.snapshot())?;
    undo(d, baseline)?;
    d.check("A second durable Undo removes the explicit target save and restores the entire sampled fixture",
        equivalent(&workspace(d)?.document,&baseline.document)
            && !workspace(d)?.document.targets().contains_key(&id),
        json!({"target_save":"undone","fixture":"restored"}),d.snapshot())
}
