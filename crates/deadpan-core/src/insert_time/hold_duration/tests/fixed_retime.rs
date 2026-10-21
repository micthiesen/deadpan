//! A fixed affine stage stops exterior ripple without stopping interior clocks.
use super::*;

fn rate(child: &str, length: i64, start: i64, end: i64) -> BeatNode {
    let mut node = BeatNode::sequence("Rate", vec![]);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(length),
        mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    node
}

fn fixed() -> ProjectDocument {
    let mut document = fixture(0, false);
    document.nodes.remove(&id("before"));
    document.nodes.insert(id("after"), source(3));
    document.nodes.insert(
        id("body"),
        BeatNode::sequence("Body", vec![id("hold"), id("after")]),
    );
    document.nodes.insert(id("rate"), rate("body", 4, 0, 4));
    document.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("rate"), id("tail")]),
    );
    document.validate().unwrap();
    document
}

fn captured(document: &ProjectDocument) -> AudioBindingState {
    capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: rev("old-window"),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn instance(owner: &str, repeats: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(owner),
        repeats: repeats
            .iter()
            .map(|(owner, ordinal)| RepeatInstance {
                node: id(owner),
                iteration: IterationId {
                    allocation: rev(owner),
                    ordinal: *ordinal,
                },
            })
            .collect(),
    }
}

fn same_lattice(old: &AudioBindingState, after: &ProjectDocument, instance: &InstancePath) {
    let old = binding(old, instance, &None);
    let actual = binding(after.audio_bindings(), instance, &None);
    assert_eq!(actual.lattice.origin, old.lattice.origin);
    assert_eq!(
        actual.lattice.frames_per_local_frame,
        old.lattice.frames_per_local_frame
    );
    assert_eq!(actual.lattice.grid_origin, old.lattice.grid_origin);
    assert_eq!(
        actual.lattice.frames_per_sample,
        old.lattice.frames_per_sample
    );
    assert_eq!(actual.lattice.local_support, old.lattice.local_support);
    assert_eq!(
        actual.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        old.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
    );
}

#[test]
fn ordinary_hold_duration_fixed_window_grow_shrink_keep_old_interior_clocks_and_outer_reanchors() {
    for partial in [false, true] {
        let mut before = fixed();
        let old = captured(&before);
        if partial {
            before.audio_bindings = old.clone();
            before.audio_bindings.bindings.remove(&id("after"));
        }
        for length in [1, 3] {
            let after = resized(&before, "hold", length);
            assert_eq!(after.duration().unwrap(), before.duration().unwrap());
            assert_eq!(after.nodes()[&id("rate")], before.nodes()[&id("rate")]);
            let speech = instance("after", &[]);
            same_lattice(&old, &after, &speech);
            let frozen = FrozenAudioLayout::capture(&after).unwrap();
            assert_eq!(
                frozen
                    .project(&speech, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
                    .unwrap()
                    .origin,
                ExactRatio::integer(3 + length)
            );
            assert_eq!(
                binding(after.audio_bindings(), &speech, &None)
                    .lattice
                    .origin,
                ExactRatio::integer(5)
            );
            for owner in ["lead", "tail"] {
                let current = &after.audio_bindings.bindings()[&id(owner)];
                assert!(
                    current.reanchors.is_empty(),
                    "unchanged exterior owner {owner} was reanchored"
                );
                if partial {
                    assert_eq!(current, &before.audio_bindings.bindings()[&id(owner)]);
                }
            }
        }
    }
}

#[test]
fn ordinary_hold_duration_fixed_window_crop_noop_invalid_shrink_and_processing_ancestor_keep_admission()
 {
    let mut before = fixed();
    before.nodes.insert(id("rate"), rate("body", 4, 3, 5));
    before.validate().unwrap();
    assert_eq!(
        resized(&before, "hold", 2).audio_bindings,
        before.audio_bindings
    );
    let old = captured(&before);
    let speech = instance("after", &[]);
    assert_eq!(
        binding(&old, &speech, &None).lattice.local_support.start,
        ExactRatio::ONE
    );
    let after = resized(&before, "hold", 3);
    same_lattice(&old, &after, &speech);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    // The selected window still has to fit the reduced child. No clipping or
    // window rewrite is invented to admit a destructive shortening.
    assert!(apply(&before, &request(&before, "hold", 1)).is_err());
    assert_eq!(before.audio_bindings, AudioBindingState::default());

    // A processing ancestor above the affine window retains that window while
    // its physical descendants now retain their selected input clocks.
    let mut processing = fixed();
    let mut outer = rate("rate", 3, 0, 4);
    let NodeKind::Retime { pitch, .. } = &mut outer.kind else {
        panic!()
    };
    *pitch = PitchPolicy::Preserve;
    processing.nodes.insert(id("processing"), outer);
    processing.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("processing"), id("tail")]),
    );
    processing.validate().unwrap();
    let after = resized(&processing, "hold", 3);
    assert_eq!(
        after.audio_bindings.bindings()[&id("after")]
            .lattice
            .reference
            .root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("processing")
        }
    );
    assert!(
        after.audio_bindings.bindings()[&id("processing")]
            .reanchors
            .is_empty()
    );
    assert_eq!(after.duration().unwrap(), processing.duration().unwrap());
}

#[test]
fn ordinary_hold_duration_fixed_nested_nonunity_windows_capture_exact_rational_origin() {
    let mut before = fixed();
    before.nodes.insert(id("rate"), rate("body", 6, 0, 4));
    before.nodes.insert(id("outer-rate"), rate("rate", 5, 1, 5));
    before.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("outer-rate"), id("tail")]),
    );
    before.validate().unwrap();
    let old = captured(&before);
    let speech = instance("after", &[]);
    let original = binding(&old, &speech, &None);
    assert_eq!(original.lattice.origin, ExactRatio::new(11, 2).unwrap());
    assert_eq!(
        original.lattice.frames_per_local_frame,
        ExactRatio::new(15, 8).unwrap()
    );
    for (length, live_origin) in [
        (1, ExactRatio::new(29, 8).unwrap()),
        (3, ExactRatio::new(59, 8).unwrap()),
    ] {
        let after = resized(&before, "hold", length);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        same_lattice(&old, &after, &speech);
        let live = FrozenAudioLayout::capture(&after)
            .unwrap()
            .project(&speech, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
            .unwrap();
        assert_eq!(live.origin, live_origin);
        assert_eq!(
            live.frames_per_local_frame,
            original.lattice.frames_per_local_frame
        );
        for owner in ["lead", "tail"] {
            assert!(
                after.audio_bindings.bindings()[&id(owner)]
                    .reanchors
                    .is_empty()
            );
        }
    }
}

fn repeated_window() -> ProjectDocument {
    let mut before = fixture(2, true);
    before.nodes.insert(id("rate"), rate("inner", 20, 0, 20));
    let NodeKind::Repeat { child, .. } = &mut before.nodes.get_mut(&id("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = id("rate");
    before.validate().unwrap();
    before
}

#[test]
fn ordinary_hold_duration_fixed_window_repeats_inside_and_outside_keep_only_interior_steps() {
    let mut before = repeated_window();
    before.audio_bindings = captured(&before);
    before.validate().unwrap();
    let affected = super::super::fixed_window_owners(&before, &id("hold"))
        .unwrap()
        .unwrap();
    assert!(affected.contains(&id("inner")) && affected.contains(&id("owned-after")));
    assert!(!affected.contains(&id("outer")) && !affected.contains(&id("tail")));
    for length in [1, 3] {
        let after = resized(&before, "hold", length);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        for owner in ["lead", "tail"] {
            assert_eq!(
                after.audio_bindings.bindings()[&id(owner)],
                before.audio_bindings.bindings()[&id(owner)]
            );
        }
        assert_eq!(
            after.audio_bindings.gap_bindings()[&id("outer")],
            before.audio_bindings.gap_bindings()[&id("outer")]
        );
        assert!(
            after.audio_bindings.gap_bindings()[&id("inner")]
                .reanchors
                .len()
                > before.audio_bindings.gap_bindings()[&id("inner")]
                    .reanchors
                    .len()
        );
        for outer in 0..2 {
            for inner in 0..3 {
                let owner = if inner == 1 { "owned-after" } else { "after" };
                same_lattice(
                    &before.audio_bindings,
                    &after,
                    &instance(owner, &[("outer", outer), ("inner", inner)]),
                );
            }
        }
    }
}

#[test]
fn ordinary_hold_duration_fixed_window_million_by_million_repeat_clocks_stay_compact() {
    let mut before = repeated_window();
    for owner in ["outer", "inner"] {
        let NodeKind::Repeat { iterations, .. } =
            &mut before.nodes.get_mut(&id(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::new(rev(owner), 1_000_000).unwrap();
    }
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.nodes().len(), before.nodes().len());
    assert!(after.audio_bindings.timings().len() <= 1);
    assert!(after.audio_bindings.to_json().unwrap().len() < 50_000);
    assert!(
        after.audio_bindings.gap_bindings()[&id("outer")]
            .reanchors
            .is_empty()
    );
    assert!(
        after.audio_bindings.bindings()[&id("tail")]
            .reanchors
            .is_empty()
    );
}

#[test]
fn ordinary_hold_duration_fixed_window_all_overridden_default_resizes_keep_entire_clock_state() {
    let mut before = fixed();
    before.nodes.insert(id("inner"), repeat("inner", "body", 2));
    before.nodes.insert(id("owned-a"), source(6));
    before.nodes.insert(id("owned-b"), source(6));
    before.overrides.insert(
        id("inner"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: IterationId {
                    allocation: rev("inner"),
                    ordinal: 0,
                },
                root: id("owned-a"),
            },
            PlayOverride {
                iteration: IterationId {
                    allocation: rev("inner"),
                    ordinal: 1,
                },
                root: id("owned-b"),
            },
        ])
        .unwrap(),
    );
    // Both six-frame plays and their one-frame room-tone gap are live. The
    // default Body remains authored but neither play ever uses its Hold.
    before.nodes.insert(id("rate"), rate("inner", 13, 0, 13));
    before.validate().unwrap();
    before.audio_bindings = captured(&before);
    before.validate().unwrap();
    let old_clocks = before.audio_bindings.clone();
    let mut current = before.clone();
    // The old collector appended steps to audible owned branches and the gap
    // on every dormant resize, eventually exhausting the 256-term limit.
    for index in 0..(MAX_AUDIO_BINDING_TERMS + 2) {
        let length = if index % 2 == 0 { 1 } else { 3 };
        let mut request = request(&current, "hold", length);
        request.new_revision = rev(&format!("dormant-resize-{index}"));
        let edit = apply(&current, &request).unwrap();
        assert_eq!(edit.duration_delta, 0);
        let after = edit.forward.apply(&current).unwrap();
        assert_eq!(edit.inverse.apply(&after).unwrap(), current);
        assert_eq!(after.audio_bindings, old_clocks);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        current = after;
    }
    assert_eq!(
        ProjectDocument::from_json(&current.to_json().unwrap()).unwrap(),
        current
    );
}
