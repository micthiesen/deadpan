//! Interactive target samples use root Time endpoints, even through nested Plays.
//! The oracle uses fixture arithmetic and integer Q32 interpolation, never the
//! production owner map, target projection or framing builder.
use super::*;
use deadpan_core::{Command, CommandRequest, FramingPose, RevisionId, TargetCorrection};
use deadpan_store::{AccessMode, ProjectStore};

mod detected;

pub(super) fn backend() -> crate::project::targets::Backend {
    detected::backend()
}

const CREEP: &str = "creep from=1.5 to=2.2 curve=linear target=current";

fn workspace(d: &Driver<'_>) -> Result<std::sync::Arc<crate::project::Workspace>, String> {
    d.app()
        .workspace
        .clone()
        .ok_or("No sampled range workspace".into())
}

fn close(d: &mut Driver<'_>) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close sampled fixture writer", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })
}

fn open(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Open(path.into()))?;
    d.wait_for("Reopen sampled fixture through native service", |app| {
        !app.service.is_busy() && app.workspace.as_ref().is_some_and(|w| w.path == path)
    })?;
    idle(d)?;
    super::super::transcript::focus_your_edit(d)?;
    at(d, 0)
}

fn apply(store: &mut ProjectStore, revision: &str, command: Command) -> Result<(), String> {
    let document = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).map_err(|e| e.to_string())?,
            command,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn fixture(
    d: &mut Driver<'_>,
    owner: &NodeId,
    name: &str,
) -> Result<std::sync::Arc<crate::project::Workspace>, String> {
    let before = workspace(d)?;
    let id = TargetId::new("target-1").unwrap();
    let mut target = before.document.targets()[&id].clone();
    let index = before.sources[&target.asset]
        .video_index
        .as_ref()
        .ok_or("No fixture index")?;
    // j=6 -> Original4 -> 30%; j=23 -> Original15 -> 70%;
    // stopped j=24 -> Original16 -> 40%. All three are distinct.
    target.region = TargetPoint {
        center: [300_000, 500_000],
    }
    .into();
    target.corrections = [(6, 550_000), (12, 700_000), (16, 400_000)]
        .into_iter()
        .map(|(ordinal, x)| TargetCorrection {
            at: index.frames()[ordinal].pts,
            region: TargetPoint {
                center: [x, 500_000],
            }
            .into(),
        })
        .collect();
    target.label = "Sampled moving point".into();
    target.samples.clear();
    target.provenance = None;
    leave(d)?;
    close(d)?;
    let mut store =
        ProjectStore::open(&before.path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    apply(
        &mut store,
        &format!("sampled-{name}-static"),
        Command::SetFraming {
            node: owner.clone(),
            framing: Some(
                Framing::static_pose(FramingPose::identity()).map_err(|e| e.to_string())?,
            ),
        },
    )?;
    apply(
        &mut store,
        &format!("sampled-{name}-target"),
        Command::SetTarget { id, target },
    )?;
    drop(store);
    open(d, &before.path)?;
    workspace(d)
}

fn undo(d: &mut Driver<'_>, baseline: &crate::project::Workspace) -> Result<(), String> {
    edit(d, "undo")?;
    d.check(
        "One Undo removes sampled framing and both isolation edges",
        equivalent(&workspace(d)?.document, &baseline.document) && d.app().scoped.is_none(),
        json!({"document":baseline.document,"overrides":baseline.document.overrides()}),
        d.snapshot(),
    )
}

fn round(n: i128, denominator: i128) -> i128 {
    let magnitude = n.abs();
    let a = magnitude / denominator;
    let r = magnitude % denominator;
    n.signum() * (a + i128::from(2 * r > denominator || (2 * r == denominator && a % 2 != 0)))
}

fn q(n: i128, denominator: i128) -> i128 {
    round(
        n * i128::from(deadpan_core::FRAMING_NUMERIC_SCALE),
        denominator,
    )
}

fn expected(j: i64, start: i64, end: i64, creep: bool, point: i128) -> FramingPose {
    if j < start || (!creep && j >= end) {
        return FramingPose::identity();
    }
    let scale = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    if !creep {
        return FramingPose {
            center_x: ratio(q(point, 100), scale),
            center_y: ratio(1, 2),
            scale: ExactRatio::integer(2),
        };
    }
    if j >= end {
        return FramingPose {
            center_x: ratio(q(point, 100), scale),
            center_y: ratio(1, 2),
            scale: ratio(11, 5),
        };
    }
    let interpolate = |from: i128, to: i128| {
        let progress = q(
            i128::from(2 * (j - start) + 1),
            i128::from(2 * (end - start)),
        );
        let result = from + round((to - from) * progress, scale);
        ratio(result, scale)
    };
    FramingPose {
        center_x: interpolate(q(1, 2), q(point, 100)),
        center_y: ratio(1, 2),
        scale: interpolate(q(3, 2), q(11, 5)),
    }
}

struct Case {
    root: i64,
    start: i64,
    end: i64,
    creep: bool,
    point: i128,
}

fn effect(
    d: &mut Driver<'_>,
    baseline: &crate::project::Workspace,
    captured: &crate::project::scoped::Target,
    case: &Case,
) -> Result<(), String> {
    let after = workspace(d)?;
    let selected = d.app().scoped_target()?.ok_or("No sampled result owner")?;
    let aliases = aliases(baseline, &after)?;
    let authored = framing(d, &selected.target.node);
    let fixed_recipe = authored.as_ref().is_some_and(|f| {
        f.clock == FramingClock::OwnerOutput && !matches!(f.value, FramingValue::Follow { .. })
    });
    let mut exact = true;
    let mut pictures = 0;
    let mut mismatches = Vec::new();
    for frame in 0..240 {
        let mut old = baseline
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        let new = after
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        for layer in &mut old.framing {
            if !layer.escalation && captured.presentation.as_ref() == Some(&layer.instance) {
                let j = frame - case.root;
                exact &= (0..36).contains(&j)
                    && layer.local_position == ratio(i128::from(2 * j + 1), 2)
                    && layer.effective_duration == ExactRatio::integer(36);
                layer.pose = Some(expected(j, case.start, case.end, case.creep, case.point));
                pictures += 1;
            }
        }
        let normalized_new = normalized(view(&new), &aliases);
        let expected_view = view(&old);
        let frame_exact = normalized_new == expected_view;
        if !frame_exact && mismatches.len() < 3 {
            mismatches.push(json!({"Edit":frame,"local":frame-case.root,
                "actual_selected_pose":new.framing.iter().find(|layer|
                    !layer.escalation&&selected.presentation.as_ref()==Some(&layer.instance)).and_then(|layer|layer.pose),
                "expected_selected_pose":old.framing.iter().find(|layer|
                    !layer.escalation&&captured.presentation.as_ref()==Some(&layer.instance)).and_then(|layer|layer.pose),
                "normalized_actual":normalized_new,"expected_view":expected_view}));
        }
        exact &= frame_exact;
    }
    let end = baseline.plan.audio_duration().map_err(|e| e.to_string())?;
    let old_audio = baseline
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .map_err(|e| e.to_string())?;
    let new_audio = after
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .map_err(|e| e.to_string())?;
    let audio = normalized(json!(new_audio.spans), &aliases) == json!(old_audio.spans);
    let originals = baseline
        .document
        .nodes()
        .iter()
        .all(|(id, node)| after.document.nodes().get(id) == Some(node));
    let clones = after
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| !baseline.document.nodes().contains_key(*id))
        .all(|(id, node)| {
            let Some(original) = aliases.get(id) else {
                return false;
            };
            let mut expected = baseline.document.nodes()[original].clone();
            if id == &selected.target.node {
                expected.framing = authored.clone();
            }
            normalized(json!(node), &aliases) == json!(expected)
        });
    let address = captured.root == selected.root
        && captured.target.repeats.len() == 2
        && selected.target.repeats.len() == 2
        && captured
            .target
            .repeats
            .iter()
            .zip(&selected.target.repeats)
            .all(|(a, b)| {
                a.branch == b.branch && aliases.get(&b.repeat).unwrap_or(&b.repeat) == &a.repeat
            });
    let isolation = after.document.overrides().len() == 2
        && selected.target.repeats.iter().enumerate().all(|(i, step)| {
            let deadpan_core::RepeatEditBranch::Play { iteration } = &step.branch else {
                return false;
            };
            let next = selected
                .target
                .repeats
                .get(i + 1)
                .map_or(&selected.target.node, |s| &s.repeat);
            after
                .document
                .overrides()
                .get(&step.repeat)
                .is_some_and(|entries| entries.len() == 1 && entries.get(iteration) == Some(next))
        });
    d.check("Sampled Time framing changes exactly one explicit nested Play; independent root-clock poses, every provider, gap, caption and complete audio remain exact",
        fixed_recipe&&exact&&pictures==36&&audio&&originals&&clones&&address&&isolation
            &&after.document.targets()==baseline.document.targets()
            &&after.document.gap_overrides()==baseline.document.gap_overrides()
            &&after.plan.duration()==baseline.plan.duration()
            &&after.plan.audio_duration().is_ok_and(|value|value==end),
        json!({"root_owner":[case.root,case.root+36],"root_range":[case.root+case.start,case.root+case.end],
            "sampled_center_percent":case.point,"creep":case.creep,"pictures":240,"selected_pictures":36,"audio_samples":end}),
        json!({"framing":authored,"fixed_recipe":fixed_recipe,"frames_exact":exact,"audio":audio,
            "originals":originals,"clones":clones,"address":address,"isolation":isolation,"aliases":aliases,"pictures":pictures,"first_mismatches":mismatches}))
}

fn refuse(d: &mut Driver<'_>, command: &str, needle: &str) -> Result<(), String> {
    let before = workspace(d)?;
    let selection = d.app().edit_range.semantic()?;
    let cursor = d.app().sequence_cursor;
    d.command(command)?;
    idle(d)?;
    d.check(
        "Sampled ranged refusal preserves the complete document, selection and stopped cursor",
        workspace(d)?.document == before.document
            && d.app().edit_range.semantic()? == selection
            && d.app().sequence_cursor == cursor
            && d.app().error.as_deref().is_some_and(|e| e.contains(needle)),
        json!({"command":command,"error_contains":needle,"write":false}),
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
        .ok_or("No sampled root viewport")?
        .inner_rect = Some(rect);
    d.step("Sampled Time framing at minimum workspace size", true)?;
    d.settled()
}

fn history(
    d: &mut Driver<'_>,
    baseline: &crate::project::Workspace,
    authored: &crate::project::Workspace,
) -> Result<(), String> {
    let path = authored.path.clone();
    leave(d)?;
    close(d)?;
    open(d, &path)?;
    d.check(
        "Reopen retains the exact sampled recipe and physical cloned identities",
        workspace(d)?.document == authored.document,
        json!(authored.document),
        d.snapshot(),
    )?;
    undo(d, baseline)?;
    edit(d, "redo")?;
    d.check(
        "Durable Redo restores the exact sampled recipe and both cloned branches",
        equivalent(&workspace(d)?.document, &authored.document),
        json!(authored.document),
        d.snapshot(),
    )
}

fn forward_owner_motion(
    d: &mut Driver<'_>,
    baseline: &crate::project::Workspace,
    root: u64,
) -> Result<(), String> {
    let selection = Some(deadpan_core::SemanticVisualSelection::Time {
        anchor: ProjectFrame(i64::try_from(root).map_err(|e| e.to_string())?),
        head: ProjectFrame(i64::try_from(root + 36).map_err(|e| e.to_string())?),
        extending: false,
    });
    let original = d.app().source_cursor;
    d.chord(&[Key::End, Key::End, Key::L])?;
    d.settled()?;
    let right = baseline
        .plan
        .picture(ProjectFrame(
            i64::try_from(root + 36).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    d.check("A forward full-owner Time range retains its exact end seam while End and l saturate there and display the following gap",
        d.app().sequence_cursor==root+36
            &&d.app().capture_visual_selection()?==selection
            &&matches!(right.picture,deadpan_plan::Picture::Freeze {..})
            &&d.app().presentation.picture().is_some_and(|shown|shown.framing==right.framing
                &&shown.follow_point.as_ref().map(|(asset,point)|(asset,*point))==right.picture.follow_point()),
        json!({"Edit":root+36,"selection":selection,"shown":"following gap"}),d.snapshot())?;
    d.key(Key::H)?;
    d.settled()?;
    d.check("One h from the selected end seam returns exactly one frame inside the owner without moving the finished range head",
        d.app().sequence_cursor==root+35&&d.app().capture_visual_selection()?==selection,
        json!({"Edit":root+35,"selection":selection}),d.snapshot())?;
    d.chord(&[Key::Home, Key::Home, Key::H])?;
    d.settled()?;
    d.check(
        "Home and h saturate at the full-owner range's start without changing its retained end",
        d.app().sequence_cursor == root && d.app().capture_visual_selection()? == selection,
        json!({"Edit":root,"selection":selection}),
        d.snapshot(),
    )?;
    d.key(Key::L)?;
    d.settled()?;
    d.check(
        "One l from Home advances one frame while the finished full-owner selection stays fixed",
        d.app().sequence_cursor == root + 1 && d.app().capture_visual_selection()? == selection,
        json!({"Edit":root+1,"selection":selection}),
        d.snapshot(),
    )?;
    d.key(Key::H)?;
    d.settled()?;
    d.check(
        "One h reverses that l exactly while retaining the forward full-owner range",
        d.app().sequence_cursor == root && d.app().capture_visual_selection()? == selection,
        json!({"Edit":root,"selection":selection}),
        d.snapshot(),
    )?;
    d.chord(&[Key::End, Key::H])?;
    d.settled()?;
    let inside = baseline
        .plan
        .picture(ProjectFrame(
            i64::try_from(root + 35).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    d.check("The forward full-owner range remains finished at Out while the exact stopped picture returns inside for sampled Zoom",
        d.app().sequence_cursor==root+35&&d.app().capture_visual_selection()?==selection
            &&d.app().source_cursor==original&&workspace(d)?.document==baseline.document
            &&d.app().presentation.picture().is_some_and(|shown|shown.framing==inside.framing
                &&shown.follow_point.as_ref().map(|(asset,point)|(asset,*point))==inside.picture.follow_point()),
        json!({"Edit":root+35,"Original":original,"selection":selection,"document":"unchanged"}),d.snapshot())
}

pub(super) fn run(d: &mut Driver<'_>, original: &crate::project::Workspace) -> Result<(), String> {
    undo(d, original)?;
    ranges::select(d, 2, 2, 6, 24)?;
    // Retain the outside-path refusal on the old live Follow fixture.
    refuse(d, CREEP, "follows a target")?;
    let owner = d
        .app()
        .scoped_target()?
        .ok_or("No sampled fixture owner")?
        .target
        .node;
    let baseline = fixture(d, &owner, "first")?;
    let source_cursor = d.app().source_cursor;
    for (outer, inner, start, end, command, creep, point) in [
        (2, 2, 6, 24, "zoom 2 target=current", false, 30),
        (
            2,
            2,
            24,
            6,
            "creep from=1.5 to=2.2 curve=linear target=target-1",
            true,
            70,
        ),
        (1, 3, 36, 0, "zoom 2 target=target-1", false, 30),
        (
            1,
            1,
            36,
            0,
            "creep from=1.5 to=2.2 curve=linear target=current",
            true,
            40,
        ),
        (1, 2, 0, 36, "zoom 2 target=current", false, 30),
    ] {
        let captured = ranges::select(d, outer, inner, start, end)?;
        let case = Case {
            root: i64::from((outer - 1) * 124 + (inner - 1) * 40),
            start: i64::from(start.min(end)),
            end: i64::from(start.max(end)),
            creep,
            point,
        };
        if start == 0 && end == 36 {
            let motion_baseline = workspace(d)?;
            forward_owner_motion(
                d,
                &motion_baseline,
                u64::try_from(case.root).map_err(|e| e.to_string())?,
            )?;
        }
        edit(d, command)?;
        effect(d, &baseline, &captured, &case)?;
        if !creep && start == 6 {
            let authored = workspace(d)?;
            history(d, &baseline, &authored)?;
        }
        undo(d, &baseline)?;
    }
    // A real writer history change while the command field owns input must
    // invalidate the captured revision, even when its text remains open.
    ranges::select(d, 2, 2, 6, 24)?;
    d.key(Key::Colon)?;
    d.events(
        "Stage sampled command before writer Undo",
        vec![egui::Event::Text(CREEP.into())],
    )?;
    let revision = d.revision();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: workspace(d)?.document.revision_id().clone(),
    })?;
    d.changed(&revision)?;
    idle(d)?;
    let stale = workspace(d)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "A sampled command captured before writer Undo refuses without another write",
        workspace(d)?.document == stale.document
            && d.app().error.as_deref().is_some_and(|e| {
                e.contains("changed") || e.contains("revision") || e.contains("stale")
            }),
        json!({"write":false,"captured_revision":baseline.document.revision_id()}),
        d.snapshot(),
    )?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the exact sampled fixture after delayed-command invalidation",
        equivalent(&workspace(d)?.document, &baseline.document),
        json!(baseline.document),
        d.snapshot(),
    )?;

    // Clip the sample by framing the inner Source. This is fixture setup only.
    let source = match &baseline.document.nodes()[&owner].kind {
        NodeKind::Retime { child, .. } => child.clone(),
        _ => return Err("No sampled Retime child".into()),
    };
    leave(d)?;
    close(d)?;
    let mut store =
        ProjectStore::open(&baseline.path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    apply(
        &mut store,
        "sampled-clipped-source",
        Command::SetFraming {
            node: source,
            framing: Some(
                Framing::static_pose(
                    FramingPose::new(ratio(4, 5), ratio(1, 2), ExactRatio::integer(2))
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?,
            ),
        },
    )?;
    drop(store);
    open(d, &baseline.path)?;
    ranges::select(d, 2, 2, 6, 24)?;
    refuse(d, "zoom 2 target=target-1", "not visible")?;
    undo(d, &baseline)?;
    detected::run(d, &baseline)?;
    edit(d, "undo")?;
    edit(d, "undo")?;
    d.check("Temporary static owner and moving target fixture undo back to the entire original nested baseline",
        equivalent(&workspace(d)?.document,&original.document),json!(original.document),d.snapshot())?;

    // Keep the final actual native-authored sample available for export and
    // independent Undo/Redo media comparison by the qualification driver.
    let baseline = fixture(d, &owner, "final")?;
    let captured = ranges::select(d, 2, 2, 6, 24)?;
    edit(d, CREEP)?;
    let case = Case {
        root: 164,
        start: 6,
        end: 24,
        creep: true,
        point: 70,
    };
    effect(d, &baseline, &captured, &case)?;
    let authored = workspace(d)?;
    history(d, &baseline, &authored)?;
    let authored = workspace(d)?;
    let selected = ranges::select(d, 2, 2, 6, 24)?;
    local(d, 18)?;
    resize(d, 960.0, 640.0)?;
    d.check("Final sampled native Creep fixture identity for emitted-media verification",
        equivalent(&workspace(d)?.document,&authored.document)&&d.app().source_cursor==source_cursor,
        json!({"baseline_revision":baseline.document.revision_id(),"sampled_revision":authored.document.revision_id(),
            "root_range":[170,188],"root_sample":187,"root_owner":[164,200],"owner":selected.target.node,
            "occurrence":selected.presentation,"target_id":"target-1","total_frames":240,"sampled_center_percent":70}),
        json!({"baseline_revision":baseline.document.revision_id(),"sampled_revision":workspace(d)?.document.revision_id(),
            "root_range":[170,188],"root_sample":187,"root_owner":[164,200],"owner":selected.target.node,
            "occurrence":selected.presentation,"target_id":"target-1","total_frames":workspace(d)?.plan.duration().frames(),
            "sampled_center_percent":70,"Original":d.app().source_cursor,"Edit":d.app().sequence_cursor}))?;
    let expected = workspace(d)?
        .plan
        .picture(ProjectFrame(182))
        .map_err(|e| e.to_string())?;
    d.check(
        "Minimum-size native stopped picture displays the exact committed sampled Creep",
        d.app()
            .presentation
            .picture()
            .is_some_and(|shown| shown.framing == expected.framing),
        json!(expected.framing),
        d.snapshot(),
    )?;
    d.capture("Sampled Creep explicit Play Time range inspector at 960x640")?;
    d.key(Key::Questionmark)?;
    d.key(Key::Home)?;
    d.check(
        "Sampled framing help opens at minimum workspace size without authoring",
        d.app().help_open && equivalent(&workspace(d)?.document, &authored.document),
        json!({"help":true,"write":false}),
        d.snapshot(),
    )?;
    d.capture("Sampled Creep contextual help at 960x640")?;
    d.key(Key::Escape)?;
    resize(d, 1280.0, 820.0)
}
