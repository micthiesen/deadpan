//! Timing-neutral semantic policy edits retain actual nested occurrence PCM.
use super::*;
use std::num::NonZeroU32;

fn repeated(name: &str, child: &str) -> BeatNode {
    let NodeKind::Hold { recipe: gap } = room_tone(2, audio_at_rate(512, 833, 44_100)).kind else {
        panic!()
    };
    let mut node = BeatNode::sequence(name, vec![]);
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(RevisionId::new(name).unwrap(), 2).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    node
}

fn voice(start: i64) -> BeatNode {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut node = source(rate, 2, 0..1);
    let NodeKind::Source { source } = &mut node.kind else {
        panic!()
    };
    let audio = audio_at_rate(start, start + 20_000, 44_100);
    source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
    source.audio = Some(audio);
    node
}

fn fixture(owned_and_partial: bool) -> ProjectDocument {
    let mut nodes = vec![
        ("lead", hold(1)),
        ("outer", repeated("outer", "inner")),
        ("inner", repeated("inner", "body")),
        (
            "body",
            BeatNode::sequence("Body", vec![id("before"), id("hold"), id("after")]),
        ),
        ("before", voice(3100)),
        ("hold", hold(2)),
        ("after", voice(8000)),
        ("tail", voice(15_000)),
    ];
    let overrides = if owned_and_partial {
        nodes.extend([
            (
                "owned-body",
                BeatNode::sequence(
                    "Owned body",
                    vec![id("owned-before"), id("owned-hold"), id("owned-after")],
                ),
            ),
            ("owned-before", voice(11_000)),
            ("owned-hold", hold(2)),
            ("owned-after", voice(21_000)),
        ]);
        BTreeMap::from([(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("inner").unwrap(),
                    ordinal: 1,
                },
                root: id("owned-body"),
            }])
            .unwrap(),
        )])
    } else {
        BTreeMap::new()
    };
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "outer", "tail"],
        nodes,
        overrides,
        audio_at_rate(0, 44_117, 44_100).span,
    );
    assert_eq!(document.duration().unwrap().frames(), 33);
    if !owned_and_partial {
        return document;
    }
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("old-policy-clock").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    for owner in ["after", "owned-after", "tail"] {
        assert!(
            wire["audio_bindings"]["bindings"]
                .as_object_mut()
                .unwrap()
                .remove(owner)
                .is_some()
        );
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn context(document: &ProjectDocument, outer: u32, inner: u32, owned: bool) -> SemanticContext {
    let repeats = [("outer", outer), ("inner", inner)].map(|(name, ordinal)| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(name)].kind else {
            panic!()
        };
        RepeatInstance {
            node: id(name),
            iteration: iterations.at(ordinal).unwrap(),
        }
    });
    let target = id(if owned && inner == 1 {
        "owned-hold"
    } else {
        "hold"
    });
    SemanticContext {
        original: None,
        parent: id("root"),
        selected_child: Some(id("outer")),
        cursor: ProjectFrame(4 + i64::from(outer) * 16 + i64::from(inner) * 8),
        visual_selection: None,
        scoped: Some(SemanticScopedContext {
            root: id("outer"),
            target: ScopedNodeTarget {
                node: target.clone(),
                repeats: repeats
                    .iter()
                    .map(|step| RepeatEditStep {
                        repeat: step.node.clone(),
                        branch: RepeatEditBranch::Play {
                            iteration: step.iteration.clone(),
                        },
                    })
                    .collect(),
            },
            presentation: InstancePath {
                node: target,
                repeats: repeats.into(),
            },
        }),
    }
}

fn policy(
    document: &ProjectDocument,
    context: &SemanticContext,
    audio: HoldAudio,
    name: &str,
) -> SemanticPlan {
    let a = RegisterName::new('a').unwrap();
    let instructions = vec![SemanticInstruction::SetHoldAudio { audio }];
    let bank = BTreeMap::from([(
        a,
        Arc::new(RegisterValue::Macro {
            program: Arc::new(SemanticProgram::new(instructions).unwrap()),
        }),
    )]);
    let plan = plan_semantic(
        document,
        context,
        &SemanticProgram::new(vec![SemanticInstruction::Call {
            register: a,
            count: NonZeroU32::new(1).unwrap(),
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        RevisionId::new(format!("{name}-outer")).unwrap(),
        |request| {
            let SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } = request
            else {
                panic!("{request:?}")
            };
            Ok(SemanticAllocation::Isolation {
                new_revision: RevisionId::new(format!("{name}-leaf-{step_index}")).unwrap(),
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| id(&format!("{name}-node-{step_index}-{n}")))
                        .collect(),
                    marks: (0..marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{step_index}-{n}")).unwrap())
                        .collect(),
                },
            })
        },
        |_, _| panic!("literal Hold policy needs no Original resolver"),
    )
    .unwrap();
    assert_eq!(plan.context.cursor, context.cursor);
    assert_eq!(plan.context.parent, context.parent);
    assert_eq!(plan.context.selected_child, context.selected_child);
    assert_eq!(
        plan.document.duration().unwrap(),
        document.duration().unwrap()
    );
    let transaction = apply(document, plan.request.as_ref().unwrap()).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(transaction.forward.apply(document).unwrap(), plan.document);
    assert_eq!(
        transaction.inverse.apply(&plan.document).unwrap(),
        *document
    );
    plan
}

fn render(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    chunks: &[u32],
) -> Vec<[f32; 2]> {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    // The common read_all helper deliberately asserts stage-revision;
    // semantic plans instead carry their actual staged revision identities.
    let total = usize::try_from(renderer.plan().audio_duration().unwrap().0).unwrap();
    let mut samples = Vec::new();
    for requested in chunks.iter().copied().cycle() {
        if samples.len() == total {
            break;
        }
        let block = renderer
            .read(
                provider,
                AudioSample(samples.len() as i64),
                requested.min((total - samples.len()) as u32),
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(&block.revision_id, document.revision_id());
        samples.extend(block.samples);
    }
    samples
}

fn boundary(frame: i64) -> usize {
    usize::try_from(ratio(i128::from(frame) * 8008, 5).round_even().unwrap()).unwrap()
}

#[test]
fn semantic_scoped_hold_audio_keeps_nested_44100_pcm_and_partial_owned_clocks_then_silences_only_that_play()
 {
    for owned in [false, true] {
        let before = fixture(owned);
        let mut provider = FixtureProvider::from_fixture(
            "pcm-mono-44100.wav",
            AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
        );
        let old = render(&before, &mut provider, &[256]);
        assert!(old.iter().flatten().any(|sample| sample.abs() > 0.01));
        for (outer, inner) in [(0, 0), (1, 1)] {
            let context = context(&before, outer, inner, owned);
            let selected = 3 + i64::from(outer) * 16 + i64::from(inner) * 8;
            let range = boundary(selected)..boundary(selected + 2);
            let source = audio_at_rate(100, 321, 44_100);
            let toned = policy(
                &before,
                &context,
                HoldAudio::RoomTone {
                    source: source.clone(),
                },
                "room-policy",
            );
            let mapped = toned.context.scoped.as_ref().unwrap();
            assert!(
                mapped
                    .target
                    .matches_instance(&toned.document, &mapped.presentation)
                    .unwrap()
            );
            let NodeKind::Hold { recipe } = &toned.document.nodes()[&mapped.target.node].kind
            else {
                panic!()
            };
            assert_eq!(
                recipe.audio,
                HoldAudio::RoomTone {
                    source: source.clone()
                }
            );
            let actual = render(&toned.document, &mut provider, &[127, 3, 251]);
            // A literal standalone owner at this authored absolute origin is
            // independent of isolation and preserves the short exact source
            // span's fractional 48 kHz loop period and physical sample phase.
            let reference = document_with_asset(
                FrameRate::new(30_000, 1001).unwrap(),
                &["lead", "room"],
                [("lead", hold(selected)), ("room", room_tone(2, source))],
                BTreeMap::new(),
                audio_at_rate(0, 44_117, 44_100).span,
            );
            let expected = render(&reference, &mut provider, &[199, 7]);
            assert_eq!(actual[range.clone()], expected[range.clone()]);
            assert!(
                actual[range.clone()]
                    .iter()
                    .flatten()
                    .any(|sample| sample.abs() > 0.01)
            );
            assert_eq!(actual[..range.start], old[..range.start]);
            assert_eq!(actual[range.end..], old[range.end..]);
            assert_eq!(render(&toned.document, &mut provider, &[1, 239]), actual);
            let silent = policy(
                &toned.document,
                &toned.context,
                HoldAudio::Silence,
                "silence-policy",
            );
            assert_eq!(
                silent.document.audio_bindings(),
                toned.document.audio_bindings()
            );
            let silenced = render(&silent.document, &mut provider, &[3, 251]);
            assert_eq!(silenced, old);
            let mut renderer =
                StageAudio::new(Arc::new(RenderPlan::compile(&silent.document).unwrap()));
            for start in (range.start..range.end).step_by(256) {
                let end = (start + 256).min(range.end);
                let block = renderer
                    .read(
                        &mut provider,
                        AudioSample(start as i64),
                        (end - start) as u32,
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert!(block.samples.iter().all(|sample| *sample == [0.; 2]));
                assert!((start..end).all(|sample| {
                    block
                        .suppressed
                        .iter()
                        .any(|range| range.contains(&AudioSample(sample as i64)))
                }));
            }
            assert_eq!(render(&silent.document, &mut provider, &[1, 239]), silenced);
        }
    }
}
