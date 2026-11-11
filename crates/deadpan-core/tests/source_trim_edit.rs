use deadpan_core::*;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn span(start: i64, end: i64, rate: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, rate).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}
fn window() -> SourceEditWindow {
    SourceEditWindow::new(ratio(1, 3), ratio(29, 3)).unwrap()
}
fn single_fixture(audio: Option<(i64, i64)>, offset: i64) -> ProjectDocument {
    let picture = span(0, 100, 30);
    let asset = AssetId::new("original").unwrap();
    let sound = audio.map(|(a, b)| span(a, b, 44_100));
    let audio_mapping = audio.map_or(SourceAudioMapping::FitBeat, |(a, b)| {
        let start = ratio(i128::from(a), 1470)
            .checked_sub(ExactRatio::integer(10))
            .unwrap();
        let extent = ratio(i128::from(b - a), 1470);
        let shift = ratio(i128::from(offset), 1600);
        let end = start.checked_add(extent).unwrap();
        let lo = start.checked_add(shift).unwrap();
        let hi = end.checked_add(shift).unwrap();
        let first = if lo.compare(window().start()).is_gt() {
            lo
        } else {
            window().start()
        };
        let last = if hi.compare(window().end()).is_lt() {
            hi
        } else {
            window().end()
        };
        let selection = if first.compare(last).is_lt() {
            ExactFrameRange {
                start: first.checked_sub(shift).unwrap(),
                end: last.checked_sub(shift).unwrap(),
            }
        } else {
            let point = if hi.compare(window().start()).is_le() {
                end
            } else {
                start
            };
            ExactFrameRange {
                start: point,
                end: point,
            }
        };
        SourceAudioMapping::SelectedPlacement {
            start,
            frames: extent,
            selection,
        }
    });
    let source = SourceNode {
        duration: frames(10),
        edit_window: Some(window()),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span: picture,
        },
        video_mapping: SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::integer(-10),
            frames: ExactRatio::integer(100),
            selection: ExactFrameRange::new(window().start(), window().end()).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
        audio: sound.map(|span| SourceAudio { asset, span }),
        audio_mapping,
        audio_offset: AudioSample(offset),
        link: if audio.is_some() {
            LinkRelation::Linked
        } else {
            LinkRelation::Independent
        },
    };
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"slip","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","marks":{},"overrides":{},
        "assets":{"original":AssetRecord {label:"Original".into(),content_hash:"a".repeat(64),video:Some(picture),audio:sound,still_image:false,frame_count:Some(frames(100)),source_qualification:Some(SourceQualificationId::new("b".repeat(64)).unwrap())}},
        "nodes":{"root":BeatNode::sequence("Root",vec![id("source")]),"source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}, cutaways: Vec::new(), captions: Vec::new() }},
    }).to_string()).unwrap()
}

fn modify(document: &ProjectDocument, update: impl FnOnce(&mut Value)) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    update(&mut wire);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn fixture(audio: Option<(i64, i64)>, offset: i64) -> ProjectDocument {
    modify(&single_fixture(audio, offset), |wire| {
        let source = wire["nodes"]
            .as_object_mut()
            .unwrap()
            .remove("source")
            .unwrap();
        wire["nodes"]["left"] = source.clone();
        wire["nodes"]["right"] = source;
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "right"]);
    })
}
fn source<'a>(document: &'a ProjectDocument, node: &str) -> &'a SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&id(node)].kind else {
        panic!()
    };
    source
}
fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}

fn hold(n: i64) -> BeatNode {
    BeatNode::hold(
        "Silent",
        HoldRecipe {
            duration: frames(n),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn revision(d: &ProjectDocument) -> RevisionId {
    RevisionId::new(format!("{}x", d.revision_id())).unwrap()
}
fn request(d: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision(d),
        command,
    }
}
fn intent(i: i64, o: i64, s: i64, r: i64, policy: SourceTrimPolicy) -> SourceTrimIntent {
    SourceTrimIntent {
        in_frames: i,
        out_frames: o,
        slip_frames: s,
        roll_frames: r,
        policy,
    }
}
fn resources(r: &SourceTrimEditResolution) -> SourceTrimResources {
    SourceTrimResources {
        target_wrapper: r.required_target_wrapper.then(|| id("a-crop")),
        right_wrapper: r.required_right_wrapper.then(|| id("b-crop")),
        split: SplitIdentities {
            nodes: (0..r.required_split_nodes)
                .map(|n| id(&format!("split-{n}")))
                .collect(),
        },
        fillers: (0..r.required_filler_nodes)
            .map(|n| id(&format!("filler-{n}")))
            .collect(),
    }
}
fn command_for(
    d: &ProjectDocument,
    left: &str,
    right: Option<&str>,
    intent: SourceTrimIntent,
) -> Command {
    let right = right.map(id);
    let r = d
        .source_trim_edit(&id("root"), &id(left), right.as_ref(), intent)
        .unwrap();
    Command::ApplySourceTrim {
        parent: id("root"),
        node: id(left),
        right,
        intent,
        resources: resources(&r),
    }
}
fn combined(d: &ProjectDocument, intent: SourceTrimIntent) -> Command {
    command_for(d, "left", Some("right"), intent)
}
fn edit(d: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(d, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let tx = apply(d, &request).unwrap();
    let next = tx.forward.apply(d).unwrap();
    assert_eq!(tx.inverse.apply(&next).unwrap(), *d);
    assert_eq!(
        tx.forward.apply(&tx.inverse.apply(&next).unwrap()).unwrap(),
        next
    );
    assert_eq!(
        ProjectDocument::from_json(&next.to_json().unwrap()).unwrap(),
        next
    );
    (next, tx)
}
fn wide(audio: Option<(i64, i64)>) -> ProjectDocument {
    let d = fixture(audio, 0);
    modify(&d, |v| {
        for (name, n) in [("left", 10), ("right", 20)] {
            let mut source = source(&d, name).clone();
            source.duration = frames(n);
            let w = SourceEditWindow::new(ratio(1, 3), ratio(i128::from(n * 3 - 1), 3)).unwrap();
            source.edit_window = Some(w);
            source.video_mapping = SourceVideoMapping::SelectedPlacement {
                start: ratio(-40, 1),
                frames: ratio(100, 1),
                selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
                endpoints: EndpointPolicy::HoldAdjacent,
            };
            if source.audio.is_some() {
                source.audio_mapping = SourceAudioMapping::SelectedPlacement {
                    start: ratio(-40, 1),
                    frames: ratio(100, 1),
                    selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
                };
            }
            v["nodes"][name]["kind"]["source"] = serde_json::to_value(source).unwrap();
        }
        v["nodes"]["prefix"] = serde_json::to_value(hold(10)).unwrap();
        v["nodes"]["suffix"] = serde_json::to_value(hold(20)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "left", "right", "suffix"]);
    })
}
fn intervals(d: &ProjectDocument) -> Vec<(String, FrameRange)> {
    let NodeKind::Sequence { children } = &d.nodes()[&id("root")].kind else {
        panic!()
    };
    let durations = d.durations().unwrap();
    let mut cursor = 0;
    children
        .iter()
        .map(|node| {
            let start = cursor;
            cursor += durations[node].frames();
            (node.to_string(), range(start, cursor))
        })
        .collect()
}
fn mark(
    d: &ProjectDocument,
    name: &str,
    owner: &str,
    coordinate: Anchor,
    bias: InsertionBias,
    policy: AnchorLossPolicy,
) -> ProjectDocument {
    modify(d, |v| {
        v["marks"][name] = serde_json::to_value(Mark {
            owner: id(owner),
            label: name.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy: policy,
            state: MarkState::Bound,
            fragments: vec![],
        })
        .unwrap()
    })
}

#[test]
fn overwrite_normal_form_handles_compensation_and_disjoint_swept_silence() {
    let before = wide(None);
    for (i, o, r, a, b, fillers) in [
        (
            5,
            20,
            -15,
            range(15, 25),
            Some(range(25, 40)),
            vec![range(10, 15)],
        ),
        (
            20,
            15,
            0,
            range(30, 35),
            Some(range(35, 40)),
            vec![range(10, 30)],
        ),
        (
            -10,
            -15,
            0,
            range(0, 5),
            Some(range(20, 40)),
            vec![range(5, 20)],
        ),
        (0, 0, 2, range(10, 22), Some(range(22, 40)), vec![]),
        (
            0,
            -5,
            5,
            range(10, 20),
            Some(range(25, 40)),
            vec![range(20, 25)],
        ),
        (
            2,
            -2,
            0,
            range(12, 18),
            Some(range(20, 40)),
            vec![range(10, 12), range(18, 20)],
        ),
        (35, 30, 0, range(45, 50), None, vec![range(10, 45)]),
    ] {
        let accepted = intent(i, o, 0, r, SourceTrimPolicy::Overwrite);
        let resolution = before
            .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), accepted)
            .unwrap();
        assert_eq!(resolution.geometry.target.output_after, a);
        assert_eq!(resolution.right_after.as_ref().map(|s| s.output), b);
        assert_eq!(resolution.fillers, fillers);
        let ids = resources(&resolution);
        let result_ids = resolution.result_identities(&ids).unwrap();
        let (after, tx) = edit(
            &before,
            Command::ApplySourceTrim {
                parent: id("root"),
                node: id("left"),
                right: Some(id("right")),
                intent: accepted,
                resources: ids,
            },
        );
        assert_eq!(tx.duration_delta, 0);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        let placements = intervals(&after);
        assert!(placements.contains(&(result_ids.target.to_string(), a)));
        for (id, expected) in result_ids.fillers.iter().zip(&fillers) {
            assert!(placements.contains(&(id.to_string(), *expected)));
            assert!(
                after.nodes()[id].audio_editorial_edges.start
                    && after.nodes()[id].audio_editorial_edges.end
            );
        }
        if b.is_none() {
            assert!(!after.nodes().contains_key(&id("right")));
        }
    }
}

#[test]
fn all_scalar_paths_preserve_their_exact_source_candidates() {
    let before = fixture(Some((0, 147000)), 17);
    for delta in [-2, 2] {
        for edge in [SourceTrimEdge::In, SourceTrimEdge::Out] {
            let scalar = before
                .source_trim(
                    &id("root"),
                    &id("left"),
                    edge,
                    delta,
                    SourceTrimMode::Ripple,
                )
                .unwrap();
            let accepted = if edge == SourceTrimEdge::In {
                intent(delta, 0, 0, 0, SourceTrimPolicy::Ripple)
            } else {
                intent(0, delta, 0, 0, SourceTrimPolicy::Ripple)
            };
            let (after, _) = edit(&before, combined(&before, accepted));
            assert_eq!(source(&after, "left"), &scalar.after);
        }
        let scalar = before.source_slip(&id("root"), &id("left"), delta).unwrap();
        let (after, _) = edit(
            &before,
            combined(&before, intent(0, 0, delta, 0, SourceTrimPolicy::Ripple)),
        );
        assert_eq!(source(&after, "left"), &scalar.after);
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        let scalar = before
            .source_roll(&id("root"), &id("left"), &id("right"), delta)
            .unwrap();
        let (after, _) = edit(
            &before,
            combined(&before, intent(0, 0, 0, delta, SourceTrimPolicy::Ripple)),
        );
        assert_eq!(source(&after, "left"), &scalar.left.after);
        assert_eq!(source(&after, "right"), &scalar.right.after);
    }
}

#[test]
fn near_empty_siblings_move_with_a_while_far_endpoints_survive_and_interior_empties_retire() {
    let before = modify(&wide(None), |v| {
        for name in ["near-in", "near-out", "inside", "far"] {
            v["nodes"][name] = serde_json::to_value(BeatNode::sequence(name, vec![])).unwrap();
        }
        v["nodes"]["middle"] = serde_json::to_value(hold(3)).unwrap();
        v["nodes"]["tail"] = serde_json::to_value(hold(7)).unwrap();
        v["nodes"].as_object_mut().unwrap().remove("suffix");
        v["nodes"]["root"]["kind"]["children"] = json!([
            "prefix", "near-in", "left", "near-out", "middle", "inside", "tail", "far", "right"
        ]);
    });
    let accepted = intent(2, 10, 0, 0, SourceTrimPolicy::Overwrite);
    let r = before
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(
        r.empty_moves
            .iter()
            .map(|m| (m.node.to_string(), m.before.0, m.after.0))
            .collect::<Vec<_>>(),
        vec![("near-in".into(), 10, 12), ("near-out".into(), 20, 30)]
    );
    let (after, _) = edit(&before, command_for(&before, "left", None, accepted));
    let positions = intervals(&after);
    assert!(positions.contains(&("near-in".into(), range(12, 12))));
    assert!(positions.contains(&("near-out".into(), range(30, 30))));
    assert!(positions.contains(&("far".into(), range(30, 30))));
    assert!(!after.nodes().contains_key(&id("inside")));
    let at30: Vec<_> = positions
        .iter()
        .filter(|(_, r)| r.start().0 == 30)
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(at30, vec!["near-out", "far", "right"]);
}

#[test]
fn split_occurrence_fragments_receive_loss_only_after_the_final_survivor_exists() {
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["neighbor"] =
            serde_json::to_value(BeatNode::sequence("neighbor", vec![id("right")])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
    });
    let mut before = before;
    for (name, pos, bias, policy) in [
        (
            "kept",
            7,
            InsertionBias::Right,
            AnchorLossPolicy::DeleteOwned,
        ),
        (
            "lost",
            1,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "cut-left",
            3,
            InsertionBias::Left,
            AnchorLossPolicy::DeleteOwned,
        ),
        (
            "cut-right",
            3,
            InsertionBias::Right,
            AnchorLossPolicy::DeleteOwned,
        ),
    ] {
        before = mark(
            &before,
            name,
            "right",
            Anchor::Occurrence {
                instance: InstancePath {
                    node: id("right"),
                    repeats: vec![],
                },
                position: ratio(pos, 1),
            },
            bias,
            policy,
        );
    }
    before = mark(
        &before,
        "ancestor",
        "root",
        Anchor::Local {
            node: id("root"),
            position: ratio(17, 1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = mark(
        &before,
        "definition",
        "right",
        Anchor::Local {
            node: id("right"),
            position: ratio(1, 1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::DeleteOwned,
    );
    let (after, _) = edit(
        &before,
        command_for(
            &before,
            "left",
            None,
            intent(0, 3, 0, 0, SourceTrimPolicy::Overwrite),
        ),
    );
    assert!(
        after.marks()[&MarkId::new("kept").unwrap()]
            .bindings()
            .all(|m| m.state == MarkState::Bound)
    );
    assert!(
        !after
            .marks()
            .contains_key(&MarkId::new("cut-left").unwrap())
    );
    assert_eq!(
        after.marks()[&MarkId::new("cut-right").unwrap()].state,
        MarkState::Bound
    );
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(
        after.marks()[&MarkId::new("ancestor").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ratio(17, 1)
        }
    );
    let hidden = &after.marks()[&MarkId::new("definition").unwrap()];
    assert_eq!(hidden.state, MarkState::Bound);
    assert_eq!(hidden.binding_count(), 1);
    assert_ne!(hidden.owner, id("right"));
}

fn with_sound(d: &ProjectDocument, issuer: Option<SoundHoldIssuer>) -> ProjectDocument {
    let sound = SoundId::new("sound").unwrap();
    modify(d, |v| {
        v["sounds"] = json!({"sound":SoundEvent{   owner:id("root"),label:"Root bus".into(),source:source(d,"left").audio.clone().unwrap(),mapping:SourceAudioMapping::SelectedPlacement{start:ratio(0,1),frames:ratio(100,1),selection:ExactFrameRange::new(ratio(0,1),ExactRatio::integer(d.duration().unwrap().frames())).unwrap()},offset:AudioSample(0),treatments: deadpan_core::AudioTreatments::default(),start_edge:AudioEdgePolicy::Automatic,end_edge:AudioEdgePolicy::Hard,overflow:SoundOverflowPolicy::Reject}});
        v["sound_routes"] = serde_json::to_value(std::collections::BTreeMap::from([(
            sound.clone(),
            RootSoundRoute::identity(d.duration().unwrap(), d.presentation_basis().frame_rate),
        )]))
        .unwrap();
        if let Some(issuer) = issuer {
            v["sound_allowances"] = serde_json::to_value(std::collections::BTreeMap::from([(
                sound,
                SoundHoldAllowances::try_from(vec![issuer]).unwrap(),
            )]))
            .unwrap();
        }
    })
}

#[test]
fn root_sound_mapping_uses_the_complete_in_out_intent_once_and_identity_restores_objects() {
    let before = with_sound(&wide(Some((0, 147000))), None);
    for accepted in [
        intent(0, 0, 2, 0, SourceTrimPolicy::Ripple),
        intent(0, 0, 0, 2, SourceTrimPolicy::Ripple),
    ] {
        let (after, _) = edit(&before, combined(&before, accepted));
        assert_eq!(after.sounds(), before.sounds());
        assert_eq!(after.sound_routes(), before.sound_routes());
    }
    for accepted in [
        intent(2, 2, 0, 0, SourceTrimPolicy::Ripple),
        intent(0, -3, 0, 3, SourceTrimPolicy::Ripple),
    ] {
        let (after, _) = edit(&before, combined(&before, accepted));
        let route = &after.sound_routes()[&SoundId::new("sound").unwrap()];
        assert_eq!(route.edits.len(), 1);
        assert_eq!(
            route.edits[0].operation,
            RootSoundOperation::Trim {
                range: range(10, 20),
                in_frames: accepted.in_frames,
                out_frames: accepted.out_frames
            }
        );
    }
}

#[test]
fn raw_zero_bad_resources_stale_targets_and_absence_never_author_or_retarget() {
    let before = wide(None);
    let bytes = before.to_json().unwrap();
    let zero = command_for(&before, "left", None, SourceTrimIntent::default());
    assert!(
        apply(&before, &request(&before, zero))
            .unwrap_err()
            .message
            .contains("no change")
    );
    let r = before
        .source_trim_edit(
            &id("root"),
            &id("left"),
            None,
            intent(0, 0, 1, 0, SourceTrimPolicy::Ripple),
        )
        .unwrap();
    assert!(matches!(
        r.geometry.roll_availability,
        SourceTrimRollAvailability::Unavailable { .. }
    ));
    assert!(
        before
            .source_trim_edit(
                &id("root"),
                &id("left"),
                None,
                intent(0, 0, 0, 1, SourceTrimPolicy::Ripple)
            )
            .is_err()
    );
    let good = combined(&before, intent(2, -2, 0, 0, SourceTrimPolicy::Overwrite));
    for mutate in 0..6 {
        let mut bad = good.clone();
        let Command::ApplySourceTrim {
            resources, right, ..
        } = &mut bad
        else {
            panic!()
        };
        match mutate {
            0 => resources.fillers.push(id("extra")),
            1 => resources.target_wrapper = None,
            2 => resources.fillers[0] = id("left"),
            3 => resources.fillers[1] = resources.fillers[0].clone(),
            4 => resources.target_wrapper = Some(id("left")),
            5 => *right = Some(id("prefix")),
            _ => unreachable!(),
        }
        // R=0 explicitly tolerates ineligible captured right; that mutation
        // changes the required generic neighbor treatment, not captured A.
        if mutate != 5 {
            assert!(apply(&before, &request(&before, bad)).is_err());
        } else {
            assert!(apply(&before, &request(&before, bad)).is_ok());
        }
    }
    let mut wrong_pair = combined(&before, intent(0, 0, 0, 1, SourceTrimPolicy::Ripple));
    let Command::ApplySourceTrim { right, .. } = &mut wrong_pair else {
        panic!()
    };
    *right = Some(id("prefix"));
    assert!(apply(&before, &request(&before, wrong_pair)).is_err());
    let mut stale = request(&before, good);
    stale.expected_revision = RevisionId::new("stale").unwrap();
    assert_eq!(
        apply(&before, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    assert_eq!(before.to_json().unwrap(), bytes);
}

#[test]
fn linked_dormant_support_can_activate_and_absent_audio_stays_absent() {
    for audio in [None, Some((60_000, 147_000))] {
        let before = fixture(audio, 0);
        let accepted = intent(25, 25, 0, 0, SourceTrimPolicy::Ripple);
        let old = source(&before, "left");
        if old.audio.is_some() {
            let w = old.audio_mapping.selection_frames(old.duration).unwrap();
            assert_eq!(w.start, w.end);
        }
        let (after, _) = edit(&before, combined(&before, accepted));
        let next = source(&after, "left");
        if audio.is_none() {
            assert!(next.audio.is_none())
        } else {
            let w = next.audio_mapping.selection_frames(next.duration).unwrap();
            assert!(w.start.compare(w.end).is_lt())
        }
    }
}

#[test]
fn closed_resources_reject_new_vocabulary() {
    let before = wide(None);
    let current = request(
        &before,
        combined(&before, intent(2, -2, 0, 0, SourceTrimPolicy::Overwrite)),
    );
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["command"]["resources"]["fillers"] = json!(["one", "two", "three"]);
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["command"]["resources"]["unexpected"] = json!(null);
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
}

#[test]
fn nested_scope_ripple_captures_ancestor_suffix_but_overwrite_cannot_consume_it() {
    let before = modify(&wide(None), |v| {
        v["nodes"]["group"] =
            serde_json::to_value(BeatNode::sequence("Group", vec![id("left"), id("right")]))
                .unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "group", "suffix"]);
    });
    let accepted = intent(1, 4, 0, -1, SourceTrimPolicy::Ripple);
    let r = before
        .source_trim_edit(&id("group"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    let command = Command::ApplySourceTrim {
        parent: id("group"),
        node: id("left"),
        right: Some(id("right")),
        intent: accepted,
        resources: resources(&r),
    };
    let (after, _) = edit(&before, command);
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 3
    );
    assert!(
        before
            .source_trim_edit(
                &id("group"),
                &id("left"),
                Some(&id("right")),
                intent(-1, 0, 0, 0, SourceTrimPolicy::Overwrite)
            )
            .is_err()
    );
}

#[test]
fn retained_padding_only_b_tail_keeps_its_full_window_and_is_not_a_retired_owner() {
    let base = wide(None);
    let before = modify(&base, |v| {
        let mut s = source(&base, "right").clone();
        let w = SourceEditWindow::new(ratio(1, 3), ratio(4, 3)).unwrap();
        s.edit_window = Some(w);
        s.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: ratio(-40, 1),
            frames: ratio(100, 1),
            selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        v["nodes"]["right"]["kind"]["source"] = serde_json::to_value(s).unwrap();
    });
    let accepted = intent(0, 13, 0, 0, SourceTrimPolicy::Overwrite);
    let r = before
        .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    let b = r.right_after.as_ref().unwrap();
    assert_eq!(b.output, range(33, 40));
    assert_eq!(b.allocation, range(13, 20));
    assert!(b.visible_selection.is_none());
    assert!(r.required_right_wrapper);
    let (after, _) = edit(&before, combined(&before, accepted));
    assert_eq!(source(&after, "right"), source(&before, "right"));
    assert!(after.nodes().contains_key(&id("right")));
    assert!(
        after
            .source_trim_geometry(
                &id("root"),
                &id("b-crop"),
                None,
                SourceTrimIntent::default()
            )
            .is_err()
    );
}

/// Empty Sequences add structural capacity without changing picture duration.
/// Callers use unbound fixtures: binding validation has a separate work budget
/// that includes all current nodes and therefore cannot admit a bound 100k tree.
fn at_node_capacity(document: &ProjectDocument, count: usize) -> ProjectDocument {
    assert!(document.nodes().len() <= count);
    assert!(count <= MAX_DOCUMENT_NODES);
    modify(document, |wire| {
        let extra = count - document.nodes().len();
        let ids: Vec<_> = (0..extra).map(|n| id(&format!("capacity-{n}"))).collect();
        for node in &ids {
            wire["nodes"][node.as_str()] =
                serde_json::to_value(BeatNode::sequence("", Vec::new())).unwrap();
        }
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .extend(ids.into_iter().map(|node| json!(node)));
    })
}

#[test]
fn overwrite_rejects_real_temporary_split_peak_even_when_final_tree_would_fit() {
    // A[0,10), Sequence(B)[10,20). Keeping the final five frames of that
    // Sequence needs its two-node full copy plus two neutral crop wrappers.
    let small = modify(&fixture(None, 0), |wire| {
        wire["nodes"]["neighbor"] =
            serde_json::to_value(BeatNode::sequence("Neighbor", vec![id("right")])).unwrap();
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
    });
    let accepted = intent(0, 5, 0, 0, SourceTrimPolicy::Overwrite);
    let small_resolution = small
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(small_resolution.required_split_nodes, 4);
    assert_eq!(small_resolution.splits.len(), 1);
    assert!(!small_resolution.required_target_wrapper);
    assert_eq!(small_resolution.required_filler_nodes, 0);
    let (small_after, _) = edit(&small, command_for(&small, "left", None, accepted));
    // The discarded left wrapper and its original two-node context are removed
    // only after the four temporary nodes exist. The final net growth is one.
    assert_eq!(small_after.nodes().len(), small.nodes().len() + 1);
    let before = at_node_capacity(&small, MAX_DOCUMENT_NODES - 3);
    let original = before.clone();
    let geometry = before
        .source_trim_geometry(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(geometry.required_source_wrappers, 0);
    assert!(geometry.requires_overwrite_overlay);
    let error = before
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("temporary node budget"));
    let authored = Command::ApplySourceTrim {
        parent: id("root"),
        node: id("left"),
        right: None,
        intent: accepted,
        resources: SourceTrimResources {
            split: SplitIdentities {
                nodes: (0..4).map(|n| id(&format!("peak-copy-{n}"))).collect(),
            },
            ..Default::default()
        },
    };
    let error = apply(&before, &request(&before, authored)).unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("temporary node budget"));
    assert_eq!(before, original);
}

#[test]
fn combined_trims_and_rolls_before_an_attached_sound_owner_keep_its_sound() {
    let sound = span(0, 4410, 44_100);
    let before = modify(&fixture(Some((0, 147000)), 7), |wire| {
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "right", "owner"]);
        wire["nodes"]["owner"] = serde_json::to_value(hold(4)).unwrap();
        wire["beat_sounds"] = json!({"owner": {"effect": BeatSound {
            label: "Effect".into(),
            source: SourceAudio { asset: AssetId::new("original").unwrap(), span: sound },
            mapping: SourceAudioMapping::natural_rate(sound, FrameRate::new(30, 1).unwrap()).unwrap(),
            offset: AudioSample(11),
            treatments: AudioTreatments::default(),
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Automatic,
            overflow: SoundOverflowPolicy::Reject,
        }}});
    });
    let journal = |document: &ProjectDocument| {
        document
            .audio_bindings()
            .sound_clocks()
            .get(&id("owner"))
            .map(|events| events[&SoundId::new("effect").unwrap()].clocks().len())
            .unwrap_or(0)
    };
    let owner_start = |document: &ProjectDocument| {
        let NodeKind::Sequence { children } = &document.nodes()[&id("root")].kind else {
            panic!()
        };
        let index = children
            .iter()
            .position(|child| child == &id("owner"))
            .unwrap();
        document
            .source_splice_boundary(&id("root"), index)
            .unwrap()
            .0
    };
    // A ripple Out trim moves the owner and records one clock.
    let (rippled, _) = edit(
        &before,
        combined(&before, intent(0, -2, 0, 0, SourceTrimPolicy::Ripple)),
    );
    assert_eq!(owner_start(&rippled), 18);
    assert_eq!(rippled.beat_sounds(), before.beat_sounds());
    assert_eq!(journal(&rippled), 1);
    // A Roll keeps the pair's duration: the owner does not move, and no
    // clock is needed.
    let (rolled, _) = edit(
        &before,
        combined(&before, intent(0, 0, 0, -2, SourceTrimPolicy::Ripple)),
    );
    assert_eq!(owner_start(&rolled), 20);
    assert_eq!(journal(&rolled), 0);
    // Overwrite fills the trimmed time, so the owner stays in place.
    let (overwritten, _) = edit(
        &before,
        combined(&before, intent(0, -2, 0, 0, SourceTrimPolicy::Overwrite)),
    );
    assert_eq!(owner_start(&overwritten), 20);
    assert_eq!(overwritten.beat_sounds(), before.beat_sounds());
}
