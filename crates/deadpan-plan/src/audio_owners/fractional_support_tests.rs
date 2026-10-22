//! Retained ownership uses interval overlap, not one reference-sample adjacency.
use super::*;

// Isolate the receiving walk after a checked retained transfer. The immutable
// document supplies the real node and placement; only the transferred reference
// coordinate/rate varies. Raw PCM support and source admission are not changed.
fn transferred(
    position: Position,
    grid: AudioSampleGrid<AudioSample>,
    origin: AudioOwnerClockOrigin,
    allow_inactive: bool,
    extent: Range<ExactRatio>,
    maximum_work: usize,
) -> Result<(WalkOutput, usize), PlanError> {
    let document = document(
        48_000,
        &["prefix", "voice"],
        vec![("prefix", hold(10)), ("voice", hold(3))],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let placement = Placement {
        origin: ratio(10, 1),
        scale: ExactRatio::ONE,
    };
    let state = Walk {
        node: plan.by_id[&id("voice")],
        grid,
        placement,
        binding: BindingClock { grid, placement },
        extent,
        allow_inactive,
        ..Walk::root(&plan).unwrap()
    };
    let mut output = WalkOutput {
        start: 0,
        end: 1,
        owners: vec![],
        support: AudioOwnerSupport::Active,
        through_processor: false,
        original_is_silent: false,
    };
    let mut budget = Budget {
        remaining: maximum_work,
        lookup: LookupStats::default(),
    };
    walk(&plan, state, position, &mut output, origin, &mut budget, 0)?;
    Ok((output, budget.remaining))
}

fn retained_origin() -> AudioOwnerClockOrigin {
    AudioOwnerClockOrigin::Retained {
        definition: None,
        instance: InstancePath {
            node: id("voice"),
            repeats: vec![],
        },
        kind: AudioOwnerKind::Node,
        gap_after: None,
    }
}

fn point_grid() -> AudioSampleGrid<AudioSample> {
    AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap()
}

#[test]
fn retained_wide_transfer_overlap_preserves_exact_negative_owner_clock_and_work() {
    let (_, ordinary_remaining) = transferred(
        Position {
            at: ratio(10, 1),
            step: ratio(7, 4),
        },
        point_grid(),
        retained_origin(),
        false,
        ratio(10, 1)..ratio(13, 1),
        100,
    )
    .unwrap();
    for (distance, step) in [
        (ratio(23, 20), ratio(7, 4)),
        (ratio(5, 2), ratio(7, 2)),
        (ratio(63, 2), ratio(127, 4)),
    ] {
        let (output, remaining) = transferred(
            Position {
                at: ratio(10, 1).checked_sub(distance).unwrap(),
                step,
            },
            point_grid(),
            retained_origin(),
            false,
            ratio(10, 1)..ratio(13, 1),
            100,
        )
        .unwrap();
        assert_eq!(remaining, ordinary_remaining);
        assert_eq!(output.support, AudioOwnerSupport::Active);
        assert_eq!(output.owners.len(), 1);
        assert_eq!(output.owners[0].instance.node, id("voice"));
        assert_eq!(
            output.owners[0].position.at,
            ExactRatio::ZERO.checked_sub(distance).unwrap()
        );
        assert_eq!(output.owners[0].position.step, step);
        assert_eq!(output.end, 1);
    }
    let used = 100 - ordinary_remaining;
    assert!(matches!(
        transferred(
            Position {
                at: ratio(177, 20),
                step: ratio(7, 4)
            },
            point_grid(),
            retained_origin(),
            false,
            ratio(10, 1)..ratio(13, 1),
            used - 1,
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

#[test]
fn retained_overlap_requires_positive_allocation_and_strictly_crossing_start() {
    for (at, step, extent) in [
        // Exactly reaching the first point gives it no half-open overlap.
        (ratio(33, 4), ratio(7, 4), ratio(10, 1)..ratio(13, 1)),
        // A sample wholly before that point remains inactive.
        (ratio(8, 1), ratio(7, 4), ratio(10, 1)..ratio(13, 1)),
        // Empty meaningful support never grants an owner.
        (ratio(177, 20), ratio(7, 4), ratio(10, 1)..ratio(10, 1)),
        // End exhaustion is not a beginning overlap.
        (ratio(13, 1), ratio(7, 4), ratio(10, 1)..ratio(13, 1)),
    ] {
        let (output, _) = transferred(
            Position { at, step },
            point_grid(),
            retained_origin(),
            true,
            extent.clone(),
            100,
        )
        .unwrap();
        assert_eq!(output.support, AudioOwnerSupport::Inactive);
        assert!(output.owners.is_empty());
        assert!(matches!(
            transferred(
                Position { at, step },
                point_grid(),
                retained_origin(),
                false,
                extent,
                100,
            ),
            Err(PlanError::InvalidPlan(
                "owner coordinate is outside its allocated interval"
            ))
        ));
    }
    assert!(matches!(
        transferred(
            Position {
                at: ratio(177, 20),
                step: ratio(7, 4)
            },
            point_grid(),
            AudioOwnerClockOrigin::Current,
            true,
            ratio(10, 1)..ratio(13, 1),
            100,
        ),
        Err(PlanError::InvalidPlan(
            "owner coordinate is outside its allocated interval"
        ))
    ));
}

fn public_fractional_history() -> ProjectDocument {
    let old = document_at(
        FrameRate::new(30_000, 1001).unwrap(),
        &["prefix", "processor", "suffix"],
        vec![
            ("prefix", hold(3)),
            ("before", hold(3)),
            ("pause", hold(2)),
            ("after", hold(4)),
            ("suffix", hold(3)),
            ("body", sequence(&["before", "pause", "after"])),
            (
                "processor",
                retime("body", 4, 1..8, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
    );
    // Capture all physical recipes so this pure clock fixture does not need
    // decoded media to obtain the same retained PointCeil child provenance.
    let bindings = capture_unbound_audio_bindings(
        &old,
        AudioTimingId {
            allocation: revision("capture-fractional"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("move-fractional-stage"),
            command: Command::SetHoldDuration {
                node: id("prefix"),
                duration: frames(4),
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

#[test]
fn fractional_preserve_seam_selects_following_hold_with_retained_negative_clock() {
    let before = public_fractional_history();
    let plan = RenderPlan::compile(&before).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(8235)..AudioSample(8238), Default::default())
        .unwrap();
    let observed = observations(&query);
    assert_eq!(observed[&8235].last().unwrap().instance.node, id("before"));
    let hold = observed[&8236].last().unwrap();
    assert_eq!(hold.instance.node, id("pause"));
    assert_eq!(hold.local, ratio(-1, 4576));
    assert_eq!(hold.step, ratio(35, 32032));
    assert_eq!(query.spans()[0].support(), AudioOwnerSupport::Active);
    assert!(query.spans().iter().all(|span| !span.original_is_silent()));
    assert!(
        query
            .spans()
            .iter()
            .all(|span| span.support() == AudioOwnerSupport::Active)
    );
    for at in 8235..8238 {
        let single = plan
            .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
            .unwrap();
        assert_eq!(observations(&single).get(&at), observed.get(&at));
    }
    let exhausted = plan
        .audio_gain_owners(AudioSample(12812)..AudioSample(12813), Default::default())
        .unwrap();
    assert_eq!(exhausted.spans()[0].support(), AudioOwnerSupport::Inactive);
    assert_eq!(
        exhausted.spans()[0]
            .owners()
            .iter()
            .map(|owner| owner.instance().node.clone())
            .collect::<Vec<_>>(),
        vec![id("root")],
    );
}

#[test]
fn retained_overlap_keeps_round_even_tie_ownership_after_odd_label_translation() {
    let grid: AudioSampleGrid<AudioSample> =
        AudioSampleGrid::new(ratio(1, 2), ExactRatio::ONE, AudioBoundaryRule::RoundEven).unwrap();
    let transported = grid.translated_samples(1).unwrap();
    for (grid, label, first) in [(grid, ratio(35, 4), 10), (transported, ratio(39, 4), 11)] {
        assert_eq!(grid.boundary(ratio(10, 1)).unwrap(), AudioSample(first));
        let (probe, bias) = grid.probe(AudioSample(first - 1)).unwrap();
        assert_eq!(probe, ratio(10, 1));
        assert_eq!(bias, InsertionBias::Left);
        let (output, _) = transferred(
            Position {
                at: label,
                step: ratio(7, 4),
            },
            grid,
            retained_origin(),
            false,
            ratio(10, 1)..ratio(13, 1),
            100,
        )
        .unwrap();
        assert_eq!(output.support, AudioOwnerSupport::Active);
        assert_eq!(output.owners[0].instance.node, id("voice"));
        assert_eq!(output.owners[0].position.at, ratio(-3, 4));
        assert_eq!(output.owners[0].position.step, ratio(7, 4));
    }
}
