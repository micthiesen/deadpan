//! Native gain commands, buffered drafts and identity-tagged comparison clocks.

use super::scenarios::{PICTURE_TOLERANCE_PIXELS, picture_contains_rect};
use super::*;
use deadpan_core::{
    AudioSample, AudioTreatments, ClipGain, ExactRatio, GainClock, GainCurve, GainDb, GainEnvelope,
    GainRange, GainSegment,
};
use deadpan_playback::{ContentIdentity, Phase, Snapshot, Update};
use egui::{Key, Modifiers};

const TRIM: &str = "Whole beat trim · dB";
const UPDATE_TRIM: &str = "Set trim";
const APPLY: &str = "Apply · Enter";
const CANCEL: &str = "Cancel · Esc";
const PLAY: &str = "Audition · Space";
const PAUSE: &str = "Pause · Space";
const BEATS: &str = "Current group beat outline pane";
const DRAFT_FOCUS: &str = "Gain draft keyboard focus";
const GRAPH: &str = "Gain envelope graph · owner-output frames";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Gain uses real qualified source evidence, writer previews and durable commands. Audio delivery is injected through typed playback updates; this replay does not prepare PCM, open an audio device or establish acoustic quality. Canonical Before/Draft PCM and cache separation belong to the playback tests.".into());
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num7, Key::L])?;
    d.settled()?;
    let owner = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No gain owner selected")?;
    let initial = document(d)?.nodes().clone();
    let entry = recipe(d, &owner)?;
    let editor = editor_state(d);

    let before = d.revision();
    d.key(Key::Plus)?;
    d.changed(&before)?;
    let raised = recipe(d, &owner)?;
    d.check(
        "Normal plus adds exactly 3 dB to the captured beat without moving either cursor",
        trim(&raised) == 3_000 && editor_state(d) == editor,
        json!({"millidecibels":3000,"editor":editor}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.chord(&[Key::Num2, Key::Minus])?;
    d.changed(&before)?;
    d.check(
        "Counted minus changes gain by two 3 dB steps in one command",
        trim(&recipe(d, &owner)?) == -3_000 && editor_state(d) == editor,
        json!({"millidecibels":-3000,"editor":editor}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "One undo restores the exact recipe before counted gain",
        recipe(d, &owner)? == raised,
        json!(raised),
        json!(recipe(d, &owner)?),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Two gain undos restore the full protected baseline",
        document(d)?.nodes() == &initial
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| !workspace.can_undo),
        json!(initial),
        d.snapshot(),
    )?;

    let before = d.revision();
    d.command("gain -4.125")?;
    d.changed(&before)?;
    let absolute = recipe(d, &owner)?;
    d.check(
        "Direct gain entry retains exact millidecibels",
        trim(&absolute) == -4_125,
        json!(-4125),
        json!(absolute),
    )?;
    let before = d.revision();
    d.command("gain-mute")?;
    d.changed(&before)?;
    let muted = recipe(d, &owner)?;
    d.check(
        "True mute retains the authored trim",
        muted.clip_gain().is_some_and(|clip| clip.muted()) && trim(&muted) == -4_125,
        json!({"muted":true,"millidecibels":-4125}),
        json!(muted),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Mute undo restores the complete prior recipe",
        recipe(d, &owner)? == absolute,
        json!(absolute),
        json!(recipe(d, &owner)?),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;

    wrong_focus(d, &initial)?;
    d.click(BEATS)?;
    let editor = editor_state(d);
    let base_revision = d.revision();
    open(d)?;
    let initial_proposal = prepared(d)?.clone();
    d.check(
        "Opening Gain creates a proposed identity without authoring history",
        d.revision() == base_revision
            && document(d)?.nodes() == &initial
            && matches!(&initial_proposal.content, ContentIdentity::Proposed { base_revision: base, draft, change } if base.as_str() == base_revision && *draft > 0 && *change > 0)
            && initial_proposal.document.revision_id().as_str() != base_revision
            && d.app().workspace.as_ref().is_some_and(|workspace| !workspace.can_undo),
        json!({"committed_revision":base_revision,"new_proposed_revision":true,"can_undo":false}),
        d.snapshot(),
    )?;
    field(d, TRIM, "-6.250")?;
    d.check(
        "Native trim text stays buffered until its Update action",
        prepared(d)?.content == initial_proposal.content
            && prepared(d)?.document.nodes()[&owner].audio_treatments == entry
            && d.revision() == base_revision
            && d.rect(APPLY).is_err(),
        json!({"history_unchanged":true,"proposal_unchanged":true,"apply_enabled":false}),
        d.snapshot(),
    )?;
    d.click(UPDATE_TRIM)?;
    wait_prepared(d)?;
    d.check(
        "Update trim prepares the buffered value without changing the workspace",
        trim(&prepared(d)?.document.nodes()[&owner].audio_treatments) == -6_250
            && prepared(d)?.content != initial_proposal.content
            && d.revision() == base_revision
            && document(d)?.nodes() == &initial,
        json!({"proposed_millidecibels":-6250,"committed_revision":base_revision}),
        d.snapshot(),
    )?;
    coalesced_trim(d, &owner, &base_revision)?;
    envelope_and_mute(d, &owner, &base_revision)?;
    gain_layout(d, &base_revision)?;
    let chosen = prepared(d)?.clone();
    let expected_nodes = chosen.document.nodes().clone();
    d.capture("Unsaved gain draft ready for Before and Draft comparison")?;
    audition(d, &chosen, &editor, &base_revision)?;
    d.click(APPLY)?;
    d.changed(&base_revision)?;
    let applied = d.revision();
    d.check(
        "Apply authors the latest complete treatment once and retains the captured owner",
        d.app().gain.is_none()
            && document(d)?.nodes() == &expected_nodes
            && editor_state(d) == editor,
        json!({"nodes":expected_nodes,"editor":editor}),
        d.snapshot(),
    )?;
    d.click(BEATS)?;
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.check(
        "One undo restores the pre-draft document with no preview history entries",
        document(d)?.nodes() == &initial
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| !workspace.can_undo),
        json!({"nodes":initial,"can_undo":false}),
        d.snapshot(),
    )?;
    native_text_cancel(d, &initial)?;
    Ok(())
}

fn wrong_focus(
    d: &mut Driver<'_>,
    nodes: &std::collections::BTreeMap<NodeId, deadpan_core::BeatNode>,
) -> Result<(), String> {
    let revision = d.revision();
    d.command("source")?;
    d.key(Key::Plus)?;
    d.command("gain -6")?;
    d.check(
        "Original gain input cannot edit the retained picture beat",
        d.revision() == revision && document(d)?.nodes() == nodes && d.app().gain.is_none(),
        json!({"revision":revision,"draft":false}),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.click("Original and sounds pane")?;
    d.key(Key::Minus)?;
    d.command("gain")?;
    d.check(
        "Sources focus cannot fall through to a retained beat gain target",
        d.app().pane == Pane::Sources
            && d.revision() == revision
            && document(d)?.nodes() == nodes
            && d.app().gain.is_none(),
        json!({"pane":"Sources","revision":revision,"draft":false}),
        d.snapshot(),
    )
}

fn coalesced_trim(d: &mut Driver<'_>, owner: &NodeId, revision: &str) -> Result<(), String> {
    d.app_mut().feedback.hold_project_updates = true;
    field(d, TRIM, "-9.500")?;
    d.click(UPDATE_TRIM)?;
    d.wait_for(
        "First gain preparation completes while its reply is held",
        |app| !app.service.is_busy(),
    )?;
    field(d, TRIM, "-12.000")?;
    d.click(UPDATE_TRIM)?;
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Release a superseded gain proposal", true)?;
    d.check(
        "A superseded successful proposal never becomes the current Draft",
        d.app()
            .gain
            .as_ref()
            .is_some_and(|draft| draft.prepared_snapshot().is_none())
            && d.revision() == revision,
        json!({"prepared":false,"revision":revision}),
        d.snapshot(),
    )?;
    wait_prepared(d)?;
    d.check(
        "Coalesced gain preparation admits only the latest buffered recipe",
        trim(&prepared(d)?.document.nodes()[owner].audio_treatments) == -12_000
            && d.revision() == revision,
        json!({"proposed_millidecibels":-12000,"revision":revision}),
        d.snapshot(),
    )
}

fn envelope_and_mute(d: &mut Driver<'_>, owner: &NodeId, revision: &str) -> Result<(), String> {
    let committed_nodes = document(d)?.nodes().clone();
    let owner_frames = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No gain workspace")?
        .plan
        .node_duration(owner)
        .ok_or("Gain owner has no duration")?
        .frames();
    click_control(d, "Add envelope")?;
    wait_prepared(d)?;
    let clip = prepared(d)?.document.nodes()[owner]
        .audio_treatments
        .clip_gain()
        .ok_or("Add envelope lost the ClipGain stage")?;
    d.check(
        "Add envelope explicitly prepares unity over the exact owner allocation",
        clip.trim().millidecibels() == -12_000
            && clip.envelopes().len() == 1
            && clip.envelopes()[0].clock() == GainClock::OwnerOutput
            && clip.envelopes()[0].range().start() == ExactRatio::ZERO
            && clip.envelopes()[0].range().end() == ExactRatio::integer(owner_frames)
            && clip.envelopes()[0].initial() == GainDb::UNITY
            && clip.envelopes()[0].segments().len() == 1
            && clip.envelopes()[0].segments()[0].value() == GainDb::UNITY
            && clip.envelopes()[0].segments()[0].curve() == GainCurve::Linear
            && d.revision() == revision,
        json!({"clock":"owner_output","range":[0,owner_frames],"trim_millidecibels":-12000,"revision":revision}),
        json!(prepared(d)?.document.nodes()[owner].audio_treatments),
    )?;

    let before_range = prepared(d)?.content.clone();
    field(d, "In · owner frames", "1/2")?;
    d.key(Key::Tab)?;
    expect_focus(d, "Out · exclusive")?;
    replace_focused_text(d, "179/2")?;
    d.check(
        "Exact envelope In and Out remain buffered without replacing the proposal",
        prepared(d)?.content == before_range && d.rect(APPLY).is_err() && d.revision() == revision,
        json!({"proposal_unchanged":true,"apply_enabled":false,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Tab)?;
    expect_focus(d, "Update range")?;
    d.key(Key::Enter)?;
    wait_prepared(d)?;
    let start = ExactRatio::new(1, 2).map_err(|error| error.to_string())?;
    let end = ExactRatio::new(179, 2).map_err(|error| error.to_string())?;
    let range = GainRange::new(start, end).map_err(|error| error.to_string())?;
    d.check(
        "Native Tab and Enter update fractional owner-frame boundaries exactly",
        prepared(d)?.document.nodes()[owner]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| clip.envelopes()[0].range() == range)
            && d.app().gain.is_some()
            && d.revision() == revision,
        json!({"range":["1/2","179/2"],"draft":true,"revision":revision}),
        d.snapshot(),
    )?;

    field(d, "Value · dB", "-4.125")?;
    click_control(d, "Update key")?;
    wait_prepared(d)?;
    let initial = GainDb::new(-4_125).map_err(|error| error.to_string())?;
    d.check(
        "Updating the initial key preserves its exact range start and owner clock",
        prepared(d)?.document.nodes()[owner]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| {
                clip.envelopes()[0].initial() == initial
                    && clip.envelopes()[0].range() == range
                    && clip.envelopes()[0].clock() == GainClock::OwnerOutput
            })
            && d.revision() == revision,
        json!({"initial_millidecibels":-4125,"range":["1/2","179/2"]}),
        d.snapshot(),
    )?;
    let before_navigation = prepared(d)?.content.clone();
    click_control(d, "Next point")?;
    d.check(
        "Next point selects the retained final key without preparing new content",
        prepared(d)?.content == before_navigation
            && field_value(d, "Time · owner frames")? == "179/2"
            && field_value(d, "Value · dB")? == "0"
            && d.revision() == revision,
        json!({"time":"179/2","value":"0","proposal_unchanged":true}),
        d.snapshot(),
    )?;
    field(d, "Value · dB", "-9.5")?;
    click_control(d, "Linear")?;
    click_popup_item(d, "Smoothstep")?;
    click_control(d, "Update key")?;
    wait_prepared(d)?;
    let ending = GainDb::new(-9_500).map_err(|error| error.to_string())?;
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range,
        initial,
        vec![
            GainSegment::new(end, ending, GainCurve::Smoothstep)
                .map_err(|error| error.to_string())?,
        ],
    )
    .map_err(|error| error.to_string())?;
    d.check(
        "The final key retains its exact time and owns the selected Smoothstep interpolation",
        prepared(d)?.document.nodes()[owner]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| clip.envelopes() == std::slice::from_ref(&envelope))
            && d.revision() == revision,
        json!(envelope),
        json!(prepared(d)?.document.nodes()[owner].audio_treatments),
    )?;
    click_control(d, "Add point")?;
    field(d, "Time · owner frames", "91/2")?;
    field(d, "Value · dB", "6")?;
    click_control(d, "Insert key")?;
    wait_prepared(d)?;
    let interior = ExactRatio::new(91, 2).map_err(|error| error.to_string())?;
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range,
        initial,
        vec![
            GainSegment::new(
                interior,
                GainDb::new(6_000).map_err(|error| error.to_string())?,
                GainCurve::Linear,
            )
            .map_err(|error| error.to_string())?,
            GainSegment::new(end, ending, GainCurve::Smoothstep)
                .map_err(|error| error.to_string())?,
        ],
    )
    .map_err(|error| error.to_string())?;
    d.check(
        "Insert key adds an exact fractional interior point and retains the final incoming curve",
        prepared(d)?.document.nodes()[owner]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| clip.envelopes() == std::slice::from_ref(&envelope))
            && d.revision() == revision,
        json!(envelope),
        json!(prepared(d)?.document.nodes()[owner].audio_treatments),
    )?;
    let before_navigation = prepared(d)?.content.clone();
    click_control(d, "Previous point")?;
    d.check(
        "Previous point restores the initial exact key fields without changing the proposal",
        field_value(d, "Time · owner frames")? == "1/2"
            && field_value(d, "Value · dB")? == "-4.125"
            && prepared(d)?.content == before_navigation,
        json!({"time":"1/2","value":"-4.125","proposal_unchanged":true}),
        d.snapshot(),
    )?;
    click_control(d, "Next point")?;
    click_control(d, "Next point")?;

    let before_mute = prepared(d)?.content.clone();
    click_control(d, "Add mute range")?;
    d.check(
        "Adding a mute-range row buffers an explicit range before insertion",
        prepared(d)?.content == before_mute
            && prepared(d)?.document.nodes()[owner]
                .audio_treatments
                .clip_gain()
                .is_some_and(|clip| clip.mute_ranges().is_empty())
            && d.rect(APPLY).is_err()
            && d.revision() == revision,
        json!({"mute_ranges":[],"apply_enabled":false,"revision":revision}),
        d.snapshot(),
    )?;
    field(d, "Mute In · frames", "3/2")?;
    d.key(Key::Tab)?;
    expect_focus(d, "Mute Out · exclusive")?;
    replace_focused_text(d, "5/2")?;
    d.key(Key::Tab)?;
    expect_focus(d, "Insert mute range")?;
    d.key(Key::Enter)?;
    wait_prepared(d)?;
    let mute = GainRange::new(
        ExactRatio::new(3, 2).map_err(|error| error.to_string())?,
        ExactRatio::new(5, 2).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let expected = AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(-12_000).map_err(|error| error.to_string())?,
            false,
            vec![envelope],
            vec![mute],
        )
        .map_err(|error| error.to_string())?,
    );
    d.check(
        "Native mute-range insertion preserves the complete trim and envelope recipe without history",
        prepared(d)?.document.nodes()[owner].audio_treatments == expected
            && document(d)?.nodes() == &committed_nodes
            && d.revision() == revision,
        json!({"recipe":expected,"revision":revision,"committed_nodes_unchanged":true}),
        json!(prepared(d)?.document.nodes()[owner].audio_treatments),
    )
}

fn gain_layout(d: &mut Driver<'_>, revision: &str) -> Result<(), String> {
    let content = prepared(d)?.content.clone();
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(viewport);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing gain replay viewport")?
            .inner_rect = Some(viewport);
        d.step("Paint the exact gain draft at the resized viewport", true)?;
        toolbar_visible(d, width, height)?;
        gain_viewer_visible(d, width, height)?;
        gain_heading_left_aligned(d)?;
        for label in [
            TRIM,
            UPDATE_TRIM,
            "In · owner frames",
            "Out · exclusive",
            "Update range",
            "Time · owner frames",
            "Value · dB",
            "Smoothstep",
            "Update key",
            "Mute In · frames",
            "Mute Out · exclusive",
            "Update mute range",
        ] {
            reveal_control(d, label)?;
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Native gain controls remain fully painted and reachable after bounded scrolling",
                painted_control(d, label).is_some(),
                json!({"label":label,"viewport":[width,height]}),
                json!(paint),
            )?;
        }
        toolbar_visible(d, width, height)?;
        frame_graph(d)?;
        d.capture(&format!(
            "Complete owner gain envelope and fixed comparison actions at {width}x{height}"
        ))?;
        d.check(
            "Resize and inspection scrolling do not change the prepared gain or project history",
            prepared(d)?.content == content && d.revision() == revision,
            json!({"proposal_unchanged":true,"revision":revision}),
            d.snapshot(),
        )?;
        populated_tab_visibility(d, width, height, revision)?;
    }
    Ok(())
}

fn populated_tab_visibility(
    d: &mut Driver<'_>,
    width: f32,
    height: f32,
    revision: &str,
) -> Result<(), String> {
    let content = prepared(d)?.content.clone();
    for (direction, modifiers) in [("Tab", Modifiers::NONE), ("Shift Tab", Modifiers::SHIFT)] {
        // Establish one known start. From this point to the completed circuit,
        // only native Tab events and normal UI frames may reveal controls.
        d.click(DRAFT_FOCUS)?;
        let mut visited = Vec::new();
        let mut completed = false;
        for step in 1..=60 {
            d.key_modified(Key::Tab, modifiers)?;
            // Give ordinary focus-driven scrolling its animation time at both
            // supported replay rates. No pointer wheel or reveal helper runs.
            for _ in 0..(d.options.hz / 5) {
                d.step("Settle native gain keyboard focus", false)?;
            }
            let focused = d
                .harness
                .root()
                .children_recursive()
                .filter(|node| node.accesskit_node().is_focused())
                .take(4)
                .map(|node| {
                    let access = node.accesskit_node();
                    (
                        access
                            .label()
                            .or_else(|| access.value())
                            .unwrap_or_default(),
                        node.rect(),
                        !access.is_disabled() && !access.is_hidden(),
                    )
                })
                .collect::<Vec<_>>();
            let name = focused
                .first()
                .map_or("No focused control", |(name, _, _)| name.as_str());
            let visible = match focused.as_slice() {
                [(name, rect, true)] if name == DRAFT_FOCUS => {
                    d.harness.ctx.content_rect().contains_rect(*rect)
                        && d.harness.output().shapes.iter().any(|clipped| {
                            let egui::Shape::Text(text) = &clipped.shape else {
                                return false;
                            };
                            text.galley.text().starts_with("Gain · ")
                                && clipped.clip_rect.contains_rect(text.visual_bounding_rect())
                                && clipped.clip_rect.contains_rect(rect.shrink(1.0))
                        })
                }
                [(name, rect, true)] => control_is_painted(d, name, *rect),
                _ => false,
            };
            d.check(
                "Keyboard focus reveals the populated gain control's actual paint and complete hit target",
                visible && prepared(d)?.content == content && d.revision() == revision,
                json!({"direction":direction,"step":step,"viewport":[width,height],"focused_control":name,"fully_painted":true,"proposal_unchanged":true}),
                control_diagnostic(d, name),
            )?;
            visited.push(name.to_owned());
            if name == DRAFT_FOCUS {
                completed = true;
                break;
            }
        }
        let required = [
            TRIM,
            "In · owner frames",
            "Out · exclusive",
            "Update range",
            "Time · owner frames",
            "Value · dB",
            "Update key",
            "Mute In · frames",
            "Mute Out · exclusive",
            "Update mute range",
            CANCEL,
        ];
        d.check(
            "A complete keyboard circuit reaches populated envelope and mute fields in both directions",
            completed && required.iter().all(|label| visited.iter().any(|visited| visited.as_str() == *label))
                && prepared(d)?.content == content && d.revision() == revision,
            json!({"direction":direction,"viewport":[width,height],"returned_to_heading":true,"required":required,"maximum_steps":60}),
            json!({"completed":completed,"visited":visited}),
        )?;
    }
    Ok(())
}

fn frame_graph(d: &mut Driver<'_>) -> Result<(), String> {
    wheel_controls(d, 2_048.0)?;
    // The trim label uses the one scroller's full clip. The graph's own painter
    // intersects that clip with its allocation, so use the outer clip to place
    // the entire measured graph rather than one currently visible curve part.
    let clip = d
        .harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| {
            let egui::Shape::Text(text) = &clipped.shape else {
                return None;
            };
            (text.galley.text() == TRIM
                && clipped.clip_rect.contains_rect(text.visual_bounding_rect()))
            .then_some(clipped.clip_rect)
        })
        .ok_or("Cannot find the painted gain scroller after returning to its top")?;
    let viewport = d.harness.ctx.content_rect();
    for attempt in 0..=4 {
        let graph = control_rect(d, GRAPH).ok_or("Gain graph has no accessible allocation")?;
        if clip.contains_rect(graph) && viewport.contains_rect(graph) {
            let axes = d
                .harness
                .output()
                .shapes
                .iter()
                .filter_map(|clipped| {
                    let egui::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    let label = text.galley.text();
                    let bounds = text.visual_bounding_rect();
                    (label == "0 f"
                        || label
                            .strip_suffix(" owner frames")
                            .is_some_and(|value| value.parse::<u64>().is_ok()))
                    .then(|| (label.to_owned(), bounds, clipped.clip_rect))
                })
                .collect::<Vec<_>>();
            return d.check(
                "The complete gain graph and both owner-axis labels fit the real paint clip",
                axes.len() == 2 && axes.iter().all(|(_, bounds, axis_clip)| {
                    graph.contains_rect(*bounds) && axis_clip.contains_rect(*bounds) && viewport.contains_rect(*bounds)
                }),
                json!({"complete_graph":true,"owner_axis_labels":2}),
                json!({"graph_height":graph.height(),"scroll_height":clip.height(),"axis_labels":axes.iter().map(|(label,_,_)|label.as_str()).collect::<Vec<_>>()}),
            );
        }
        if graph.height() > clip.height() || graph.width() > clip.width() {
            return Err(format!(
                "Gain graph {}×{} cannot fit its {}×{} paint clip",
                graph.width(),
                graph.height(),
                clip.width(),
                clip.height()
            ));
        }
        if attempt < 4 {
            wheel_controls(d, clip.center().y - graph.center().y)?;
        }
    }
    Err(format!(
        "Gain graph did not fit after bounded measured scrolling: {}",
        control_diagnostic(d, GRAPH)
    ))
}

fn toolbar_visible(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    for label in ["Before", "Draft", PLAY, "Restart loop", APPLY, CANCEL] {
        d.check(
            "Gain comparison and commit actions stay fully painted outside the scroller",
            painted_control(d, label).is_some(),
            json!({"label":label,"viewport":[width,height]}),
            json!(scenarios::text_paint_visibility(d, label)),
        )?;
    }
    Ok(())
}

fn gain_heading_left_aligned(d: &mut Driver<'_>) -> Result<(), String> {
    let heading = d.rect(DRAFT_FOCUS)?;
    let painted = d
        .harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| {
            let egui::Shape::Text(text) = &clipped.shape else {
                return None;
            };
            text.galley
                .text()
                .starts_with("Gain · ")
                .then(|| text.visual_bounding_rect())
        })
        .collect::<Vec<_>>();
    d.check(
        "The gain owner heading paints against the panel's left edge",
        matches!(painted.as_slice(), [rect] if (rect.left() - heading.left()).abs() <= 2.0),
        json!({"heading_left":heading.left(),"maximum_text_inset":2.0}),
        json!(painted.iter().map(|rect| rect.left()).collect::<Vec<_>>()),
    )
}

fn gain_viewer_visible(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Gain draft has no displayed picture")?;
    // A retained picture can be noninteractive while modal controls own input.
    let viewers = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| {
            let access = node.accesskit_node();
            access.label().as_deref() == Some(label.as_str())
                && !access.is_hidden()
                && access.bounding_box().is_some()
                && node.rect().is_positive()
        })
        .map(|node| node.rect())
        .collect::<Vec<_>>();
    let [viewer] = viewers.as_slice() else {
        return Err(format!(
            "Expected one retained gain viewer, found {}",
            viewers.len()
        ));
    };
    let texture = d
        .app()
        .target
        .as_ref()
        .ok_or("Gain draft has no picture texture")?
        .texture;
    let viewport = d.harness.ctx.content_rect();
    let pixels_per_point = d.harness.ctx.pixels_per_point();
    let viewer_in_viewport = picture_contains_rect(viewport, *viewer, pixels_per_point);
    let picture = d.harness.output().shapes.iter().filter_map(|clipped| {
        let egui::Shape::Mesh(mesh) = &clipped.shape else {
            return None;
        };
        if mesh.texture_id != texture {
            return None;
        }
        let bounds = mesh.calc_bounds();
        let inside_clip = picture_contains_rect(clipped.clip_rect, bounds, pixels_per_point);
        let inside_viewer = picture_contains_rect(*viewer, bounds, pixels_per_point);
        let inside_viewport = picture_contains_rect(viewport, bounds, pixels_per_point);
        Some(json!({
            "bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "clip":[clipped.clip_rect.min.x,clipped.clip_rect.min.y,clipped.clip_rect.max.x,clipped.clip_rect.max.y],
            "inside_clip":inside_clip,"inside_viewer":inside_viewer,"inside_viewport":inside_viewport,
            "visible":inside_clip && inside_viewer && inside_viewport,
        }))
    }).collect::<Vec<_>>();
    let painted = picture.iter().any(|mesh| mesh["visible"] == true);
    let minimum = if width <= 960.0 { 140.0 } else { 230.0 };
    d.check(
        "Gain editing retains a useful painted picture above its controls",
        painted && viewer_in_viewport && viewer.height() >= minimum,
        json!({"minimum_viewer_height":minimum,"viewport":[width,height],"painted":true}),
        json!({"viewer_height":viewer.height(),"painted":painted,"viewport":[width,height],
            "viewer_in_viewport":viewer_in_viewport,"pixels_per_point":pixels_per_point,
            "containment_tolerance_pixels":PICTURE_TOLERANCE_PIXELS,"picture":picture}),
    )
}

fn audition(
    d: &mut Driver<'_>,
    proposed: &Snapshot,
    editor: &Value,
    revision: &str,
) -> Result<(), String> {
    d.click("Before")?;
    d.check(
        "Choosing Before while paused does not start playback",
        d.app().transport.is_none(),
        json!("paused"),
        d.snapshot(),
    )?;
    d.click(PLAY)?;
    let run = d.app().transport.as_ref().ok_or("Before did not start")?;
    let window = *run.window();
    let expected_content = AudioSample(window.start().0 + 137);
    let delivered = AudioSample(window.end().0 + 137);
    d.check(
        "Before uses committed content and the captured looping Sequence window",
        run.content == ContentIdentity::Committed
            && run.revision.as_str() == revision
            && window.looping()
            && d.app()
                .gain
                .as_ref()
                .is_some_and(|draft| draft.window() == window)
            && run.sample == window.start()
            && expected_content < window.end(),
        json!({"content":"Committed","revision":revision,"looping":true}),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let before_update = delivery(d, Phase::Playing, delivered, generation)?;
    inject(
        d,
        before_update.clone(),
        "Deliver Before after one complete loop plus 137 samples",
    )?;
    d.check(
        "The heard comparison clock wraps exactly while editor targeting stays fixed",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.content_sample() == Ok(expected_content))
            && editor_state(d) == *editor
            && d.revision() == revision,
        json!({"heard_sample":expected_content.0,"editor":editor,"revision":revision}),
        d.snapshot(),
    )?;
    same_comparison_tab(d, "Before")?;
    d.click("Draft")?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Draft comparison did not start")?;
    d.check(
        "Draft switches content at the exact heard sample in the identical window",
        run.content == proposed.content
            && &run.revision == proposed.document.revision_id()
            && run.window() == &window
            && run.sample == expected_content
            && editor_state(d) == *editor,
        json!({"heard_sample":expected_content.0,"window":[window.start().0,window.end().0],"editor":editor}),
        d.snapshot(),
    )?;
    let mut stale = before_update;
    stale.phase = Phase::Failed;
    stale.error = Some("obsolete Before failure".into());
    inject(
        d,
        stale,
        "Deliver an obsolete Before failure after switching to Draft",
    )?;
    d.check(
        "An old Before failure cannot stop or relabel the active Draft",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.content == proposed.content)
            && d.app().error.is_none(),
        json!("current Draft remains preparing"),
        d.snapshot(),
    )?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let next = AudioSample(expected_content.0 + 89);
    let update = delivery(d, Phase::Playing, next, generation)?;
    let mut foreign = update.clone();
    if let ContentIdentity::Proposed { change, .. } = &mut foreign.content {
        *change = change
            .checked_add(1)
            .ok_or("Replay proposal identity overflow")?;
    }
    inject(
        d,
        foreign,
        "Deliver a different change identity with an otherwise matching ticket",
    )?;
    d.check(
        "A different gain change cannot advance the current proposed transport",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == expected_content && run.generation.is_none()),
        json!({"sample":expected_content.0,"generation":null}),
        d.snapshot(),
    )?;
    inject(d, update, "Deliver the admitted Draft output generation")?;
    same_comparison_tab(d, "Draft")?;
    field(d, TRIM, "unapplied gain")?;
    d.check(
        "Unapplied trim text keeps Pause available and disables Restart loop",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == Phase::Playing)
            && d.rect(PAUSE).is_ok()
            && d.rect("Restart loop").is_err()
            && d.rect(APPLY).is_err()
            && d.revision() == revision,
        json!({"playing":true,"pause_enabled":true,"restart_enabled":false,"revision":revision}),
        d.snapshot(),
    )?;
    d.click(PAUSE)?;
    d.check(
        "Pause remains effective while trim text is invalid and creates no history",
        d.app().transport.is_none()
            && d.app()
                .gain
                .as_ref()
                .is_some_and(|draft| draft.position == next)
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"paused":true,"heard_sample":next.0,"revision":revision}),
        d.snapshot(),
    )?;
    click_control(d, "Reset fields")?;
    d.step("Paint the restored gain field readiness", true)?;
    d.check(
        "Reset fields restores the prepared recipe and comparison readiness",
        prepared(d)?.content == proposed.content
            && d.rect(PLAY).is_ok()
            && d.rect("Restart loop").is_ok()
            && d.rect(APPLY).is_ok()
            && d.revision() == revision,
        json!({"play_enabled":true,"restart_enabled":true,"apply_enabled":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.click("Before")?;
    d.click("Draft")?;
    d.check(
        "Paused Before and Draft switches retain the exact heard position without restarting",
        d.app().transport.is_none()
            && d.app()
                .gain
                .as_ref()
                .is_some_and(|draft| draft.position == next)
            && editor_state(d) == *editor,
        json!({"paused":true,"heard_sample":next.0,"editor":editor}),
        d.snapshot(),
    )?;
    d.click(DRAFT_FOCUS)?;
    d.key(Key::Space)?;
    d.check(
        "Modal Space resumes the selected Draft at its retained sample",
        d.app().transport.as_ref().is_some_and(|run| {
            run.sample == next && run.content == proposed.content && run.window() == &window
        }),
        json!({"sample":next.0,"content":"Proposed"}),
        d.snapshot(),
    )?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let failed_at = AudioSample(next.0 + 41);
    let mut failure = delivery(d, Phase::Failed, failed_at, generation)?;
    failure.error = Some("gain replay device fault".into());
    inject(d, failure, "Deliver a current Draft device failure")?;
    d.check(
        "A current fault pauses comparison, revokes resume and preserves the editable proposal",
        d.app().transport.is_none()
            && d.app().resume.is_none()
            && d.app().gain.as_ref().is_some_and(|draft| {
                draft.position == failed_at && draft.prepared_snapshot().is_some()
            })
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("gain replay device fault"))
            && editor_state(d) == *editor
            && d.revision() == revision,
        json!({"paused":true,"resume":null,"heard_sample":failed_at.0,"revision":revision}),
        d.snapshot(),
    )?;
    d.click("Before")?;
    d.check(
        "A paused comparison choice cannot silently recover from a device fault",
        d.app().transport.is_none(),
        json!("paused"),
        d.snapshot(),
    )?;
    Ok(())
}

fn same_comparison_tab(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No active comparison to reselect")?;
    let before = (
        run.ticket,
        run.phase,
        *run.window(),
        run.sample,
        run.generation,
        run.content.clone(),
    );
    d.click(label)?;
    let after = d.app().transport.as_ref().map(|run| {
        (
            run.ticket,
            run.phase,
            *run.window(),
            run.sample,
            run.generation,
            run.content.clone(),
        )
    });
    d.check(
        "Reselecting the active comparison tab leaves its running generation and exact clock intact",
        before.1 == Phase::Playing && after.as_ref() == Some(&before),
        json!({"tab":label,"ticket":before.0,"phase":"Playing","sample":before.3.0,"window":[before.2.start().0,before.2.end().0]}),
        json!({"ticket":after.as_ref().map(|value|value.0),"playback":d.snapshot()["playback"]}),
    )
}

fn delivery(
    d: &Driver<'_>,
    phase: Phase,
    sample: AudioSample,
    generation: deadpan_output::Generation,
) -> Result<Update, String> {
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No gain audition to deliver")?;
    Ok(Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase,
        sample: Some(sample),
        generation: Some(generation),
        error: None,
    })
}

fn inject(d: &mut Driver<'_>, update: Update, label: &str) -> Result<(), String> {
    d.app_mut().feedback.playback_updates.push_back(update);
    d.step(label, true)
}

fn native_text_cancel(
    d: &mut Driver<'_>,
    nodes: &std::collections::BTreeMap<NodeId, deadpan_core::BeatNode>,
) -> Result<(), String> {
    let revision = d.revision();
    let editor = editor_state(d);
    d.settled()?;
    open(d)?;
    let entry_picture = d.app().presentation.diagnostic_snapshot();
    d.app_mut().feedback.hold_preview = true;
    let close = d.key(Key::Escape);
    let retained = d.app().presentation.has_displayed();
    d.app_mut().feedback.hold_preview = false;
    close?;
    d.check(
        "Opening then cancelling an unauditioned gain draft retains the displayed picture",
        retained
            && d.app().gain.is_none()
            && d.app().presentation.diagnostic_snapshot()["displayed"]
                == entry_picture["displayed"]
            && d.revision() == revision
            && editor_state(d) == editor,
        json!({"displayed_picture_retained":true,"draft":false,"revision":revision}),
        d.snapshot(),
    )?;
    open(d)?;
    field(d, TRIM, "dd + j y")?;
    d.chord(&[Key::D, Key::D, Key::Plus, Key::J])?;
    d.key(Key::Enter)?;
    d.check(
        "Native field letters, gain keys and Enter cannot execute editor commands or Apply",
        d.app().gain.is_some()
            && d.app().transport.is_none()
            && d.revision() == revision
            && document(d)?.nodes() == nodes
            && editor_state(d) == editor,
        json!({"draft":true,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    field(d, TRIM, "-3")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Native gain composition owns its Enter key",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "-3".into(),
                active_range_chars: Some(0..2),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME Enter keeps the draft open without history",
        d.app().gain.is_some() && d.revision() == revision,
        json!({"draft":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.events(
        "Finish gain field composition",
        vec![egui::Event::Ime(egui::ImeEvent::Commit("-3".into()))],
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Escape from native gain text discards the draft and restores entry targeting",
        d.app().gain.is_none()
            && d.revision() == revision
            && document(d)?.nodes() == nodes
            && editor_state(d) == editor,
        json!({"draft":false,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    open(d)?;
    d.click(CANCEL)?;
    d.check(
        "Closing an unchanged draft also creates no history",
        d.app().gain.is_none() && d.revision() == revision,
        json!(revision),
        d.snapshot(),
    )?;
    open(d)?;
    tab_containment(d)?;
    d.click(DRAFT_FOCUS)?;
    d.key(Key::Enter)?;
    d.check(
        "Enter from the draft heading closes unchanged gain without history",
        d.app().gain.is_none()
            && d.revision() == revision
            && document(d)?.nodes() == nodes
            && editor_state(d) == editor,
        json!({"draft":false,"revision":revision,"editor":editor}),
        d.snapshot(),
    )
}

fn tab_containment(d: &mut Driver<'_>) -> Result<(), String> {
    d.click(DRAFT_FOCUS)?;
    let top = d.rect(DRAFT_FOCUS)?.top();
    let revision = d.revision();
    let content = prepared(d)?.content.clone();
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.check(
        "Shift Tab wraps directly from the gain heading to Cancel",
        control_focused(d, CANCEL) && d.app().gain.is_some(),
        json!(CANCEL),
        control_diagnostic(d, CANCEL),
    )?;
    d.key(Key::Tab)?;
    d.check(
        "Tab wraps directly from Cancel to the gain heading without losing focus",
        control_focused(d, DRAFT_FOCUS) && d.app().gain.is_some(),
        json!(DRAFT_FOCUS),
        control_diagnostic(d, DRAFT_FOCUS),
    )?;
    // More than one complete circuit in both directions. Boundary wrapping
    // must retain native traversal through the enabled middle controls.
    for (direction, modifiers) in [("Tab", Modifiers::NONE), ("Shift Tab", Modifiers::SHIFT)] {
        d.click(DRAFT_FOCUS)?;
        let mut focused = Vec::new();
        let mut contained = true;
        for _ in 0..20 {
            d.key_modified(Key::Tab, modifiers)?;
            let current = d
                .harness
                .root()
                .children_recursive()
                .filter(|node| {
                    let access = node.accesskit_node();
                    access.is_focused() && !access.is_disabled() && !access.is_hidden()
                })
                .map(|node| (node.accesskit_node().label(), node.rect()))
                .collect::<Vec<_>>();
            contained &= !current.is_empty()
                && current.iter().all(|(_, rect)| {
                    rect.is_positive()
                        && rect.center().y >= top
                        && d.harness.ctx.content_rect().contains(rect.center())
                });
            focused.push(json!(
                current.iter().map(|(label, _)| label).collect::<Vec<_>>()
            ));
        }
        d.check(
            "Forward and reverse Tab traversal stay inside the gain draft",
            contained && d.app().gain.is_some() && prepared(d)?.content == content && d.revision() == revision,
            json!({"direction":direction,"focus_contained":true,"proposal_unchanged":true,"revision":revision}),
            json!(focused),
        )?;
    }
    Ok(())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Gain replay lost its workspace".into())
}

fn recipe(d: &Driver<'_>, owner: &NodeId) -> Result<AudioTreatments, String> {
    document(d)?
        .nodes()
        .get(owner)
        .map(|node| node.audio_treatments.clone())
        .ok_or_else(|| "Captured gain owner is absent".into())
}

fn trim(recipe: &AudioTreatments) -> i32 {
    recipe
        .clip_gain()
        .map_or(GainDb::UNITY, |clip| clip.trim())
        .millidecibels()
}

fn prepared<'a>(d: &'a Driver<'_>) -> Result<&'a Arc<Snapshot>, String> {
    d.app()
        .gain
        .as_ref()
        .and_then(|draft| draft.prepared_snapshot())
        .ok_or_else(|| "Latest gain proposal is not prepared".into())
}

fn wait_prepared(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Latest captured gain proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .gain
                .as_ref()
                .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })
}

fn open(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("gain")?;
    wait_prepared(d)
}

fn editor_state(d: &Driver<'_>) -> Value {
    let snapshot = d.snapshot();
    json!({"source_cursor":snapshot["source_cursor"],"sequence_cursor":snapshot["sequence_cursor"],"selected_beat":snapshot["selected_beat"],"sequence_scope":snapshot["sequence_scope"],"duration":snapshot["duration"]})
}

fn field(d: &mut Driver<'_>, label: &str, text: &str) -> Result<(), String> {
    reveal_control(d, label)?;
    let rect = field_rect(d, label)?;
    d.click_at(label, rect.center())?;
    replace_focused_text(d, text)
}

fn field_rect(d: &Driver<'_>, label: &str) -> Result<egui::Rect, String> {
    let fields = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| {
            let access = node.accesskit_node();
            access.role() == egui::accesskit::Role::TextInput
                && access.label().as_deref() == Some(label)
                && !access.is_disabled()
                && !access.is_hidden()
        })
        .map(|node| node.rect())
        .collect::<Vec<_>>();
    let [rect] = fields.as_slice() else {
        return Err(format!(
            "Expected one native gain field {label:?}, found {}",
            fields.len()
        ));
    };
    Ok(*rect)
}

fn replace_focused_text(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Edit buffered native gain field",
        vec![egui::Event::Text(text.into())],
    )
}

fn field_value(d: &Driver<'_>, label: &str) -> Result<String, String> {
    d.harness
        .root()
        .children_recursive()
        .find_map(|node| {
            let access = node.accesskit_node();
            if access.role() == egui::accesskit::Role::TextInput
                && access.label().as_deref() == Some(label)
            {
                access.value()
            } else {
                None
            }
        })
        .ok_or_else(|| format!("Native gain field {label:?} has no value"))
}

fn expect_focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    d.check(
        "Native Tab focuses the next exact gain field or row action",
        control_focused(d, label),
        json!(label),
        control_diagnostic(d, label),
    )
}

fn control_focused(d: &Driver<'_>, label: &str) -> bool {
    d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.is_focused()
            && access.label().as_deref() == Some(label)
            && !access.is_disabled()
            && !access.is_hidden()
    })
}

fn click_control(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    reveal_control(d, label)?;
    let rect = painted_control(d, label)
        .ok_or_else(|| format!("Gain control {label:?} lost its painted hit target"))?;
    d.click_at(label, rect.center())
}

fn click_popup_item(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    // egui's first popup sizing frame exposes disabled provisional controls.
    // Wait only for actual enabled, painted geometry; never click that frame's
    // tentative rectangle or scroll the draft underneath an open popup.
    for attempt in 0..=4 {
        let candidates = d
            .harness
            .root()
            .children_recursive()
            .filter(|node| {
                let access = node.accesskit_node();
                matches!(
                    access.role(),
                    egui::accesskit::Role::Button
                        | egui::accesskit::Role::MenuItem
                        | egui::accesskit::Role::ListBoxOption
                ) && (access.label().as_deref() == Some(label)
                    || access.value().as_deref() == Some(label))
                    && !access.is_disabled()
                    && !access.is_hidden()
                    && access.bounding_box().is_some()
                    && node.rect().is_positive()
            })
            .map(|node| node.rect())
            .collect::<Vec<_>>();
        if let [rect] = candidates.as_slice()
            && control_is_painted(d, label, *rect)
        {
            return d.click_at(label, rect.center());
        }
        if attempt < 4 {
            d.step("Settle native gain interpolation popup placement", false)?;
        }
    }
    Err(format!(
        "Gain popup item {label:?} did not become enabled and fully painted: {}",
        control_diagnostic(d, label)
    ))
}

fn control_rect(d: &Driver<'_>, label: &str) -> Option<egui::Rect> {
    if let Ok(rect) = field_rect(d, label) {
        return Some(rect);
    }
    let matches = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| {
            let access = node.accesskit_node();
            (access.label().as_deref() == Some(label)
                || (matches!(
                    access.role(),
                    egui::accesskit::Role::ComboBox
                        | egui::accesskit::Role::Button
                        | egui::accesskit::Role::MenuItem
                        | egui::accesskit::Role::ListBoxOption
                ) && access.value().as_deref() == Some(label)))
                && !access.is_disabled()
                && !access.is_hidden()
                && access.bounding_box().is_some()
                && node.rect().is_positive()
        })
        .map(|node| node.rect())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [rect] => Some(*rect),
        _ => None,
    }
}

fn painted_control(d: &Driver<'_>, label: &str) -> Option<egui::Rect> {
    let rect = control_rect(d, label)?;
    control_is_painted(d, label, rect).then_some(rect)
}

fn control_is_painted(d: &Driver<'_>, label: &str, rect: egui::Rect) -> bool {
    let viewport = d.harness.ctx.content_rect();
    if !viewport.contains_rect(rect) {
        return false;
    }
    d.harness.output().shapes.iter().any(|clipped| {
        let egui::Shape::Text(text) = &clipped.shape else {
            return false;
        };
        text.galley.text() == label
            && clipped.clip_rect.contains_rect(text.visual_bounding_rect())
            && viewport.contains_rect(text.visual_bounding_rect())
            && clipped.clip_rect.contains_rect(rect.shrink(1.0))
    })
}

fn reveal_control(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    if painted_control(d, label).is_some() {
        return Ok(());
    }
    // Reset the single gain scroller through real pointer input, then inspect
    // overlapping bounded increments. Neither access-tree presence nor a
    // viewport-only rectangle proves that a clipped control can be clicked.
    wheel_controls(d, 2_048.0)?;
    for _ in 0..12 {
        if painted_control(d, label).is_some() {
            return Ok(());
        }
        wheel_controls(d, -48.0)?;
    }
    if painted_control(d, label).is_some() {
        return Ok(());
    }
    Err(format!(
        "Gain control {label:?} was not fully painted after bounded scrolling: {}",
        control_diagnostic(d, label)
    ))
}

fn control_diagnostic(d: &Driver<'_>, label: &str) -> Value {
    let candidates = d.harness.root().children_recursive().filter(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some(label) || access.value().as_deref() == Some(label)
            || access.is_focused()
    }).take(6).map(|node| {
        let access = node.accesskit_node();
        let rect = node.rect();
        json!({"role":format!("{:?}",access.role()),"label":access.label(),
            "value":access.value().map(|value|value.chars().take(80).collect::<String>()),
            "focused":access.is_focused(),"disabled":access.is_disabled(),"hidden":access.is_hidden(),
            "rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y]})
    }).collect::<Vec<_>>();
    json!({"control":label,"candidates":candidates,"paint":scenarios::text_paint_visibility(d,label).into_iter().take(3).collect::<Vec<_>>()})
}

fn wheel_controls(d: &mut Driver<'_>, delta: f32) -> Result<(), String> {
    let point = d.rect(DRAFT_FOCUS)?.center() + egui::vec2(0.0, 70.0);
    d.events(
        "Scroll within the gain draft controls",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Gain control scroll settles", false)?;
    }
    Ok(())
}
