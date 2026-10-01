use super::*;
use crate::*;
use serde_json::json;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn sound() -> SoundId {
    SoundId::new("sound").unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn selected(start: ExactRatio, end: ExactRatio) -> ExactFrameRange {
    ExactFrameRange::new(start, end).unwrap()
}
fn trim(start: i64, end: i64, i: i64, o: i64) -> RootSoundOperation {
    RootSoundOperation::Trim {
        range: range(start, end),
        in_frames: i,
        out_frames: o,
    }
}

fn fixture(rate: FrameRate, extent: i64, selection: ExactFrameRange) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("root-trim").unwrap(),
        RevisionId::new("base").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 48000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 480000,
            time_base,
        },
    )
    .unwrap();
    let mut wire = json!(empty);
    wire["assets"] = json!({"sound":AssetRecord {label:"Sound".into(),content_hash:"a".repeat(64),video:None,audio:Some(span),still_image:false,frame_count:None,source_qualification:Some(SourceQualificationId::new("b".repeat(64)).unwrap())}});
    wire["nodes"] = json!({"root":BeatNode::sequence("Root",vec![node("held")]),"held":BeatNode::hold("Held",HoldRecipe {duration:frames(extent),video:HoldVideo::Background,picture_context:None,audio:HoldAudio::Silence})});
    wire["sounds"] = json!({"sound":SoundEvent {owner:node("root"),label:"Exact recipe".into(),source:SourceAudio {asset:AssetId::new("sound").unwrap(),span},mapping:SourceAudioMapping::SelectedPlacement {start:ExactRatio::ZERO,frames:SourceAudioMapping::natural_rate(span,rate).unwrap().duration_frames(FrameDuration::ZERO).unwrap(),selection},offset:AudioSample(0),gain_millidecibels:-1234,start_edge:AudioEdgePolicy::Hard,end_edge:AudioEdgePolicy::Automatic,overflow:SoundOverflowPolicy::Reject}});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn resize(document: &mut ProjectDocument, extent: i64) {
    let NodeKind::Hold { recipe } = &mut document.nodes.get_mut(&node("held")).unwrap().kind else {
        panic!()
    };
    recipe.duration = frames(extent);
}

#[test]
fn identity_capture_restores_exact_objects_with_or_without_prior_routes() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for routed in [false, true] {
        let mut before = fixture(
            rate,
            6,
            selected(
                ExactRatio::ZERO,
                ExactRatio::integer(if routed { 5 } else { 6 }),
            ),
        );
        if routed {
            before.sound_routes.insert(
                sound(),
                RootSoundRoute {
                    recipe_extent: frames(5),
                    recipe_grid: RootSoundGrid::root(rate),
                    edits: vec![RootSoundEdit {
                        grid: RootSoundGrid::root(rate),
                        operation: RootSoundOperation::Insert {
                            at: ProjectFrame(1),
                            duration: frames(1),
                        },
                        cuts: RootSoundCutEdges {
                            before: AudioEdgePolicy::Hard,
                            after: AudioEdgePolicy::Automatic,
                        },
                    }],
                },
            );
        }
        before.sound_allowances.insert(
            sound(),
            SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: node("held"),
                    repeats: vec![],
                },
            }])
            .unwrap(),
        );
        before.validate().unwrap();
        let capture = RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 0, 0)).unwrap();
        let allowances = crate::sound_allowance::SoundAllowanceEdit::capture(
            &before,
            &Command::Rename {
                node: node("root"),
                label: "Root".into(),
            },
        )
        .unwrap();
        let mut detached = capture.structural_document(&before);
        assert!(
            detached.sounds.is_empty()
                && detached.sound_routes.is_empty()
                && detached.sound_allowances.is_empty()
        );
        capture.restore(&mut detached).unwrap();
        allowances.restore(&mut detached).unwrap();
        assert_eq!(detached, before);
    }
}

#[test]
fn one_append_keeps_prior_recipe_grid_policies_and_exact_qualified_recipe() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let mut before = fixture(rate, 7, selected(ExactRatio::ZERO, ExactRatio::integer(6)));
    let previous = RootSoundRoute {
        recipe_extent: frames(6),
        recipe_grid: RootSoundGrid::root(rate),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(rate),
            operation: RootSoundOperation::Insert {
                at: ProjectFrame(1),
                duration: frames(1),
            },
            cuts: RootSoundCutEdges {
                before: AudioEdgePolicy::Hard,
                after: AudioEdgePolicy::Hard,
            },
        }],
    };
    before.sound_routes.insert(sound(), previous.clone());
    before.validate().unwrap();
    let capture = RootSoundEditCapture::prepare_operation(&before, trim(2, 4, 1, 1)).unwrap();
    let mut after = capture.structural_document(&before);
    capture.restore(&mut after).unwrap();
    after.validate().unwrap();
    assert_eq!(after.sounds, before.sounds);
    assert_eq!(
        after.sound_routes[&sound()].recipe_extent,
        previous.recipe_extent
    );
    assert_eq!(
        after.sound_routes[&sound()].recipe_grid,
        previous.recipe_grid
    );
    assert_eq!(after.sound_routes[&sound()].edits[..1], previous.edits);
    assert_eq!(after.sound_routes[&sound()].edits.len(), 2);
    assert_eq!(
        after.sound_routes[&sound()].edits[1].operation,
        trim(2, 4, 1, 1)
    );
    assert_eq!(before.sound_routes[&sound()], previous);
}

#[test]
fn nonidentity_same_duration_removes_only_exhausted_event_and_allowances() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let mut before = fixture(
        rate,
        6,
        selected(ExactRatio::integer(1), ExactRatio::integer(2)),
    );
    let retained = SoundId::new("retained").unwrap();
    let mut event = before.sounds[&sound()].clone();
    let SourceAudioMapping::SelectedPlacement { selection, .. } = &mut event.mapping else {
        panic!()
    };
    *selection = selected(ExactRatio::integer(4), ExactRatio::integer(5));
    before.sounds.insert(retained.clone(), event.clone());
    let allowance = SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node("held"),
            repeats: vec![],
        },
    }])
    .unwrap();
    before.sound_allowances.insert(sound(), allowance.clone());
    before
        .sound_allowances
        .insert(retained.clone(), allowance.clone());
    let allowances = crate::sound_allowance::SoundAllowanceEdit::capture(
        &before,
        &Command::Rename {
            node: node("root"),
            label: "Root".into(),
        },
    )
    .unwrap();
    let capture = RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 1, 1)).unwrap();
    let mut after = capture.structural_document(&before);
    capture.restore(&mut after).unwrap();
    allowances.restore(&mut after).unwrap();
    after.validate().unwrap();
    assert!(!after.sounds.contains_key(&sound()));
    assert!(!after.sound_routes.contains_key(&sound()));
    assert!(!after.sound_allowances.contains_key(&sound()));
    assert_eq!(after.sounds[&retained], event);
    assert_eq!(after.sound_allowances[&retained], allowance);
    assert_eq!(after.sound_routes[&retained].edits.len(), 1);
}

#[test]
fn initially_sampleless_intent_survives_only_kept_logical_support() {
    let rate = FrameRate::new(32000, 1).unwrap();
    let before = fixture(
        rate,
        3,
        selected(
            ExactRatio::new(21, 10).unwrap(),
            ExactRatio::new(11, 5).unwrap(),
        ),
    );
    let capture = RootSoundEditCapture::prepare_operation(&before, trim(0, 1, -1, 0)).unwrap();
    let mut after = capture.structural_document(&before);
    resize(&mut after, 4);
    capture.restore(&mut after).unwrap();
    after.validate().unwrap();
    assert_eq!(after.sounds, before.sounds);
    let removed = RootSoundEditCapture::prepare_operation(&before, trim(2, 3, 1, 1)).unwrap();
    assert!(removed.sounds.is_empty() && removed.routes.is_empty());
}

#[test]
fn exhausted_physical_sample_does_not_resurrect_as_sampleless_intent() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let selection = selected(
        ExactRatio::new(9609 * 5, 8008).unwrap(),
        ExactRatio::integer(6),
    );
    let before = fixture(rate, 6, selection);
    let journal = RootSoundRoute {
        recipe_extent: frames(6),
        recipe_grid: RootSoundGrid::root(rate),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(rate),
            operation: trim(1, 3, 1, 0),
            cuts: Default::default(),
        }],
    };
    assert!(retains_logical_selection(selection, &journal).unwrap());
    assert!(!retains_selection(&before.sounds[&sound()], &journal).unwrap());
    let removed = RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 1, 0)).unwrap();
    assert!(removed.sounds.is_empty() && removed.routes.is_empty());
    let retained = RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 1, 1)).unwrap();
    assert_eq!(retained.sounds, before.sounds);
}

#[test]
fn capacity_failure_and_duration_mismatch_leave_original_capture_untouched() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let mut before = fixture(rate, 6, selected(ExactRatio::ZERO, ExactRatio::integer(6)));
    before.sound_routes.insert(
        sound(),
        RootSoundRoute {
            recipe_extent: frames(6),
            recipe_grid: RootSoundGrid::root(rate),
            edits: vec![
                RootSoundEdit {
                    grid: RootSoundGrid::root(rate),
                    operation: trim(1, 3, 0, 0),
                    cuts: Default::default()
                };
                MAX_ROOT_SOUND_EDITS
            ],
        },
    );
    before.validate().unwrap();
    let unchanged = before.clone();
    assert!(RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 0, 0)).is_ok());
    let error = match RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 0, 1)) {
        Err(error) => error,
        Ok(_) => panic!("overlong journal accepted"),
    };
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert_eq!(before, unchanged);
    let before = fixture(rate, 6, selected(ExactRatio::ZERO, ExactRatio::integer(6)));
    let capture = RootSoundEditCapture::prepare_operation(&before, trim(1, 3, 0, 1)).unwrap();
    let mut wrong = capture.structural_document(&before);
    assert!(capture.restore(&mut wrong).is_err());
    assert!(wrong.sounds.is_empty() && wrong.sound_routes.is_empty());
}
