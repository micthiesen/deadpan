//! Two explicit Repeat choices isolate one fixed Preserve input without
//! changing any of the other eight processing plays or the exterior owner.
use super::*;

pub(super) fn repeated_fixed_fixture(prebound: bool) -> ProjectDocument {
    let mut wire =
        serde_json::to_value(super::super::fixture(PitchPolicy::Preserve, 1, false)).unwrap();
    let order = IterationOrder::new(RevisionId::new("nested-fixed-outer").unwrap(), 3).unwrap();
    let mut outer = BeatNode::sequence("Three fixed-stage plays", vec![]);
    outer.kind = NodeKind::Repeat {
        child: id("processor"),
        iterations: order,
        gap: None,
        escalation: None,
    };
    wire["nodes"]["outer"] = serde_json::to_value(outer).unwrap();
    let mut top = BeatNode::sequence("Three nested families", vec![]);
    top.kind = NodeKind::Repeat {
        child: id("outer"),
        iterations: IterationOrder::new(RevisionId::new("nested-fixed-top").unwrap(), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["top"] = serde_json::to_value(top).unwrap();
    wire["nodes"]["exterior"] = serde_json::to_value(voice()).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["top", "exterior"]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    document.validate().unwrap();
    if !prebound {
        return document;
    }
    // Capture clocks only after the actual Repeat and exterior audio owner
    // exist, so each binding is tied to the repeated physical owner it serves.
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("nested-fixed-audio-clocks").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    assert!(
        bindings.bindings().contains_key(&id("processor")),
        "the repeated Retime's actual audio owner receives its captured binding"
    );
    wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    document.validate().unwrap();
    document
}

fn occurrence(document: &ProjectDocument, play: u32, target: &str) -> InstancePath {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id("outer")].kind else {
        unreachable!()
    };
    let NodeKind::Repeat {
        iterations: top_order,
        ..
    } = &document.nodes()[&id("top")].kind
    else {
        unreachable!()
    };
    InstancePath {
        node: id(target),
        repeats: vec![
            RepeatInstance {
                node: id("top"),
                iteration: top_order.at(1).unwrap(),
            },
            RepeatInstance {
                node: id("outer"),
                iteration: iterations.at(play).unwrap(),
            },
        ],
    }
}

fn occurrence_resize(
    document: &ProjectDocument,
    instance: InstancePath,
    frames: i64,
    revision: &str,
) -> deadpan_core::PreparedOccurrenceHoldDuration {
    let length = duration(frames);
    let needs = document
        .occurrence_hold_duration_requirements(&instance, length)
        .unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command: Command::EditOccurrence {
            instance,
            edit: OccurrenceEdit::SetHoldDuration { duration: length },
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| id(&format!("nested-fixed-clone-{revision}-{index}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|index| {
                        MarkId::new(format!("nested-fixed-mark-{revision}-{index}")).unwrap()
                    })
                    .collect(),
            },
        },
    };
    deadpan_core::prepare_occurrence_hold_duration(document, &request).unwrap()
}

pub(super) fn stage_input(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    root_sample: usize,
    expected_instance: &InstancePath,
    chunks: &[u32],
) -> Recorded {
    let sample = SignalSample(i64::try_from(root_sample).unwrap());
    let query = plan
        .audio_signal()
        .query(
            sample..SignalSample(sample.0 + 1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let span = query.spans.into_iter().next().unwrap();
    let stage = match span.content {
        AudioSignalContent::Stage(stage) => stage,
        AudioSignalContent::Bound(bound) => bound.intrinsic_stage().unwrap().expect("processor"),
        other => panic!("expected processing stage at {sample:?}, got {other:?}"),
    };
    assert_eq!(
        &stage.descriptor().instance,
        expected_instance,
        "PointCeil root sample selects the requested physical processor instance"
    );
    super::super::input_record_for_stage(
        plan,
        provider,
        stage,
        ratio(1, 1)..ratio(8, 1),
        0..11212,
        chunks,
    )
}

#[test]
fn nested_fixed_retime_duration_preserves_fractional_input_phase_and_all_other_owners() {
    for prebound in [false, true] {
        let before = repeated_fixed_fixture(prebound);
        let mut provider = mono_original(2).1;
        let before_plan = Arc::new(RenderPlan::compile(&before).unwrap());
        let selected = occurrence(&before, 1, "pause");
        let selected_stage = occurrence(&before, 1, "processor");
        // Frame 16 is exactly 25625.6 root mix samples. PointCeil 25626
        // selects the middle/middle physical processor, independently of its input grid.
        let root_entry = usize::try_from(ratio(16 * 8008, 5).ceil().unwrap()).unwrap();
        assert_eq!(root_entry, 25626);
        let old_input = stage_input(
            &before_plan,
            &mut provider,
            root_entry,
            &selected_stage,
            &[127, 3, 251],
        );
        same_recorded(
            &old_input,
            &super::super::direct_input_reference(&provider, 1, 2),
            0,
            "old fractional fixed-Retime input",
        );
        let old_output = record(&before, &mut provider, false, &[127, 3, 251]);
        let old_total = old_output.samples.len();
        let play1 = ntsc_boundary(16);
        let play2 = ntsc_boundary(20);
        let exterior = ntsc_boundary(36);
        assert_eq!(old_total, ntsc_boundary(40));

        // The selected middle Play needs the exact fresh isolation pool.
        let needs = before
            .occurrence_hold_duration_requirements(&selected, duration(3))
            .unwrap();
        // Outside-in: clone inner Repeat + its five-node Stage subtree (6),
        // then clone that Stage subtree for the selected inner play (5).
        assert_eq!((needs.nodes, needs.marks), (11, 0));
        let grown = occurrence_resize(&before, selected, 3, "nested-fixed-grow");
        assert_eq!(
            grown.fixed_visibility().unwrap().frames,
            FrameRange::new(ProjectFrame(17), ProjectFrame(19)).unwrap()
        );
        assert_eq!(grown.isolation().steps().len(), 2);
        assert_eq!(grown.isolation().steps()[0].nodes().len(), 6);
        assert_eq!(grown.isolation().steps()[1].nodes().len(), 5);
        assert_ne!(grown.presentation.node, id("pause"));
        assert_ne!(grown.target.node, id("pause"));
        assert_eq!(
            grown.transaction.duration_delta, 0,
            "fixed Retime extent is stable"
        );
        assert_eq!(
            grown.document.duration().unwrap().frames(),
            before.duration().unwrap().frames()
        );

        let grown_plan = Arc::new(RenderPlan::compile(&grown.document).unwrap());
        let grown_stage = grown.map_instance(&before, &selected_stage).unwrap();
        let grown_input = stage_input(
            &grown_plan,
            &mut provider,
            root_entry,
            &grown_stage,
            &[127, 3, 251],
        );
        same_recorded(
            &grown_input,
            &super::super::direct_input_reference(&provider, 1, 3),
            0,
            "grown fixed-Retime PointCeil input",
        );
        assert_eq!(grown_input.samples[11211], [0.0; 2]);
        assert!(
            grown_input.suppressed[11211],
            "fractional crop exhaustion remains silent"
        );
        same_recorded(
            &stage_input(
                &grown_plan,
                &mut provider,
                root_entry,
                &grown_stage,
                &[1, 239],
            ),
            &grown_input,
            0,
            "grown fixed-Retime chunk parity",
        );
        let grown_output = record(&grown.document, &mut provider, false, &[127, 3, 251]);
        assert_other_outputs(&old_output, &grown_output);
        for (node, original) in before.nodes() {
            assert_eq!(
                &grown.document.nodes()[node],
                original,
                "retained template {node}"
            );
        }
        assert_eq!(
            grown.transaction.inverse.apply(&grown.document).unwrap(),
            before
        );
        assert_eq!(
            ProjectDocument::from_json(&grown.document.to_json().unwrap()).unwrap(),
            grown.document
        );
        assert_eq!(
            grown_output.samples.len(),
            old_total,
            "fixed stage output stays fixed"
        );
        assert_eq!(grown_output.samples[..play1], old_output.samples[..play1]);
        assert_eq!(
            grown_output.suppressed[..play1],
            old_output.suppressed[..play1]
        );
        assert_ne!(
            grown_output.samples[play1..play2],
            old_output.samples[play1..play2],
            "the selected Play's shifted Hold/Speech boundary changes its actual PCM"
        );
        assert_eq!(
            grown_output.samples[play2..exterior],
            old_output.samples[play2..exterior]
        );
        assert_eq!(
            grown_output.suppressed[play2..exterior],
            old_output.suppressed[play2..exterior]
        );
        assert_eq!(
            grown_output.samples[exterior..],
            old_output.samples[exterior..]
        );
        assert_eq!(
            grown_output.suppressed[exterior..],
            old_output.suppressed[exterior..]
        );
        assert_eq!(
            record(&grown.document, &mut provider, false, &[1, 239]),
            grown_output
        );

        // Once the selected Play owns its clone, the next duration edit uses
        // no identities, but still traverses the same fixed window.
        let already_owned = grown
            .document
            .occurrence_hold_duration_requirements(&grown.presentation, duration(1))
            .unwrap();
        assert_eq!((already_owned.nodes, already_owned.marks), (0, 0));
        let shrunk = occurrence_resize(
            &grown.document,
            grown.presentation.clone(),
            1,
            "nested-fixed-shrink",
        );
        assert_eq!(
            shrunk.fixed_visibility().unwrap().frames,
            FrameRange::new(ProjectFrame(17), ProjectFrame(18)).unwrap()
        );
        assert_eq!(
            shrunk.document.duration().unwrap().frames(),
            before.duration().unwrap().frames()
        );
        let shrunk_plan = Arc::new(RenderPlan::compile(&shrunk.document).unwrap());
        let shrunk_stage = shrunk.map_instance(&grown.document, &grown_stage).unwrap();
        let shrunk_input = stage_input(
            &shrunk_plan,
            &mut provider,
            root_entry,
            &shrunk_stage,
            &[251, 3, 127],
        );
        same_recorded(
            &shrunk_input,
            &super::super::direct_input_reference(&provider, 1, 1),
            0,
            "shrunk fixed-Retime PointCeil input",
        );
        assert_eq!(shrunk_input.samples[11211], [0.0; 2]);
        assert!(shrunk_input.suppressed[11211]);
        let shrunk_output = record(&shrunk.document, &mut provider, false, &[127, 3, 251]);
        assert_other_outputs(&old_output, &shrunk_output);
        assert_eq!(shrunk_output.samples.len(), old_total);
        same_recorded(
            &stage_input(
                &shrunk_plan,
                &mut provider,
                root_entry,
                &shrunk_stage,
                &[1, 239],
            ),
            &shrunk_input,
            0,
            "shrunk fixed-Retime chunk parity",
        );
        assert_eq!(
            record(&shrunk.document, &mut provider, false, &[1, 239]),
            shrunk_output
        );
        assert_eq!(
            shrunk_output.samples[..play1],
            grown_output.samples[..play1]
        );
        assert_eq!(
            shrunk_output.suppressed[..play1],
            grown_output.suppressed[..play1]
        );
        assert_ne!(
            shrunk_output.samples[play1..play2],
            grown_output.samples[play1..play2],
            "the second duration edit changes the selected Play's PCM again"
        );
        assert_eq!(
            shrunk_output.samples[play2..],
            grown_output.samples[play2..]
        );
        assert_eq!(
            shrunk_output.suppressed[play2..],
            grown_output.suppressed[play2..]
        );
    }
}

fn assert_other_outputs(before: &Recorded, after: &Recorded) {
    // This literal equal-layout fixture has nine four-frame processor plays.
    // RoundEven belongs to root output; PointCeil above belongs to stage input.
    for (start, end) in [
        (0, 4),
        (4, 8),
        (8, 12),
        (12, 16),
        (20, 24),
        (24, 28),
        (28, 32),
        (32, 36),
        (36, 40),
    ] {
        let range = ntsc_boundary(start)..ntsc_boundary(end);
        assert_eq!(
            after.samples[range.clone()],
            before.samples[range.clone()],
            "complete unselected output interval {start}..{end}"
        );
        assert_eq!(
            after.suppressed[range.clone()],
            before.suppressed[range],
            "complete unselected suppression interval {start}..{end}"
        );
    }
}
