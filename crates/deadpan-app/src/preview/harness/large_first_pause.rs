//! One independent first insertion into the pristine 10k-Hold fixture.
//! Diagnostic offscreen app acceptance/composition, never display scanout.

use deadpan_core::{HoldAudio, HoldVideo, ProjectDocument};

use super::*;

const BEATS: usize = 10_000;
const AT: u64 = 17_933;
const INDEX: usize = 1_494;

/// Generator commits are setup, not measured edits. A changed/undone fixture
/// has a different head and is not a fresh first-pause trial.
pub(super) fn preflight(package: &Path) -> Result<Value, String> {
    let store = deadpan_store::ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| format!("Large first-pause read-only preflight: {error}"))?;
    let document = store.snapshot().map_err(|error| error.to_string())?;
    let history = store.validate_report(true).map_err(|e| e.to_string())?;
    let availability = store.history_availability().map_err(|e| e.to_string())?;
    if !pristine(&document) || availability != (true, false) || history.revisions != 3 {
        return Err("large-first-pause requires a fresh perf make-large --beats 10000 package at large-ungrouped, with no subsequent edits".into());
    }
    use sha2::Digest as _;
    let document_json = document.to_json().map_err(|error| error.to_string())?;
    let document_sha256 = sha2::Sha256::digest(document_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(json!({
        "document_sha256":document_sha256,"revision":document.revision_id(),
        "head_revision":store.head_revision().map_err(|e| e.to_string())?,
        "nodes":document.nodes().len(),"root_beats":BEATS,"frames":120000,
        "presentation_basis":document.presentation_basis(),
        "audio_bindings_empty":document.audio_bindings().is_empty(),
        "history":history,"can_undo":availability.0,"can_redo":availability.1,
        "setup_commits":2,"prior_user_edits":0,
    }))
}

fn fixture_basis() -> deadpan_core::PresentationBasis {
    deadpan_core::PresentationBasis {
        width: 1920,
        height: 1080,
        frame_rate: deadpan_core::FrameRate::new(24, 1).expect("constant positive frame rate"),
        color_policy: deadpan_core::ColorPolicy::SdrRec709,
    }
}

fn pristine(document: &ProjectDocument) -> bool {
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        return false;
    };
    document.project_id().as_str() == "perf-large-10000"
        && document.revision_id().as_str() == "large-ungrouped"
        && document.root().as_str() == "large-root"
        && document.presentation_basis() == &fixture_basis()
        && document.nodes().len() == BEATS + 1
        && document.assets().is_empty()
        && document.audio_bindings().is_empty()
        && children.len() == BEATS
        && children.iter().enumerate().all(|(index, id)| {
            id.as_str() == format!("hold-{index:06}")
                && document.nodes().get(id)
                    == Some(&deadpan_core::BeatNode::hold(
                        format!("Scale fixture {:06}", index + 1),
                        deadpan_core::HoldRecipe {
                            picture_context: None,
                            duration: deadpan_core::FrameDuration::new(12)
                                .expect("constant positive duration"),
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ))
        })
}

fn background_hold(node: Option<&deadpan_core::BeatNode>, frames: i64) -> bool {
    node.is_some_and(|node| {
        matches!(&node.kind, NodeKind::Hold { recipe }
        if recipe.duration.frames() == frames
            && recipe.video == HoldVideo::Background
            && recipe.audio == HoldAudio::Silence)
    })
}

pub(super) fn run(d: &mut Driver<'_>, baseline: &Value) -> Result<(), String> {
    d.report.skipped.push(
        "One fresh first-pause trial (n=1), diagnostic only. Scripted production input to durable commit and new-revision Background acceptance through offscreen GPU composition. No percentile, 100 ms budget PASS, physical display/scanout, source decode or PCM claim. Aggregate 40 fresh independent invocations under PERFORMANCE.md before a UI target claim.".into(),
    );
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.key(egui::Key::Escape)?;
    d.chord(&[egui::Key::G, egui::Key::G])?;
    for digit in AT.to_string().chars() {
        d.key(egui::Key::from_name(&digit.to_string()).ok_or("Invalid cursor digit")?)?;
    }
    d.key(egui::Key::L)?;
    // Drain initial Background delivery/composition before arming the trial.
    d.settled()?;
    let app = d.app();
    let workspace = app
        .workspace
        .as_ref()
        .ok_or("No large first-pause workspace")?;
    let before = d.revision();
    let roots = root_children(workspace).to_vec();
    let picture_before = app.presentation.diagnostic_snapshot();
    let containing = app.beat_rows.get(INDEX);
    d.check(
        "Pristine 10,001-node / 120,000-frame baseline at ordinary root Hold index 1494 offset 5, without prior user or replay edits",
        pristine(&workspace.document)
            && app.sequence_length() == 120_000
            && app.sequence_cursor == AT
            && app.view == View::Sequence
            && app.pane == Pane::Sequence
            && app.sequence_scope.groups().is_empty()
            && app.scoped_target()?.is_none()
            && !app.edit_range.active
            && !app.moment.active
            && app.selected_edit_range().is_none()
            && app.edit_selection() == navigation::EditSelection::None
            && containing.is_some_and(|row| row.id == roots[INDEX] && row.start == AT - 5)
            && app.presentation.picture().is_some_and(|picture| picture.frame.is_none()),
        json!({"nodes":10001,"frames":120000,"revision":"large-ungrouped","cursor":AT,"root_hold":1494,"offset":5,"selection":"none","picture":"accepted Background"}),
        d.snapshot(),
    )?;

    if let Some(step) = d.report.steps.last_mut() {
        step.semantic["large_first_pause_baseline"] = baseline.clone();
    }
    let trace_start = d.report.steps.len();
    let commit_start = sample_count(d, "input_to_commit_ms");
    let picture_start = sample_count(d, "input_to_picture_complete_ms");
    d.command("hold 12f")?;
    d.changed(&before)?;

    let commit_ms = exactly_one_sample(d, "input_to_commit_ms", commit_start)?;
    let composition_ms = exactly_one_sample(d, "input_to_picture_complete_ms", picture_start)?;
    let app = d.app();
    let workspace = app
        .workspace
        .as_ref()
        .ok_or("Insertion lost the workspace")?;
    let after_roots = root_children(workspace);
    let selected = app.selected_beat.as_ref();
    let selected_node = selected.and_then(|id| workspace.document.nodes().get(id));
    let picture = app.presentation.diagnostic_snapshot();
    let ticket = &picture["displayed"]["ticket"];
    let events = d.report.steps[trace_start..]
        .iter()
        .flat_map(|step| step.semantic["stages"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    let count = |stage| {
        events
            .iter()
            .filter(|event| event["stage"] == stage)
            .count()
    };
    let admitted = count("command_admitted");
    let committed = count("command_committed");
    let rejected = count("command_rejected");
    let ordinary_suffix_preserved = after_roots.len() == BEATS + 2
        && after_roots[..INDEX] == roots[..INDEX]
        && after_roots[INDEX + 3..] == roots[INDEX + 1..];
    let durations = workspace.document.durations().map_err(|e| e.to_string())?;
    let split_and_inserted = after_roots.get(INDEX..INDEX + 3).is_some_and(|split| {
        // Split retains the full original Hold under partition owners.
        durations
            .get(&split[0])
            .is_some_and(|value| value.frames() == 5)
            && selected == Some(&split[1])
            && background_hold(workspace.document.nodes().get(&split[1]), 12)
            && durations
                .get(&split[2])
                .is_some_and(|value| value.frames() == 7)
    });
    let background_seams = [AT - 1, AT, AT + 11, AT + 12].into_iter().all(|frame| {
        i64::try_from(frame)
            .ok()
            .and_then(|frame| workspace.plan.picture(ProjectFrame(frame)).ok())
            .is_some_and(|sample| matches!(sample.picture, deadpan_plan::Picture::Background))
    });
    d.check(
        "Exactly one admitted/committed insertion creates a selected 12-frame Background/Silence Hold and accepts its new revision offscreen",
        admitted == 1 && committed == 1 && rejected == 0
            && d.revision() != before
            && app.sequence_length() == 120_012
            && ordinary_suffix_preserved && split_and_inserted && background_seams
            && background_hold(selected_node, 12)
            && app.presentation.picture().is_some_and(|value| value.frame.is_none())
            && picture["displayed"] != picture_before["displayed"]
            && !ticket.is_null()
            && ticket != &picture_before["displayed"]["ticket"]
            && completed_ticket(&events, ticket),
        json!({"admitted":1,"committed":1,"rejected":0,"frames":120012,"split_frames":[5,12,7],"new_ticket":true,"new_revision":true,"completion":"decoded/submitted/composed same-ticket Background; offscreen acceptance"}),
        json!({"admitted":admitted,"committed":committed,"rejected":rejected,"revision":d.revision(),"frames":app.sequence_length(),"picture":picture,"input_to_commit_ms":commit_ms,"input_to_offscreen_background_composition_ms":composition_ms}),
    )?;
    d.metric(
        "large_first_pause_input_to_commit_ms",
        commit_ms,
        SampleOutcome::Completed,
    );
    d.metric(
        "large_first_pause_input_to_offscreen_background_composition_ms",
        composition_ms,
        SampleOutcome::Completed,
    );
    if let Some(step) = d.report.steps.last_mut() {
        step.semantic["large_first_pause"] = json!({
            "samples":1,"diagnostic_only":true,"percentile":null,"budget_pass":null,
            "command":"hold 12f","cursor_before":AT,"containing_hold_index":INDEX,
            "offset":5,"before_revision":before,"offscreen_acceptance":true,
            "input_to_commit_ms":commit_ms,
            "input_to_offscreen_background_composition_ms":composition_ms,
        });
    }
    Ok(())
}

fn completed_ticket(events: &[&Value], ticket: &Value) -> bool {
    [
        ("picture_requested", "requested"),
        ("picture_received", "decoded"),
        ("picture_submitted", "submitted"),
        ("picture_composed", "completed"),
    ]
    .into_iter()
    .all(|(stage, outcome)| {
        events
            .iter()
            .filter(|event| {
                event["ticket"] == *ticket && event["stage"] == stage && event["outcome"] == outcome
            })
            .count()
            == 1
    })
}

fn sample_count(d: &Driver<'_>, name: &str) -> usize {
    d.report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .map_or(0, |metric| metric.samples.len())
}

fn exactly_one_sample(d: &Driver<'_>, name: &str, start: usize) -> Result<f64, String> {
    let samples = d
        .report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .and_then(|metric| metric.samples.get(start..))
        .ok_or_else(|| format!("First pause has no {name} sample"))?;
    match samples {
        [sample] if sample.outcome == SampleOutcome::Completed && sample.elapsed_ms.is_finite() => {
            Ok(sample.elapsed_ms)
        }
        _ => Err(format!(
            "First pause expected one completed {name} sample; observed {samples:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pristine_rejects_a_valid_tree_with_a_different_presentation_basis() {
        use deadpan_core::{BeatNode, FrameDuration, HoldRecipe, NodeId, ProjectId, RevisionId};
        let root = NodeId::new("large-root").unwrap();
        let document = ProjectDocument::new(
            ProjectId::new("perf-large-10000").unwrap(),
            RevisionId::new("large-ungrouped").unwrap(),
            fixture_basis(),
            root.clone(),
        )
        .unwrap();
        let mut nodes = std::collections::BTreeMap::new();
        let mut children = Vec::new();
        for index in 0..BEATS {
            let id = NodeId::new(format!("hold-{index:06}")).unwrap();
            nodes.insert(
                id.clone(),
                BeatNode::hold(
                    format!("Scale fixture {:06}", index + 1),
                    HoldRecipe {
                        picture_context: None,
                        duration: FrameDuration::new(12).unwrap(),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ),
            );
            children.push(id);
        }
        nodes.insert(root, BeatNode::sequence("Sequence", children));
        let mut wire = serde_json::to_value(document).unwrap();
        wire["nodes"] = serde_json::to_value(nodes).unwrap();
        let baseline = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert!(pristine(&baseline));
        for (field, different) in [
            ("width", json!(1280)),
            ("height", json!(720)),
            ("frame_rate", json!({"numerator":30,"denominator":1})),
        ] {
            let mut changed = wire.clone();
            changed["presentation_basis"][field] = different;
            let changed = ProjectDocument::from_json(&changed.to_string()).unwrap();
            assert!(!pristine(&changed), "accepted different {field}");
        }
    }

    #[test]
    fn retained_or_stale_background_cannot_complete_the_new_ticket() {
        let events = [
            json!({"stage":"picture_requested","outcome":"requested","ticket":"new"}),
            json!({"stage":"picture_received","outcome":"decoded","ticket":"new"}),
            json!({"stage":"picture_submitted","outcome":"submitted","ticket":"new"}),
            json!({"stage":"picture_composed","outcome":"completed","ticket":"new"}),
        ];
        let references = events.iter().collect::<Vec<_>>();
        assert!(completed_ticket(&references, &json!("new")));
        assert!(!completed_ticket(&references, &json!("old")));
        for stage in ["picture_received", "picture_submitted", "picture_composed"] {
            let mut stale = events.clone();
            stale
                .iter_mut()
                .find(|event| event["stage"] == stage)
                .unwrap()["ticket"] = json!("old");
            assert!(!completed_ticket(
                &stale.iter().collect::<Vec<_>>(),
                &json!("new")
            ));
        }
        let mut retained = events;
        retained[2]["outcome"] = json!("retained_or_repeated");
        assert!(!completed_ticket(
            &retained.iter().collect::<Vec<_>>(),
            &json!("new")
        ));
    }
}
