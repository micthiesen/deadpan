//! Actual decoded PCM for authored Retime changes over retained sampling clocks.
use super::*;

fn edit(
    document: &ProjectDocument,
    command: Command,
    name: &str,
) -> (ProjectDocument, EditTransaction) {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).unwrap(),
        command,
    };
    let transaction = apply(document, &request).unwrap();
    let changed = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), *document);
    (changed, transaction)
}

fn record(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    faded: bool,
    pieces: &[u32],
) -> Vec<[f32; 2]> {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = usize::try_from(renderer.plan().audio_duration().unwrap().0).unwrap();
    let mut output = Vec::new();
    for requested in pieces.iter().copied().cycle() {
        if output.len() == total {
            break;
        }
        let count = requested.min(u32::try_from(total - output.len()).unwrap());
        let start = AudioSample(i64::try_from(output.len()).unwrap());
        let samples = if faded {
            renderer
                .read_edge_faded(provider, start, count, TIMEOUT, &AtomicBool::new(false))
                .unwrap()
                .samples
        } else {
            renderer
                .read(provider, start, count, TIMEOUT, &AtomicBool::new(false))
                .unwrap()
                .samples
        };
        output.extend(samples);
    }
    assert!(output.iter().flatten().any(|sample| sample.abs() > 0.01));
    output
}

fn bound_document() -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document_with_asset(
        rate,
        &["prefix", "rate", "suffix"],
        [
            ("prefix", hold(1)),
            ("source", source(rate, 4, 100..6507)),
            ("rate", retime("source", 3, 0..4, PitchPolicy::Preserve)),
            ("suffix", source(rate, 2, 100..3303)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    // Capture both the physical Preserve output and its intrinsic source input.
    let state = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: document.revision_id().clone(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    // A prior moved output has a fractional resume. The next authored operation
    // must not accidentally reuse this old processing output's resume phase.
    wire["audio_bindings"]["bindings"]["rate"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ratio(5, 2),
        phase: AudioLocalPhase {
            constant: ratio(13, 7),
            terms: vec![],
        },
    })
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn rate_and_pitch_changes_use_fresh_output_but_keep_bound_child_pcm() {
    let before = bound_document();
    let mut provider = FixtureProvider::new();
    let original = record(&before, &mut provider, false, &[113, 17, 256]);
    for (frames, pitch) in [
        (2, PitchPolicy::Preserve),
        (5, PitchPolicy::FollowSpeed),
        (4, PitchPolicy::Preserve),
    ] {
        let (changed, transaction) = edit(
            &before,
            Command::SetRetime {
                node: id("rate"),
                duration: duration(frames),
                pitch,
            },
            "authored-rate",
        );
        // Independent structure oracle: remove the old processing owner, retain
        // its child and all old input clocks, then build a fresh selected stage.
        let mut reference = serde_json::to_value(&before).unwrap();
        let mut node = reference["nodes"]
            .as_object_mut()
            .unwrap()
            .remove("rate")
            .unwrap();
        node["kind"]["duration"] = serde_json::to_value(duration(frames)).unwrap();
        node["kind"]["pitch"] = serde_json::to_value(pitch).unwrap();
        reference["nodes"]["fresh"] = node;
        reference["nodes"]["root"]["kind"]["children"] =
            serde_json::json!(["prefix", "fresh", "suffix"]);
        reference["audio_bindings"]["bindings"]
            .as_object_mut()
            .unwrap()
            .remove("rate");
        let reference = ProjectDocument::from_json(&reference.to_string()).unwrap();
        for faded in [false, true] {
            let expected = record(&reference, &mut provider, faded, &[256]);
            assert_eq!(
                record(&changed, &mut provider, faded, &[7, 131, 23, 256]),
                expected
            );
        }
        let restored = transaction.inverse.apply(&changed).unwrap();
        assert_eq!(
            record(&restored, &mut provider, false, &[31, 256]),
            original
        );
        assert_eq!(
            changed.audio_bindings().bindings()[&id("source")],
            before.audio_bindings().bindings()[&id("source")]
        );
        assert_eq!(
            changed.audio_bindings().bindings()[&id("suffix")],
            before.audio_bindings().bindings()[&id("suffix")]
        );
    }
    let (noop, _) = edit(
        &before,
        Command::SetRetime {
            node: id("rate"),
            duration: duration(3),
            pitch: PitchPolicy::Preserve,
        },
        "same-rate",
    );
    assert_eq!(record(&noop, &mut provider, false, &[1, 251]), original);
}

#[test]
fn wrapping_a_split_partition_retains_child_processing_history_for_both_policies() {
    // One project frame is one mix sample, so the independent reference can
    // state the exact two processing grids without consulting the render plan.
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = document_with_asset(
        rate,
        &["rate"],
        [
            ("source", source(rate, 2048, 512..2560)),
            (
                "rate",
                retime("source", 3072, 0..2048, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let state = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: document.revision_id().clone(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let input: Vec<_> = (512..2560).map(fixture_sample).collect();
    let complete_inner = stretch_reference(&input, 3072, 2, 3);
    // A reset at output 1002 would discard exactly 668 input samples and the
    // stretcher's prior history. Keep this incorrect alternative observable.
    let restarted_inner = stretch_reference(&input[668..], 2070, 2, 3);
    assert_ne!(complete_inner[1002..], restarted_inner);
    let mut provider = FixtureProvider::new();
    assert_eq!(
        record(&before, &mut provider, false, &[113, 256]),
        complete_inner
    );
    let (split, _) = edit(
        &before,
        Command::Split {
            node: id("rate"),
            at: duration(1002),
            identities: SplitIdentities {
                nodes: (0..10).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
        "split-rate",
    );
    let NodeKind::Sequence { children } = &split.nodes()[&id("root")].kind else {
        unreachable!()
    };
    let selected = children[1].clone();
    assert!(matches!(
        split.nodes()[&selected].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    for pitch in [PitchPolicy::Preserve, PitchPolicy::FollowSpeed] {
        let (wrapped, _) = edit(
            &split,
            Command::WrapRetime {
                node: selected.clone(),
                id: id("new-rate"),
                duration: duration(4140),
                pitch,
            },
            "wrapped-fragment",
        );
        assert_eq!(wrapped.nodes()[&selected], split.nodes()[&selected]);
        assert_eq!(wrapped.audio_bindings(), split.audio_bindings());
        // The new wrapper consumes only the 2070-sample suffix, but its input
        // comes from the complete 3072-sample inner processing history. These
        // references use qualified DSP directly, without StageAudio or a plan.
        let (expected_tail, restarted_tail) = match pitch {
            PitchPolicy::Preserve => (
                stretch_reference(&complete_inner[1002..], 4140, 1, 2),
                stretch_reference(&restarted_inner, 4140, 1, 2),
            ),
            PitchPolicy::FollowSpeed => (
                sample_reference(
                    1002..3072,
                    ExactRatio::integer(1002),
                    ratio(1, 2),
                    4140,
                    |at| complete_inner[usize::try_from(at).unwrap()],
                ),
                sample_reference(0..2070, ExactRatio::ZERO, ratio(1, 2), 4140, |at| {
                    restarted_inner[usize::try_from(at).unwrap()]
                }),
            ),
        };
        assert_ne!(expected_tail, restarted_tail, "pitch {pitch:?}");
        let mut expected = complete_inner[..1002].to_vec();
        expected.extend(expected_tail);
        provider.revisions.insert(wrapped.revision_id().clone());
        // Read the changed suffix first through a fresh stage cache. The
        // preceding fragment cannot supply missing preparation history.
        let mut cold = StageAudio::new(Arc::new(RenderPlan::compile(&wrapped).unwrap()));
        let suffix = cold
            .read(
                &mut provider,
                AudioSample(1002),
                193,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(suffix.samples, expected[1002..1195], "pitch {pitch:?}");
        assert_eq!(
            record(&wrapped, &mut provider, false, &[1, 71, 256]),
            expected,
            "pitch {pitch:?}"
        );
        // Fades and query boundaries retain their separate partition-invariance
        // check; they are not used to derive the processing-history reference.
        for faded in [false, true] {
            assert_eq!(
                record(&wrapped, &mut provider, faded, &[256]),
                record(&wrapped, &mut provider, faded, &[1, 71, 256])
            );
        }
    }
}
