use std::collections::BTreeMap;

use deadpan_core::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
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

fn source(start: i64, end: i64) -> SourceAudio {
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: span(start, end, 44_100),
    }
}

fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}

fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(document, command);
    let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&serde_json::to_string(&transaction).unwrap())
            .unwrap(),
        transaction
    );
    (after, transaction)
}

fn fixture() -> ProjectDocument {
    fixture_with_audio(88_200)
}

/// The fixture with an Original `ticks` long at 44.1 kHz.
fn fixture_with_audio(ticks: i64) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("hold-audio").unwrap(),
        RevisionId::new("r").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let document = edit(
        &document,
        Command::ImportSource {
            id: source(0, 88_200).asset,
            asset: AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(span(0, ticks, 44_100)),
                audio: Some(span(0, ticks, 44_100)),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
            insertion: None,
            primary: None,
        },
    )
    .0;
    let mut hold = BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: frames(60),
            video: HoldVideo::Freeze {
                asset: source(0, 1).asset,
                timestamp: span(37, 38, 44_100).start(),
            },
            audio: HoldAudio::Silence,
            picture_context: Some(
                CapturedFraming::new(vec![CapturedCanvas {
                    width: 320,
                    height: 240,
                    fit: CapturedFit::Fill,
                    layers: vec![Some(FramingPose::identity())],
                }])
                .unwrap(),
            ),
        },
    );
    hold.framing = Some(Framing::static_pose(FramingPose::identity()).unwrap());
    let document = edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(node("hold"), hold)]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    edit(
        &document,
        Command::SetMark {
            id: MarkId::new("inside-pause").unwrap(),
            owner: node("hold"),
            label: "Pause point".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("hold"),
                    position: ExactRatio::new(61, 2).unwrap(),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
    .0
}

#[test]
fn exact_room_tone_and_silence_preserve_picture_timing_and_round_trip() {
    let before = fixture();
    let selected = source(137, 24_691);
    let (room, transaction) = edit(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::RoomTone {
                source: selected.clone(),
            },
        },
    );
    let mut expected = before.nodes()[&node("hold")].clone();
    let NodeKind::Hold { recipe } = &mut expected.kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::RoomTone { source: selected };
    assert_eq!(room.nodes()[&node("hold")], expected);
    assert_eq!(room.duration().unwrap(), before.duration().unwrap());
    assert_eq!(room.presentation_basis(), before.presentation_basis());
    assert_eq!(room.basis_state(), before.basis_state());
    assert_eq!(room.assets(), before.assets());
    assert_eq!(room.marks(), before.marks());
    assert_eq!(room.audio_bindings(), before.audio_bindings());
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(transaction.changed_ids, vec![node("hold")]);
    assert!(transaction.forward.audio_bindings.is_none());
    let (silent, _) = edit(
        &room,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::Silence,
        },
    );
    assert_eq!(silent.nodes(), before.nodes());
    assert_eq!(silent.audio_bindings(), before.audio_bindings());
}

#[test]
fn policy_replacement_keeps_retained_sampling_lattice() {
    let document = fixture();
    let binding = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("captured").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(&binding).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let (changed, transaction) = edit(
        &bound,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::RoomTone {
                source: source(137, 24_691),
            },
        },
    );
    assert_eq!(changed.audio_bindings(), &binding);
    assert!(transaction.forward.audio_bindings.is_none());
    assert_eq!(changed.duration().unwrap(), bound.duration().unwrap());
}

#[test]
fn invalid_audio_and_targets_fail_without_mutating_the_document() {
    let before = fixture();
    let cases = [
        SourceAudio {
            asset: AssetId::new("missing").unwrap(),
            ..source(0, 100)
        },
        source(-1, 100),
        source(88_199, 88_201),
        SourceAudio {
            span: span(0, 100, 48_000),
            ..source(0, 100)
        },
    ];
    let encoded = before.to_json().unwrap();
    for selected in cases {
        let request = request(
            &before,
            Command::SetHoldAudio {
                node: node("hold"),
                audio: HoldAudio::RoomTone { source: selected },
            },
        );
        assert!(apply(&before, &request).is_err());
        assert_eq!(before.to_json().unwrap(), encoded);
    }
    for target in ["root", "missing"] {
        assert!(
            apply(
                &before,
                &request(
                    &before,
                    Command::SetHoldAudio {
                        node: node(target),
                        audio: HoldAudio::Silence,
                    }
                )
            )
            .is_err()
        );
    }
    let mut wire = serde_json::to_value(request(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::RoomTone {
                source: source(137, 24_691),
            },
        },
    ))
    .unwrap();
    wire["command"]["audio"]["source"]["span"]["end"]["ticks"] = 137.into();
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
}

#[test]
fn existing_tail_validation_and_revision_guards_still_apply() {
    let before = fixture();
    let audio = HoldAudio::Tail {
        maximum: frames(60),
        effect: Default::default(),
    };
    let (tail, _) = edit(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: audio.clone(),
        },
    );
    assert_eq!(tail.duration().unwrap(), before.duration().unwrap());
    assert!(
        apply(
            &before,
            &request(
                &before,
                Command::SetHoldAudio {
                    node: node("hold"),
                    audio: HoldAudio::Tail {
                        maximum: frames(61),
                        effect: Default::default(),
                    },
                }
            )
        )
        .is_err()
    );
    let stale = request(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio,
        },
    );
    assert_eq!(
        apply(&tail, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
}

#[test]
fn shortening_a_tail_hold_shortens_its_ring_and_new_providers_validate() {
    let before = fixture();
    let (tail, _) = edit(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::Tail {
                maximum: frames(60),
                effect: TailEffect::Delay,
            },
        },
    );
    let (short, _) = edit(
        &tail,
        Command::SetHoldDuration {
            node: node("hold"),
            duration: frames(20),
        },
    );
    let NodeKind::Hold { recipe } = &short.nodes()[&node("hold")].kind else {
        panic!("still a Hold")
    };
    assert_eq!(
        recipe.audio,
        HoldAudio::Tail {
            maximum: frames(20),
            effect: TailEffect::Delay,
        }
    );
    // Reversed audio and tones validate; an inaudible or boosted tone does not.
    for (audio, valid) in [
        (
            HoldAudio::Reverse {
                source: source(100, 1000),
            },
            true,
        ),
        (
            HoldAudio::Tone {
                frequency_hz: 1_000,
                level: GainDb::new(-10_000).unwrap(),
            },
            true,
        ),
        (
            HoldAudio::Tone {
                frequency_hz: 10,
                level: GainDb::new(-10_000).unwrap(),
            },
            false,
        ),
        (
            HoldAudio::Tone {
                frequency_hz: 1_000,
                level: GainDb::new(3_000).unwrap(),
            },
            false,
        ),
    ] {
        let applied = apply(
            &before,
            &request(
                &before,
                Command::SetHoldAudio {
                    node: node("hold"),
                    audio,
                },
            ),
        );
        assert_eq!(applied.is_ok(), valid);
    }
}

#[test]
fn a_reversed_span_is_bounded_by_the_one_block_dsp_limit() {
    let before = fixture_with_audio(44_100 * 30);
    // 1,048,576 mix samples are 963,379.2 ticks at 44.1 kHz; the extent
    // is rounded up to whole mix samples.
    for (end, valid) in [(963_379, true), (963_380, false), (44_100 * 30, false)] {
        let applied = apply(
            &before,
            &request(
                &before,
                Command::SetHoldAudio {
                    node: node("hold"),
                    audio: HoldAudio::Reverse {
                        source: source(0, end),
                    },
                },
            ),
        );
        assert_eq!(applied.is_ok(), valid, "{end}");
    }
}

#[test]
fn a_tail_cannot_be_wrapped_in_or_moved_into_a_speed_change() {
    let before = fixture();
    let (tail, _) = edit(
        &before,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::Tail {
                maximum: frames(30),
                effect: TailEffect::Reverb,
            },
        },
    );
    for pitch in [PitchPolicy::Preserve, PitchPolicy::FollowSpeed] {
        let wrap = request(
            &tail,
            Command::WrapRetime {
                node: node("hold"),
                id: node("speed"),
                duration: frames(120),
                pitch,
            },
        );
        let error = apply(&tail, &wrap).unwrap_err();
        assert!(error.message.contains("speed change"), "{error:?}");
    }
    // A unity Retime keeps the edit clock, so the tail may stay inside it,
    // but making it a speed change is refused.
    let (unity, _) = edit(
        &tail,
        Command::WrapRetime {
            node: node("hold"),
            id: node("speed"),
            duration: frames(60),
            pitch: PitchPolicy::Preserve,
        },
    );
    let faster = request(
        &unity,
        Command::SetRetime {
            node: node("speed"),
            duration: frames(30),
            pitch: PitchPolicy::Preserve,
        },
    );
    assert!(apply(&unity, &faster).is_err());
    // A silent pause inside a speed change cannot become a tail either.
    let (slowed, _) = edit(
        &before,
        Command::WrapRetime {
            node: node("hold"),
            id: node("speed"),
            duration: frames(120),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let set = request(
        &slowed,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::Tail {
                maximum: frames(30),
                effect: TailEffect::Reverb,
            },
        },
    );
    assert!(apply(&slowed, &set).is_err());
}
