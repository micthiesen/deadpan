//! Camera proposals use the real service and host admission; only the positive
//! detector observations are scripted. Selection and framing are separate edits.
use super::*;

pub(super) fn proposed() -> Vec<DetectedFace> {
    (0..12)
        .map(|index| DetectedFace {
            region: NormalizedRect::new(
                0.04 + f64::from(index / 2) * 0.16,
                0.12 + f64::from(index % 2) * 0.46,
                0.10,
                0.18,
            )
            .unwrap(),
            confidence: 0.85,
        })
        .collect()
}

fn state(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn open(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens for face picking", |app| app.camera.is_some())?;
    d.settled()
}

fn face_job_done(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Camera detection finishes", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_some())
            && app
                .camera
                .as_ref()
                .is_some_and(|camera| camera.harness_state()["faces_pending"] == false)
    })?;
    d.settled()
}

fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Camera continues after the accepted face save", |app| {
        !app.service.is_busy()
            && app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["saving"] == false && state["rebasing"] == false && state["phase"] == "Adjust"
            })
    })?;
    d.settled()
}

fn selected_face(d: &Driver<'_>, number: u64) -> Result<DetectedFace, String> {
    match d
        .app()
        .targets
        .proposals()
        .and_then(|job| job.outcome.as_ref())
    {
        Some(crate::project::targets::ProposalOutcome::Found(found)) => found
            .faces
            .as_ref()
            .and_then(|faces| faces.get(number as usize - 1))
            .cloned()
            .ok_or_else(|| format!("Missing admitted face {number}")),
        _ => Err("Missing admitted face proposals".into()),
    }
}

fn select_face(d: &mut Driver<'_>, face: u64) -> Result<(), String> {
    let initial_page = state(d)["picker_page"].as_u64().unwrap_or(0);
    loop {
        let snapshot = state(d);
        if let Some(number) = snapshot["picker"].as_array().and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["face"] == face)
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
                _ => return Err(format!("Invalid picker digit {number}")),
            };
            return d.key(key);
        }
        d.key(Key::Tab)?;
        d.settled()?;
        if state(d)["picker_page"] == initial_page {
            return Err(format!("Face {face} is unreachable by keyboard"));
        }
    }
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

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let start = d.revision();
    open(d)?;
    d.key(Key::F)?;
    face_job_done(d)?;
    let empty = state(d);
    d.check(
        "Camera reports the explicitly scripted empty Both result and keeps ordinary targets usable",
        d.revision() == start
            && target_count(d) == 0
            && empty["phase"] == "TargetPicker"
            && empty["proposals_status"] == "Scripted: No faces or salient regions found in this picture. Saved targets and fixed points remain available."
            && empty["proposal_origin"]["type"] == "scripted"
            && empty["picker"]
                .as_array()
                .is_some_and(|entries| entries.len() == 5)
            && matches!(d.app().targets.proposals().and_then(|job| job.outcome.as_ref()),
                Some(crate::project::targets::ProposalOutcome::Found(found)) if found.faces.as_ref().is_some_and(Vec::is_empty) && found.salient_regions.as_ref().is_some_and(Vec::is_empty)),
        json!({"revision":"unchanged","targets":0,"fixed_choices":5,"faces":[]}),
        empty,
    )?;
    d.capture("Camera without detected faces")?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.settled()?;

    open(d)?;
    d.key(Key::F)?;
    d.wait_for("Camera face detection is pending", |app| {
        app.camera
            .as_ref()
            .is_some_and(|camera| camera.harness_state()["faces_pending"] == true)
            && app
                .targets
                .proposals()
                .is_some_and(|job| job.outcome.is_none())
    })?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.wait_for("Detection finishes after Camera was cancelled", |app| {
        app.targets
            .proposals()
            .is_some_and(|job| job.outcome.is_some())
    })?;
    d.settled()?;
    d.check(
        "A late detection cannot reopen cancelled Camera or save a target",
        d.app().camera.is_none() && d.revision() == start && target_count(d) == 0,
        json!({"camera":null,"revision":"unchanged","targets":0}),
        json!({"camera":state(d),"revision":d.revision(),"targets":target_count(d)}),
    )?;

    open(d)?;
    d.key(Key::Plus)?;
    d.key(Key::F)?;
    let before = state(d);
    let before_fixed: Vec<_> = before["picker"]
        .as_array()
        .ok_or("Missing Camera picker")?
        .iter()
        .take(5)
        .map(|entry| (entry["number"].clone(), entry["label"].clone()))
        .collect();
    face_job_done(d)?;
    let proposals = state(d);
    let same_fixed = proposals["picker"].as_array().is_some_and(|entries| {
        entries
            .iter()
            .take(5)
            .map(|entry| (entry["number"].clone(), entry["label"].clone()))
            .eq(before_fixed.iter().cloned())
    });
    d.check(
        "Background proposals preserve the existing digits and create no edit",
        same_fixed
            && proposals["proposals_status"] == "Scripted: 12 face and 0 region proposals. Selecting one saves that target; Enter applies framing."
            && proposals["proposal_origin"]["type"] == "scripted"
            && proposals["picker"]
                .as_array()
                .is_some_and(|entries| entries.len() == 17)
            && proposals["picker_pages"] == 2
            && d.revision() == start
            && target_count(d) == 0,
        json!({"existing_digits":"unchanged","entries":17,"pages":2,"targets":0}),
        proposals,
    )?;
    d.key(Key::Tab)?;
    d.settled()?;
    let second = state(d);
    d.key_modified(Key::Tab, egui::Modifiers::SHIFT)?;
    d.settled()?;
    d.check(
        "Tab and Shift Tab page numbered proposals without changing the draft",
        second["picker_page"] == 2
            && state(d)["picker_page"] == 1
            && second["pose"] == before["pose"]
            && d.revision() == start,
        json!({"forward_page":2,"reverse_page":1,"draft":"unchanged"}),
        json!({"forward":second,"reverse":state(d)}),
    )?;
    d.key(Key::Tab)?;
    d.settled()?;
    d.capture("Detected Camera faces on page two")?;
    let face = selected_face(d, 6)?;
    select_face(d, 6)?;
    d.changed(&start)?;
    ready(d)?;
    let accepted = d.revision();
    let saved = target(d, "face-1");
    let expected = deadpan_analysis::target_region(&face.region);
    d.check(
        "Selecting a proposal saves only that face and continues the adjusted Camera draft",
        target_count(d) == 1
            && saved.as_ref().is_some_and(|target| {
                target.region == expected.into()
                    && target.label == "Face 6"
                    && target.samples.is_empty()
                    && target.display_aspect.is_some()
            })
            && framing(d).is_none()
            && state(d)["selected"] == "face-1"
            && state(d)["draft_pose"]["scale"] == before["draft_pose"]["scale"]
            && state(d)["pose"] != before["pose"],
        json!({"targets":1,"saved":"Face 6","framing":null,"selected":"face-1"}),
        json!({"target":saved,"framing":framing(d),"camera":state(d)}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape restores entry framing while retaining the explicitly accepted target",
        d.app().camera.is_none()
            && framing(d).is_none()
            && d.revision() == accepted
            && target_count(d) == 1,
        json!({"camera":null,"framing":null,"targets":1}),
        json!({"camera":state(d),"framing":framing(d),"targets":target_count(d)}),
    )?;
    d.key(Key::U)?;
    d.changed(&accepted)?;
    d.check(
        "One Undo removes the accepted proposal without a hidden framing edit",
        target_count(d) == 0 && framing(d).is_none(),
        json!({"targets":0,"framing":null}),
        json!({"targets":target_count(d),"framing":framing(d)}),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.settled()?;
    open(d)?;
    d.key(Key::F)?;
    let prior_saved_choice = state(d)["picker"][0].clone();
    face_job_done(d)?;
    let saved_choice = state(d)["picker"][0].clone();
    d.check(
        "Existing saved target identity, label and digit survive the arrival of proposals",
        saved_choice == prior_saved_choice
            && saved_choice["saved"] == "face-1"
            && saved_choice["label"] == "Face 6"
            && saved_choice["number"] == 1
            && saved_choice["face"].is_null(),
        json!({"saved":"face-1","label":"Face 6","number":1,"face":null}),
        json!({"before":prior_saved_choice,"after":saved_choice}),
    )?;
    let saved_rectangles = |d: &Driver<'_>| {
        d.harness
            .output()
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.fill == egui::Color32::TRANSPARENT && rect.stroke.color == style::CURSOR)
            })
            .count()
    };
    let before_page = d.revision();
    let first_page_rectangles = saved_rectangles(d);
    d.key(Key::Tab)?;
    d.settled()?;
    let second_page_rectangles = saved_rectangles(d);
    d.check(
        "The picker hides off-page saved markers rather than leaving unidentified outlines",
        first_page_rectangles > 0
            && second_page_rectangles + 1 == first_page_rectangles
            && state(d)["picker_page"] == 2
            && state(d)["picker"][0]["number"].is_null()
            && d.revision() == before_page,
        json!({"off_page_saved_rectangle":"not painted","revision":"unchanged"}),
        json!({"page_one_target_strokes":first_page_rectangles,"page_two_target_strokes":second_page_rectangles,"camera":state(d)}),
    )?;
    d.capture("Paged face choices hide off-page saved markers")?;
    d.key_modified(Key::Tab, egui::Modifiers::SHIFT)?;
    d.settled()?;
    d.key(Key::Num1)?;
    d.key(Key::T)?;
    let target_only = d.revision();
    d.key(Key::Enter)?;
    d.changed(&target_only)?;
    d.settled()?;
    let followed = framing(d);
    d.check(
        "Enter separately commits framing toward the accepted face",
        matches!(&followed, Some(Framing { value: FramingValue::Follow { target, .. }, .. })
            if target.as_str() == "face-1")
            && target_count(d) == 1,
        json!({"follow":"face-1","targets":1}),
        json!({"framing":followed,"targets":target_count(d)}),
    )?;
    let committed = d.revision();
    d.key(Key::U)?;
    d.changed(&committed)?;
    d.settled()?;
    d.check(
        "One framing Undo retains the separately accepted face target",
        framing(d).is_none() && target_count(d) == 1,
        json!({"framing":null,"targets":1}),
        json!({"framing":framing(d),"targets":target_count(d)}),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.settled()?;
    let before_creep = d.revision();
    d.command("creep to=1.35")?;
    d.changed(&before_creep)?;
    d.settled()?;
    let animated = framing(d).ok_or("Missing following creep")?;
    let base = d.revision();
    open(d)?;
    d.key(Key::F)?;
    face_job_done(d)?;
    select_face(d, 6)?;
    d.changed(&base)?;
    ready(d)?;
    let target_saved = d.revision();
    d.check(
        "Accepting another detected subject keeps the committed animation untouched until Enter",
        framing(d).as_ref() == Some(&animated)
            && target_count(d) == 2
            && state(d)["follow"] == "face-2",
        json!({"committed_framing":"unchanged","targets":2,"draft_follow":"face-2"}),
        json!({"framing":framing(d),"camera":state(d),"targets":target_count(d)}),
    )?;
    d.key(Key::Enter)?;
    d.changed(&target_saved)?;
    d.settled()?;
    let switched = framing(d);
    let preserves = match (&animated, &switched) {
        (
            Framing {
                clock: old_clock,
                value:
                    FramingValue::Follow {
                        scale: old_scale,
                        fallback: old_fallback,
                        scale_transition: old_transition,
                        ..
                    },
            },
            Some(Framing {
                clock,
                value:
                    FramingValue::Follow {
                        target,
                        scale,
                        fallback,
                        scale_transition,
                        ..
                    },
            }),
        ) => {
            target.as_str() == "face-2"
                && old_clock == clock
                && old_scale == scale
                && old_fallback == fallback
                && old_transition == scale_transition
                && scale_transition.is_some()
        }
        _ => false,
    };
    d.check(
        "Choosing a detected subject preserves the full Follow clock, bases and scale transition",
        preserves && target_count(d) == 2,
        json!({"follow":"face-2","clock_bases_transition":"retained","targets":2}),
        json!({"before":animated,"after":switched,"targets":target_count(d)}),
    )?;
    d.capture("Camera proposal accepted into following creep")?;
    Ok(())
}
