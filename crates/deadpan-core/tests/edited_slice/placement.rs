use super::*;

fn pool(name: &str, count: usize) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count)
            .map(|n| id(&format!("{name}-split-{n}")))
            .collect(),
    }
}

fn inside(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    parent: &NodeId,
    target: &NodeId,
    at: i64,
) -> CommandRequest {
    let preflight = document
        .slice_splice_interior(parent, target, duration(at), slice)
        .unwrap();
    let Command::SpliceSlice {
        identities, timing, ..
    } = paste(document, slice, name, 0).command
    else {
        unreachable!()
    };
    request(
        document,
        name,
        Command::SpliceSliceAt {
            parent: parent.clone(),
            target: target.clone(),
            at: duration(at),
            slice: slice.clone(),
            identities,
            split_identities: pool(name, preflight.required_ids),
            timing,
        },
    )
}

fn replace(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    parent: &NodeId,
    range: FrameRange,
) -> CommandRequest {
    let preflight = document.slice_replacement(parent, range, slice).unwrap();
    let Command::SpliceSlice {
        identities, timing, ..
    } = paste(document, slice, name, 0).command
    else {
        unreachable!()
    };
    request(
        document,
        name,
        Command::ReplaceSlice {
            parent: parent.clone(),
            range,
            slice: slice.clone(),
            identities,
            split_identities: pool(name, preflight.required_ids),
            timing,
        },
    )
}

fn checked(document: &ProjectDocument, command: &CommandRequest) -> ProjectDocument {
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(command).unwrap()).unwrap(),
        *command
    );
    edit(document, command)
}

fn children<'a>(document: &'a ProjectDocument, parent: &NodeId) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        panic!("Sequence")
    };
    children
}

fn physical(document: &ProjectDocument, child: &NodeId) -> NodeId {
    let mut child = child;
    while let NodeKind::Retime {
        child: next,
        purpose: RetimePurpose::Partition,
        ..
    } = &document.nodes()[child].kind
    {
        child = next;
    }
    child.clone()
}

fn source_tree(frames: i64) -> ProjectDocument {
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: frames,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("media").unwrap();
    let mut node = hold(frames);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: duration(frames),
            video: SourceVideo::Stream {
                asset: asset.clone(),
                span,
            },
            audio: None,
            video_mapping: SourceVideoMapping::FitBeat,
            audio_mapping: SourceAudioMapping::FitBeat,
            link: LinkRelation::Independent,
            audio_offset: AudioSample(0),
        },
    };
    let mut wire = serde_json::to_value(tree(&["voice"], vec![("voice", hold(frames))])).unwrap();
    wire["assets"] = json!({ asset.as_str(): AssetRecord {
        label: "Media".into(), content_hash: "a".repeat(64), video: Some(span), audio: None,
        frame_count: None, still_image: false, source_qualification: None,
    }});
    wire["nodes"]["voice"] = serde_json::to_value(node).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn binding(document: &ProjectDocument, node: &NodeId) -> ResolvedAudioBinding {
    document
        .audio_bindings()
        .resolve(
            node,
            &InstancePath {
                node: node.clone(),
                repeats: Vec::new(),
            },
            MAX_AUDIO_BINDING_ENTRIES,
        )
        .unwrap()
}

fn entry(binding: &ResolvedAudioBinding) -> ExactRatio {
    ExactRatio::integer(
        binding
            .lattice
            .sample_boundary(binding.lattice.local_support.start)
            .unwrap(),
    )
    .checked_add(
        binding
            .resume
            .as_ref()
            .unwrap()
            .reference_local_delta
            .checked_div(binding.lattice.local_frames_per_sample().unwrap())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn repeated_interior_paste_retains_original_lattice_and_chronological_sample_phase() {
    let before = source_tree(4);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 1), timing("capture")).unwrap();
    let first = checked(
        &before,
        &inside(&before, &slice, "first", &id("root"), &id("voice"), 1),
    );
    let right = children(&first, &id("root"))[2].clone();
    let first_owner = physical(&first, &right);
    let first_binding = binding(&first, &first_owner);
    assert_eq!(entry(&first_binding), ExactRatio::integer(1602));
    let second = checked(
        &first,
        &inside(&first, &slice, "second", &id("root"), &right, 1),
    );
    let suffix = physical(&second, &children(&second, &id("root"))[4]);
    let second_binding = binding(&second, &suffix);
    assert_eq!(second_binding.lattice, first_binding.lattice);
    // B1 + (B3-B2) = 1602 + (4805-3203), not the original B2=3203.
    assert_eq!(entry(&second_binding), ExactRatio::integer(3204));
    assert_eq!(second.duration().unwrap(), duration(6));
    assert_eq!(second.nodes()[&suffix], before.nodes()[&id("voice")]);
}

#[test]
fn equal_replacement_keeps_the_suffix_clock_through_its_last_rounded_sample() {
    let before = source_tree(3);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 1), timing("capture")).unwrap();
    let after = checked(
        &before,
        &replace(&before, &slice, "replace", &id("root"), range(1, 2)),
    );
    let suffix = physical(&after, &children(&after, &id("root"))[2]);
    let resolved = binding(&after, &suffix);
    assert_eq!(entry(&resolved), ExactRatio::integer(3203));
    assert_eq!(
        resolved
            .lattice
            .sample_boundary(ExactRatio::integer(3))
            .unwrap(),
        4805
    );
    assert_eq!(resolved.lattice.local_support.end, ExactRatio::integer(3));
    let steps = &after.audio_bindings().bindings()[&suffix].reanchors;
    assert_eq!(steps.len(), 1);
    assert_eq!(
        steps[0].window,
        Some(ExactFrameRange::new(ExactRatio::integer(2), ExactRatio::integer(3)).unwrap())
    );
    let layout = &after.audio_bindings().timings()[&steps[0].placement.reference.timing];
    assert_eq!(layout.duration(), duration(3));
    assert_eq!(after.duration().unwrap(), duration(3));
}

#[test]
fn pasted_nested_partitions_support_both_new_placements_without_broadening_source_admission() {
    let original = source_tree(9);
    let first_slice =
        CapturedEditSlice::capture(&original, &id("root"), range(1, 8), timing("capture")).unwrap();
    let empty = tree(&[], Vec::new());
    let first = checked(&empty, &paste(&empty, &first_slice, "first", 0));
    let second_slice = CapturedEditSlice::capture(
        &first,
        &id("first-node-0"),
        range(2, 6),
        timing("capture-two"),
    )
    .unwrap();
    let second = checked(&empty, &paste(&empty, &second_slice, "second", 0));
    let parent = id("second-node-0");
    let target = children(&second, &parent)[0].clone();
    assert!(
        second
            .source_splice_interior(&parent, &target, duration(1))
            .is_err()
    );
    let inserted = checked(
        &second,
        &inside(&second, &first_slice, "nested", &parent, &target, 1),
    );
    assert_eq!(inserted.duration().unwrap(), duration(11));
    assert!(inserted.source_replacement(&parent, range(9, 10)).is_err());
    let replaced = checked(
        &inserted,
        &replace(&inserted, &first_slice, "replaced", &parent, range(9, 10)),
    );
    assert_eq!(replaced.duration().unwrap(), duration(17));
    let suffix = physical(&replaced, children(&replaced, &parent).last().unwrap());
    assert_eq!(replaced.nodes()[&suffix], original.nodes()[&id("voice")]);
}

#[test]
fn replacement_uses_only_needed_destination_timing_slots_even_at_max_ordinal() {
    let before = tree(
        &["lead", "body"],
        vec![("lead", hold(1)), ("body", hold(4))],
    );
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 1), timing("capture")).unwrap();
    let imports = u32::try_from(slice.identity_requirements().unwrap().timings).unwrap();
    assert!(imports > 0);
    for (name, start, end, destination_slots) in [
        ("whole", 0, 5, 0),
        ("terminal-seam", 1, 5, 0),
        ("prefix-seam", 0, 1, 1),
        ("terminal-cut", 2, 5, 1),
        ("two-cuts", 2, 4, 2),
    ] {
        let mut command = replace(&before, &slice, name, &id("root"), range(start, end));
        let Command::ReplaceSlice { timing, .. } = &mut command.command else {
            unreachable!()
        };
        timing.ordinal = u32::MAX - (imports + destination_slots - 1);
        let first = timing.ordinal;
        let after = checked(&before, &command);
        let ordinals: Vec<_> = after
            .audio_bindings()
            .timings()
            .keys()
            .filter(|key| key.allocation == revision(name))
            .map(|key| key.ordinal)
            .collect();
        assert_eq!(ordinals, (first..=u32::MAX).collect::<Vec<_>>(), "{name}");
    }
}

#[test]
fn joint_pools_clock_overflow_and_split_mark_fragment_limits_reject_atomically() {
    let before = tree(&["held"], vec![("held", hold(4))]);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 1), timing("capture")).unwrap();
    let snapshot = before.clone();
    for mode in 0..4 {
        let mut command = inside(&before, &slice, "invalid", &id("root"), &id("held"), 1);
        let Command::SpliceSliceAt {
            identities,
            split_identities,
            timing,
            ..
        } = &mut command.command
        else {
            unreachable!()
        };
        match mode {
            0 => split_identities.nodes[0] = identities.aliases[0].clone(),
            1 => split_identities.nodes[0] = identities.authored.nodes[0].clone(),
            2 => split_identities.nodes.clear(),
            _ => timing.ordinal = u32::MAX,
        }
        let error = apply(&before, &command).unwrap_err();
        assert_eq!(
            error.code,
            if mode < 2 {
                EditErrorCode::IdentityConflict
            } else {
                EditErrorCode::LimitExceeded
            }
        );
        assert_eq!(before, snapshot);
    }
    for at in [0, 4] {
        assert!(
            before
                .slice_splice_interior(&id("root"), &id("held"), duration(at), &slice)
                .is_err()
        );
    }
    assert!(
        before
            .slice_splice_interior(&id("held"), &id("held"), duration(1), &slice)
            .is_err()
    );
    assert!(
        before
            .slice_replacement(&id("root"), range(1, 1), &slice)
            .is_err()
    );
    let mut crowded = mark("held", 0, InsertionBias::Right);
    crowded.fragments = (1..MAX_MARK_BINDINGS)
        .map(|n| MarkFragment {
            owner: id("held"),
            coordinate: Anchor::Local {
                node: id("held"),
                position: ExactRatio::new(i128::try_from(n).unwrap(), 1024).unwrap(),
            },
            state: MarkState::Bound,
        })
        .collect();
    let crowded = marked(&before, vec![("crowded", crowded)]);
    let command = inside(
        &crowded,
        &slice,
        "crowded-paste",
        &id("root"),
        &id("held"),
        1,
    );
    assert_eq!(
        apply(&crowded, &command).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
}

pub(super) fn with_sound(document: &ProjectDocument) -> ProjectDocument {
    assert_eq!(document.duration().unwrap(), duration(10));
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 16_016,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("catalog").unwrap();
    let registered = edit(
        document,
        &request(
            document,
            "registered",
            Command::AddAsset {
                id: asset.clone(),
                asset: AssetRecord {
                    label: "Sound".into(),
                    content_hash: "a".repeat(64),
                    audio: Some(span),
                    video: None,
                    frame_count: None,
                    still_image: false,
                    source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
                },
            },
        ),
    );
    let placed = edit(
        &registered,
        &request(
            &registered,
            "placed",
            Command::SetSound {
                id: SoundId::new("sound").unwrap(),
                event: SoundEvent {
                    owner: id("root"),
                    label: "Sound".into(),
                    source: SourceAudio { asset, span },
                    mapping: SourceAudioMapping::natural_rate(
                        span,
                        registered.presentation_basis().frame_rate,
                    )
                    .unwrap(),
                    offset: AudioSample(0),
                    gain_millidecibels: 0,
                    start_edge: AudioEdgePolicy::Hard,
                    end_edge: AudioEdgePolicy::Hard,
                    overflow: SoundOverflowPolicy::Reject,
                },
            },
        ),
    );
    ["middle", "tail"]
        .into_iter()
        .fold(placed, |document, owner| {
            edit(
                &document,
                &request(
                    &document,
                    &format!("allow-{owner}"),
                    Command::SetSoundAllowance {
                        sound: SoundId::new("sound").unwrap(),
                        issuer: SoundHoldIssuer::Node {
                            instance: InstancePath {
                                node: id(owner),
                                repeats: Vec::new(),
                            },
                        },
                        allowed: true,
                    },
                ),
            )
        })
}

#[test]
fn placement_transforms_marks_root_sound_and_split_allowances_once() {
    let original = tree(
        &["lead", "middle", "tail"],
        vec![("lead", hold(2)), ("middle", hold(4)), ("tail", hold(4))],
    );
    let mut pin = mark("lead", 0, InsertionBias::Right);
    pin.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(7),
    };
    let mut unresolved = mark("lead", 100, InsertionBias::Left);
    unresolved.state = MarkState::Unresolved {
        reason: MarkLossReason::OutOfRange,
    };
    let mut removed = mark("middle", 1, InsertionBias::Right);
    removed.loss_policy = AnchorLossPolicy::DeleteOwned;
    let before = with_sound(&marked(
        &original,
        vec![
            ("root-local", mark("root", 8, InsertionBias::Right)),
            ("pin", pin),
            ("dormant", unresolved),
            ("removed", removed),
            ("kept-intent", mark("middle", 1, InsertionBias::Right)),
        ],
    ));
    let snapshot = before.clone();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 2), timing("capture-sound"))
            .unwrap();
    assert_eq!(before, snapshot);
    let sound = SoundId::new("sound").unwrap();
    for is_replace in [false, true] {
        let name = if is_replace { "replace" } else { "interior" };
        let command = if is_replace {
            replace(&before, &slice, name, &id("root"), range(2, 6))
        } else {
            inside(&before, &slice, name, &id("root"), &id("middle"), 1)
        };
        let after = checked(&before, &command);
        assert_eq!(after.sounds(), before.sounds());
        let route = &after.sound_routes()[&sound];
        assert_eq!(route.edits.len(), 1);
        assert_eq!(route.recipe_extent, duration(10));
        assert_eq!(
            route.edits[0].operation,
            if is_replace {
                RootSoundOperation::Replace {
                    range: range(2, 6),
                    duration: duration(2),
                }
            } else {
                RootSoundOperation::Insert {
                    at: ProjectFrame(3),
                    duration: duration(2),
                }
            }
        );
        let root_mark = &after.marks()[&MarkId::new("root-local").unwrap()];
        assert_eq!(
            root_mark.boundary.coordinate,
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(if is_replace { 6 } else { 10 })
            }
        );
        let copies = copied_marks(&after, name);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().any(|mark| mark.boundary.coordinate
            == Anchor::Sequence {
                frame: ProjectFrame(7)
            }
            && mark.state == MarkState::Bound));
        assert!(copies.iter().any(|mark| matches!(mark.boundary.coordinate, Anchor::Local { position, .. } if position == ExactRatio::integer(100)) && mark.state == MarkState::Unresolved { reason: MarkLossReason::OutOfRange }));
        assert_eq!(
            after.sound_allowances()[&sound].len(),
            if is_replace { 1 } else { 3 }
        );
        assert!(after.sound_allowances()[&sound].iter().all(|issuer| {
            !issuer
                .instance()
                .node
                .as_str()
                .starts_with(&format!("{name}-node-"))
        }));
        if is_replace {
            assert!(!after.marks().contains_key(&MarkId::new("removed").unwrap()));
            assert_eq!(
                after.marks()[&MarkId::new("kept-intent").unwrap()].state,
                MarkState::Unresolved {
                    reason: MarkLossReason::OwnerMissing
                }
            );
        }
    }
}
