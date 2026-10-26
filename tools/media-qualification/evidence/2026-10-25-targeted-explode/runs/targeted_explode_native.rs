//! Targeted escalation survives native Explode and ordinary Camera editing.
//! Fixture writes use the typed store; every operation under test uses the
//! production keyboard/command router and the project service.
use super::*;
use deadpan_core::{
    AudioSample, AudioTimingId, Command, CommandRequest, FollowCentering, FrameDuration,
    FramingClock, FramingCurve, GainDb, HoldAudio, HoldRecipe, HoldVideo, MarkId, NodeId,
    OccurrenceIdentities, ProjectDocument, RepeatEscalation, RevisionId, SourceSpan,
    SourceTimestamp, TargetCorrection, TargetId, TargetPoint, TargetSample, TrackState,
    WrapAnchorPolicy, ZoomProgression, ZoomStep, framing_zoom::ZoomCurve,
};
use deadpan_store::{AccessMode, ProjectStore};
use std::collections::BTreeMap;

const BEATS: &str = "Current group beat outline pane";
const SEAM: u64 = 204; // Play two, Original picture 80, after the first 4-frame gap.
const MOVING: &str = "explode-moving";
const EDGE: &str = "explode-edge";
const EDGE_LABEL: &str = "Near the left canvas edge";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.command("sequence")?;
    d.click(BEATS)?;
    at(d, 0)?;
    let original = document(d)?.clone();
    let source = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No targeted Explode Source")?;
    d.check(
        "Targeted Explode starts from the restored unsplit 120-picture Original",
        d.app().sequence_length() == 120
            && matches!(original.nodes()[&source].kind, NodeKind::Source { .. })
            && d.app().scoped.is_none()
            && d.app().sequence_scope.groups().is_empty(),
        json!({"frames":120,"ordinary_Source":true}),
        d.snapshot(),
    )?;
    let repeat = fixture(d, &source)?;
    let seeded = document(d)?.clone();
    child_camera(d, &repeat, &seeded)?;
    at(d, SEAM)?;
    let baseline = d
        .app()
        .workspace
        .clone()
        .ok_or("No seeded Repeat workspace")?;
    let before = observations(&baseline.plan, &baseline.document, &repeat)?;
    let source_cursor = d.app().source_cursor;
    let revision = d.revision();
    d.command("explode")?;
    d.changed(&revision)?;
    idle(d)?;
    let exploded = d.app().workspace.clone().ok_or("No exploded workspace")?;
    let expected = oracle_explode(&baseline.document, &repeat)?;
    let groups = children(&exploded.document, &repeat)?;
    let later = groups
        .get(2)
        .cloned()
        .ok_or("Explode did not materialize play two")?;
    let group = &exploded.document.nodes()[&later];
    let group_children = children(&exploded.document, &later)?;
    let structure = groups.len() == 4
        && group_children.len() == 2
        && matches!(group.framing.as_ref().map(|f| &f.value),
            Some(FramingValue::Follow { target, centering: FollowCentering::CanvasClamped,
                scale_transition: None, scale, .. })
            if target.as_str() == MOVING && *scale == ratio(3,2))
        && group
            .audio_treatments
            .clip_gain()
            .is_some_and(|gain| gain.trim().millidecibels() == 1000)
        && matches!(exploded.document.nodes()[&group_children[1]].kind,
            NodeKind::Hold { ref recipe } if recipe.duration.frames() == 4 && recipe.audio == HoldAudio::Silence);
    d.check(
        "Native :explode preserves every picture, target pose, owner clock, source clock and per-frame gain",
        observations(&exploded.plan, &exploded.document, &repeat)? == before
            && canonical(&exploded.document, &baseline.document) == canonical(&expected, &baseline.document)
            && structure && d.app().sequence_length() == 368
            && d.app().sequence_cursor == SEAM && d.app().source_cursor == source_cursor
            && d.app().selected_beat.as_ref() == Some(&repeat)
            && exploded.document.nodes()[&repeat].framing == baseline.document.nodes()[&repeat].framing
            && exploded.document.nodes()[exploded.document.root()].framing
                == baseline.document.nodes()[baseline.document.root()].framing,
        json!({"pictures":368,"full_authored_document":"matches typed oracle apart fresh identities",
            "play_two":"ordinary clamped Follow + 1 dB + following silent gap","Edit":SEAM}),
        json!({"structure":structure,"observations_exact":observations(&exploded.plan,&exploded.document,&repeat)?==before,
            "document_exact":canonical(&exploded.document,&baseline.document)==canonical(&expected,&baseline.document),
            "state":d.snapshot()}),
    )?;
    d.capture("Targeted Repeat exploded with its own Follow and silent gaps preserved")?;

    // Root -> former Repeat is now ordinary navigation, with play two selected.
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Enter reaches the ordinary later-play escalation owner without isolation",
        d.app().scoped.is_none()
            && d.app().selected_beat.as_ref() == Some(&later)
            && d.app().sequence_scope.groups().last() == Some(&repeat)
            && d.app().sequence_cursor == SEAM
            && equivalent(document(d)?, &exploded.document),
        json!({"ordinary":true,"selected":later,"Edit":SEAM}),
        d.snapshot(),
    )?;
    clamped_camera(d, &later, &exploded.document)?;
    // A keep-target creep must retain the newly explicit containment policy.
    let revision = d.revision();
    d.command("creep from=1.5 to=2 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    let animated = document(d)?.clone();
    d.check(
        "A keep-target creep on an exploded ordinary play retains containment",
        matches!(animated.nodes()[&later].framing.as_ref().map(|f| &f.value),
            Some(FramingValue::Follow { centering:FollowCentering::CanvasClamped,
                scale_transition:Some(_), target, .. }) if target.as_str()==MOVING),
        json!({"Follow":"same target","containment":"canvas","animation":true}),
        json!(animated.nodes()[&later].framing),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Tab, Key::Tab])?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Edit exploded Follow arrival through native text",
        vec![egui::Event::Text("240".into())],
    )?;
    d.key(Key::Tab)?;
    d.key(Key::ArrowDown)?;
    d.step("Paint the deferred exploded Follow curve", false)?;
    d.settled()?;
    let state = camera(d);
    let expected_animation = deadpan_core::framing_zoom::retune_follow(
        animated.nodes()[&later]
            .framing
            .as_ref()
            .ok_or("No animated exploded framing")?,
        None,
        ratio(12, 5),
        ZoomCurve::Smoothstep,
    )?;
    d.check(
        "Arrival text and curve edits preserve the complete clamped Follow and remain unsaved",
        state["framing"] == json!(expected_animation)
            && state["fields_valid"] == true
            && equivalent(document(d)?, &animated),
        json!({"framing":expected_animation,"write":false}),
        state,
    )?;
    resize(d, 960.0, 640.0)?;
    painted(d, "Keeps target view inside canvas above 100%")?;
    for label in [
        "Arrival scale (%)",
        "Arrival curve",
        "Smoothstep",
        "Apply  Enter",
        "Cancel  Esc",
    ] {
        painted(d, label)?;
    }
    super::super::scenarios::footer_anchored(
        d,
        "Exploded Follow controls retain the minimum-size footer",
    )?;
    d.capture(
        "Exploded Follow keeps canvas containment while native arrival and curve fields are edited",
    )?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Cancel restores every posed layer and the complete animated exploded document",
        equivalent(document(d)?, &animated) && shown_matches_plan(d)?,
        json!("complete document and displayed plan restored"),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)?;
    undo(d)?; // Discard the saved keep-target creep.
    d.check(
        "Undo removes only the post-explode creep",
        equivalent(document(d)?, &exploded.document),
        json!("original exploded document"),
        d.snapshot(),
    )?;
    d.key(Key::Backspace)?;
    idle(d)?;

    // Reopen before history traversal to prove SQLite, rather than a UI-only inverse.
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No exploded package")?
        .path
        .clone();
    reopen(d, &path)?;
    undo(d)?;
    d.check("Durable Undo after reopen restores the exact targeted Repeat and every pre-explode observation",
        equivalent(document(d)?, &baseline.document)
            && observations(&d.app().workspace.as_ref().unwrap().plan,document(d)?,&repeat)? == before,
        json!("complete targeted Repeat restored"), d.snapshot())?;
    redo(d)?;
    d.check(
        "Durable Redo after reopen restores the complete ordinary exploded plays",
        equivalent(document(d)?, &exploded.document)
            && observations(
                &d.app().workspace.as_ref().unwrap().plan,
                document(d)?,
                &repeat,
            )? == before,
        json!("complete exploded document and all 368 observations"),
        d.snapshot(),
    )?;
    d.capture(
        "Durable Redo restores targeted exploded plays and their visible containment policy",
    )?;
    undo(d)?; // Explode.
    for _ in 0..7 {
        undo(d)?;
    } // Typed fixture setup.
    d.check(
        "Undoing the targeted Explode fixture restores the complete prior authored document",
        equivalent(document(d)?, &original) && d.app().sequence_length() == 120,
        json!("unsplit Original and all prior authored fields"),
        d.snapshot(),
    )?;
    at(d, 0)
}

fn fixture(d: &mut Driver<'_>, source: &NodeId) -> Result<NodeId, String> {
    let workspace = d
        .app()
        .workspace
        .clone()
        .ok_or("No Original fixture workspace")?;
    let path = workspace.path.clone();
    let repeat = NodeId::new("targeted-explode-repeat").map_err(string)?;
    let mut target = workspace
        .document
        .targets()
        .values()
        .next()
        .cloned()
        .ok_or("No saved target template")?;
    let index = workspace
        .sources
        .get(&target.asset)
        .and_then(|s| s.video_index.as_ref())
        .ok_or("No Original video index")?;
    let point = |x| {
        TargetPoint {
            center: [x, 500_000],
        }
        .into()
    };
    target.label = "Moving subject across three independent plays".into();
    target.span = SourceSpan::new(
        SourceTimestamp {
            ticks: index.frames()[0].pts,
            time_base: target.span.start().time_base,
        },
        SourceTimestamp {
            ticks: index.frames()[110].pts,
            time_base: target.span.start().time_base,
        },
    )
    .map_err(string)?;
    target.region = point(250_000);
    target.samples = [
        (10, 250_000, TrackState::Tracked),
        (40, 500_000, TrackState::Tracked),
        (50, 500_000, TrackState::Lost),
        (80, 450_000, TrackState::Tracked),
        (100, 750_000, TrackState::Tracked),
    ]
    .into_iter()
    .map(|(ordinal, x, state)| TargetSample {
        at: index.frames()[ordinal].pts,
        region: point(x),
        confidence: 900,
        state,
    })
    .collect();
    target.corrections = vec![TargetCorrection {
        at: index.frames()[60].pts,
        region: point(350_000),
    }];
    target.provenance = None;
    let mut edge = target.clone();
    edge.label = EDGE_LABEL.into();
    edge.region = point(10_000);
    edge.samples.clear();
    edge.corrections.clear();
    let mut child = Framing::creep(
        FramingPose::new(ratio(2, 5), ratio(1, 2), ExactRatio::ONE).map_err(string)?,
        FramingPose::new(ratio(3, 5), ratio(1, 2), ExactRatio::ONE).map_err(string)?,
        FramingCurve::Smoothstep,
    )
    .map_err(string)?;
    child.clock = FramingClock::RetainedOutput {
        offset: ratio(15, 2),
        duration: ExactRatio::integer(180),
    };
    let follow = |scale| Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            centering: FollowCentering::Unconstrained,
            target: TargetId::new(MOVING).unwrap(),
            scale,
            fallback: FramingPose {
                scale,
                ..FramingPose::identity()
            },
            scale_transition: None,
        },
    };
    let gap = HoldRecipe {
        duration: FrameDuration::new(4).map_err(string)?,
        video: HoldVideo::Freeze {
            asset: target.asset.clone(),
            timestamp: SourceTimestamp {
                ticks: index.frames()[80].pts,
                time_base: target.span.start().time_base,
            },
        },
        picture_context: None,
        audio: HoldAudio::Silence,
    };
    close(d)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).map_err(string)?;
    for (suffix, command) in [
        (
            "moving",
            Command::SetTarget {
                id: TargetId::new(MOVING).unwrap(),
                target,
            },
        ),
        (
            "edge",
            Command::SetTarget {
                id: TargetId::new(EDGE).unwrap(),
                target: edge,
            },
        ),
        (
            "child",
            Command::SetFraming {
                node: source.clone(),
                framing: Some(child),
            },
        ),
        (
            "repeat",
            Command::WrapRepeat {
                node: source.clone(),
                id: repeat.clone(),
                plays: 3,
                gap: Some(gap),
                anchor_policy: WrapAnchorPolicy::default(),
            },
        ),
        (
            "escalation",
            Command::SetRepeatEscalation {
                node: repeat.clone(),
                escalation: Some(RepeatEscalation {
                    gain_step: GainDb::new(1000).map_err(string)?,
                    zoom: Some(ZoomStep {
                        step: ratio(1, 2),
                        progression: ZoomProgression::Add,
                        target: Some(TargetId::new(MOVING).unwrap()),
                    }),
                }),
            },
        ),
        (
            "repeat-follow",
            Command::SetFraming {
                node: repeat.clone(),
                framing: Some(follow(ratio(11, 10))),
            },
        ),
        (
            "ancestor",
            Command::SetFraming {
                node: workspace.document.root().clone(),
                framing: Some(follow(ratio(9, 8))),
            },
        ),
    ] {
        apply(&mut store, &format!("targeted-explode-{suffix}"), command)?;
    }
    drop(store);
    open(d, &path)?;
    d.command("sequence")?;
    d.click(BEATS)?;
    at(d, SEAM)?;
    d.check("Typed setup retains a moving target live at the chosen seam with child, Repeat and parent framing",
        d.app().selected_beat.as_ref()==Some(&repeat) && d.app().sequence_length()==368
            && d.app().workspace.as_ref().unwrap().plan.picture(ProjectFrame(SEAM as i64)).map_err(string)?
                .framing.iter().any(|layer|layer.escalation && layer.pose.is_some()),
        json!({"Repeat":repeat,"Edit":SEAM,"Original_picture":80,"frames":368}),d.snapshot())?;
    Ok(repeat)
}

fn child_camera(
    d: &mut Driver<'_>,
    repeat: &NodeId,
    seeded: &ProjectDocument,
) -> Result<(), String> {
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    d.chord(&[Key::Num8, Key::Num0, Key::L])?;
    d.settled()?;
    let source_cursor = d.app().source_cursor;
    let original_layers = json!(
        d.app()
            .presentation
            .picture()
            .ok_or("No scoped picture")?
            .framing
    );
    open_camera(d)?;
    let original_escalation = d
        .app()
        .presentation
        .picture()
        .unwrap()
        .framing
        .iter()
        .find(|layer| layer.escalation && layer.instance.node == *repeat)
        .and_then(|layer| layer.pose);
    d.chord(&[Key::Num1, Key::Num0, Key::H])?;
    d.settled()?;
    let changed = d
        .app()
        .presentation
        .picture()
        .unwrap()
        .framing
        .iter()
        .find(|layer| layer.escalation && layer.instance.node == *repeat)
        .and_then(|layer| layer.pose);
    d.check(
        "A real child Camera draft reprojects the outer targeted escalation before any save",
        original_escalation != changed
            && equivalent(document(d)?, seeded)
            && d.app().sequence_cursor == SEAM
            && d.app().source_cursor == source_cursor,
        json!({"outer_escalation":"changes with temporary inner framing","write":false}),
        json!({"before":original_escalation,"draft":changed,"state":d.snapshot()}),
    )?;
    d.capture("Child Camera temporary crop reprojects targeted outer Repeat escalation")?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Escape restores every child, escalation, Repeat and parent layer",
        json!(d.app().presentation.picture().unwrap().framing) == original_layers
            && equivalent(document(d)?, seeded),
        json!("all original evaluated layers"),
        d.snapshot(),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Num1, Key::Num0, Key::H])?;
    d.settled()?;
    let preview = shown_observation(d)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check("Saving the child Camera draft matches the committed RenderPlan including its outer escalation",
        shown_observation(d)?==preview && shown_matches_plan(d)?
            && d.app().sequence_cursor==SEAM && d.app().source_cursor==source_cursor,
        json!("same composed provider, ordered poses and retained context"),d.snapshot())?;
    d.key(Key::Backspace)?;
    idle(d)?;
    undo(d)?;
    d.check(
        "Undo restores the complete Repeat after the scoped child Camera save",
        equivalent(document(d)?, seeded),
        json!("seeded Repeat"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.click(BEATS)?;
    at(d, SEAM)
}

fn clamped_camera(
    d: &mut Driver<'_>,
    later: &NodeId,
    exploded: &ProjectDocument,
) -> Result<(), String> {
    let before = json!(
        d.app()
            .presentation
            .picture()
            .ok_or("No ordinary play picture")?
            .framing
    );
    open_camera(d)?;
    d.check(
        "Camera opens the exploded play as the exact clamped Follow without changing its recipe",
        camera(d)["framing"] == json!(exploded.nodes()[later].framing)
            && camera(d)["framing"]["value"]["centering"] == json!(FollowCentering::CanvasClamped),
        json!(exploded.nodes()[later].framing),
        camera(d),
    )?;
    // The replacement is deliberately close to the edge, so scale changes
    // alter the clamped center and exercise the locked-center handle sync.
    d.key(Key::F)?;
    d.settled()?;
    let picker = camera(d)["picker"]
        .as_array()
        .cloned()
        .ok_or("No Camera target picker")?;
    let number = picker
        .iter()
        .find(|entry| entry["label"] == EDGE_LABEL)
        .and_then(|entry| entry["number"].as_u64())
        .ok_or("Edge target is not on the first target page")?;
    d.key(Key::from_name(&number.to_string()).ok_or("Invalid edge target digit")?)?;
    d.key(Key::Tab)?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Set the exploded Follow scale to exactly 200%",
        vec![egui::Event::Text("200".into())],
    )?;
    d.key(Key::Tab)?;
    d.settled()?;
    let state = camera(d);
    let shown: FramingPose = d
        .app()
        .presentation
        .picture()
        .unwrap()
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == *later)
        .and_then(|layer| layer.pose)
        .ok_or("No clamped play pose")?;
    let handle: FramingPose =
        serde_json::from_value(state["draft_pose"].clone()).map_err(string)?;
    let draft: Framing = serde_json::from_value(state["framing"].clone()).map_err(string)?;
    d.check("Target replacement and scale preserve containment and keep locked center fields on the displayed edge view",
        matches!(&draft.value,FramingValue::Follow{target,centering:FollowCentering::CanvasClamped,..} if target.as_str()==EDGE)
            && shown.center_x==handle.center_x && shown.center_y==handle.center_y
            && shown.scale==ExactRatio::integer(2) && shown.center_x==ratio(1,4)
            && equivalent(document(d)?,exploded),
        json!({"target":EDGE,"centering":"canvas","handle_center":"shown center","write":false}),
        json!({"camera":state,"shown":shown}))?;
    // Move away from 200% first. Typing `200.` must then move the clamped
    // center and preserve its unfinished decimal through that same preview.
    // Otherwise a whole-field sync can erase the dot before the later `5`.
    d.key(Key::Tab)?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Move the clamped Follow scale away from the decimal witness",
        vec![egui::Event::Text("210".into())],
    )?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Type an unfinished decimal while the Follow center changes",
        vec![egui::Event::Text("200.".into())],
    )?;
    d.events(
        "Complete the Follow decimal in a separate native event batch",
        vec![egui::Event::Text("5".into())],
    )?;
    d.key(Key::Tab)?;
    d.settled()?;
    let state = camera(d);
    let handle: FramingPose =
        serde_json::from_value(state["draft_pose"].clone()).map_err(string)?;
    let draft: Framing = serde_json::from_value(state["framing"].clone()).map_err(string)?;
    let shown = d
        .app()
        .presentation
        .picture()
        .unwrap()
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == *later)
        .and_then(|layer| layer.pose)
        .ok_or("No decimal Follow pose")?;
    let expected = FramingPose {
        scale: ratio(401, 200),
        ..FramingPose::identity()
    }
    .quantized()
    .map_err(string)?
    .scale;
    d.check(
        "Separate native decimal batches retain 200.5% while the clamped Follow center updates",
        state["fields_valid"] == true
            && handle.scale == expected
            && shown.scale == expected
            && handle.center_x == shown.center_x
            && handle.center_y == shown.center_y
            && matches!(draft.value, FramingValue::Follow {
                centering: FollowCentering::CanvasClamped, scale, ..
            } if scale == expected)
            && equivalent(document(d)?, exploded),
        json!({"typed_batches":["200.","5"],"scale":expected,"write":false}),
        json!({"camera":state,"shown":shown}),
    )?;
    resize(d, 960.0, 640.0)?;
    for label in [
        "Keeps target view inside canvas above 100%",
        "Apply  Enter",
        "Cancel  Esc",
    ] {
        painted(d, label)?;
    }
    d.capture("Exploded ordinary play Camera preserves containment after edge target replacement and scaling")?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Cancel restores all exploded play and outer Follow poses",
        equivalent(document(d)?, exploded)
            && json!(d.app().presentation.picture().unwrap().framing) == before,
        json!("complete exploded document and displayed layer stack"),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)
}

fn observations(
    plan: &deadpan_plan::RenderPlan,
    document: &ProjectDocument,
    repeat: &NodeId,
) -> Result<Vec<Value>, String> {
    let root = plan.metadata().root.clone();
    (0..plan.duration().frames()).map(|frame|{
        let sample=plan.picture(ProjectFrame(frame)).map_err(string)?;
        let at=plan.metadata().presentation_basis.frame_rate.audio_boundary(ProjectFrame(frame)).map_err(string)?;
        let audio=plan.audio(at..AudioSample(at.0+1),Default::default()).map_err(string)?;
        let audio_clocks=audio.spans.iter().map(|span|Ok::<_,String>(json!({
            "content":span.content,"local":span.sampling.local_at(at).map_err(string)?,
            "source":if matches!(span.content,deadpan_plan::AudioContent::Source{..}){
                Some(span.source_point(at).map_err(string)?)}else{None}
        }))).collect::<Result<Vec<_>,_>>()?;
        let gain=plan.audio_gain_owners(at..AudioSample(at.0+1),Default::default()).map_err(string)?;
        let gains=gain.spans().iter().map(|span|{
            span.owners().iter().try_fold(ExactRatio::ZERO,|sum,owner|{
                let local=owner.sampling().local_at(at).map_err(string)?;
                let clip=owner.treatments().and_then(|t|t.clip_gain()).map(|clip|clip.evaluate(local))
                    .transpose().map_err(string)?.map_or(ExactRatio::ZERO,|value|value.millidecibels);
                sum.checked_add(ExactRatio::integer(owner.escalation_millidecibels())).and_then(|sum|sum.checked_add(clip)).map_err(string)
            })
        }).collect::<Result<Vec<_>,_>>()?;
        let owner_clocks=sample.framing.iter().filter(|layer|!layer.escalation
            && (layer.instance.node==*repeat||layer.instance.node==root))
            .map(|layer|json!({"node":layer.instance.node,"local":layer.local_position,
                "duration":layer.effective_duration,"pose":layer.pose})).collect::<Vec<_>>();
        let source_clocks=sample.framing.iter().filter(|layer|{
            // Every live Source has this copied retained camera path; gaps have none.
            !layer.escalation && matches!(document.nodes().get(&layer.instance.node).map(|node|&node.kind),
                Some(NodeKind::Source{..}))
        }).map(|layer|json!({"local":layer.local_position,"duration":layer.effective_duration})).collect::<Vec<_>>();
        Ok(json!({"picture":sample.picture,"context":sample.picture_context.as_deref(),
            "poses":sample.framing.iter().filter_map(|layer|layer.pose).collect::<Vec<_>>(),
            "owners":owner_clocks,"sources":source_clocks,"audio":audio_clocks,"gain_millidecibels":gains,"captions":sample.captions}))
    }).collect()
}
fn shown_observation(d: &Driver<'_>) -> Result<Value, String> {
    let picture = d
        .app()
        .presentation
        .picture()
        .ok_or("No displayed targeted picture")?;
    Ok(
        json!({"follow_point":picture.follow_point,"context":picture.picture_context.as_deref(),
        "poses":picture.framing.iter().filter_map(|layer|layer.pose).collect::<Vec<_>>()}),
    )
}
fn shown_matches_plan(d: &Driver<'_>) -> Result<bool, String> {
    let sample = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No native plan")?
        .plan
        .picture(ProjectFrame(d.app().sequence_cursor as i64))
        .map_err(string)?;
    Ok(shown_observation(d)?
        == json!({"follow_point":sample.picture.follow_point().map(|(a,p)|(a.clone(),p)),
        "context":sample.picture_context.as_deref(),"poses":sample.framing.iter().filter_map(|layer|layer.pose).collect::<Vec<_>>()}))
}
fn oracle_explode(document: &ProjectDocument, repeat: &NodeId) -> Result<ProjectDocument, String> {
    let requirements = document.explode_requirements(repeat).map_err(string)?;
    let revision = RevisionId::new("targeted-explode-oracle").map_err(string)?;
    let identities = OccurrenceIdentities {
        nodes: (0..requirements.nodes)
            .map(|n| NodeId::new(format!("oracle-node-{n}")).map_err(string))
            .collect::<Result<_, _>>()?,
        marks: (0..requirements.marks)
            .map(|n| MarkId::new(format!("oracle-mark-{n}")).map_err(string))
            .collect::<Result<_, _>>()?,
    };
    deadpan_core::apply_with_result(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision.clone(),
            command: Command::Explode {
                node: repeat.clone(),
                identities,
                timing: AudioTimingId {
                    allocation: revision,
                    ordinal: 0,
                },
            },
        },
    )
    .map(|(_, document)| document)
    .map_err(string)
}
fn canonical(document: &ProjectDocument, before: &ProjectDocument) -> Value {
    let mut replacements = BTreeMap::from([(
        document.revision_id().as_str().to_owned(),
        "NEW-REVISION".to_owned(),
    )]);
    let mut pending = vec![document.root().clone()];
    let mut next = 0;
    while let Some(id) = pending.pop() {
        if !before.nodes().contains_key(&id) {
            replacements.insert(id.as_str().to_owned(), format!("NEW-NODE-{next}"));
            next += 1;
        }
        let children: Vec<_> = document.children(&id).cloned().collect();
        pending.extend(children.into_iter().rev());
    }
    for (n, id) in document
        .marks()
        .keys()
        .filter(|id| !before.marks().contains_key(*id))
        .enumerate()
    {
        replacements.insert(id.as_str().to_owned(), format!("NEW-MARK-{n}"));
    }
    fn visit(value: Value, replacements: &BTreeMap<String, String>) -> Value {
        match value {
            Value::String(value) => {
                Value::String(replacements.get(&value).cloned().unwrap_or(value))
            }
            Value::Array(values) => {
                Value::Array(values.into_iter().map(|v| visit(v, replacements)).collect())
            }
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            replacements.get(&k).cloned().unwrap_or(k),
                            visit(v, replacements),
                        )
                    })
                    .collect(),
            ),
            value => value,
        }
    }
    visit(json!(document), &replacements)
}
fn camera(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |c| c.harness_state())
}
fn children(document: &ProjectDocument, node: &NodeId) -> Result<Vec<NodeId>, String> {
    match &document.nodes()[node].kind {
        NodeKind::Sequence { children } => Ok(children.clone()),
        _ => Err("Expected ordinary Sequence".into()),
    }
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|w| w.document.as_ref())
        .ok_or("No targeted Explode document".into())
}
fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut value = json!(actual);
    value["revision_id"] = json!(expected.revision_id());
    value == json!(expected)
}
fn open_camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Targeted Explode Camera opens", |app| app.camera.is_some())?;
    d.settled()
}
fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        for digit in frame.to_string().chars() {
            d.key(Key::from_name(&digit.to_string()).ok_or("Invalid targeted frame digit")?)?;
        }
        d.key(Key::L)?;
    }
    d.settled()
}
fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Targeted Explode receipt finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}
fn undo(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)
}
fn redo(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    idle(d)
}
fn close(d: &mut Driver<'_>) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close writer before targeted fixture setup", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })
}
fn open(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    d.app()
        .service
        .submit(ProjectRequest::Open(path.to_path_buf()))?;
    d.wait_for("Reopen targeted fixture through native service", |app| {
        !app.service.is_busy() && app.workspace.as_ref().is_some_and(|w| w.path == path)
    })?;
    idle(d)
}
fn reopen(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    close(d)?;
    open(d, path)
}
fn apply(store: &mut ProjectStore, revision: &str, command: Command) -> Result<(), String> {
    let document = store.snapshot().map_err(string)?;
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).map_err(string)?,
            command,
        })
        .map_err(string)?;
    Ok(())
}
fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("No root viewport")?
        .inner_rect = Some(rect);
    d.step("Resize targeted Explode Camera", true)?;
    d.settled()
}
fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = super::super::scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Targeted Explode control is fully painted: {label}"),
        !paint.is_empty()
            && paint
                .iter()
                .all(|p| p["fully_visible"] == true && p["elided"] == false),
        json!("fully visible with no later opaque overlap"),
        json!(paint),
    )
}
fn string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
