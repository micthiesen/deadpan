use super::*;
#[path = "duration/selected_duration.rs"]
mod selected_duration;

struct DurationFixture {
    document: ProjectDocument,
    provider: FixtureProvider,
    allocations: Vec<Range<i64>>,
    overrides: bool,
}

/// Literal layout: lead 1; two outer bodies of (two inner bodies of
/// Source 2 / Hold 2 / Source 2, separated by room tone 2; room tone 2),
/// separated by room tone 2; Source tail 4. There is no trailing Repeat gap.
fn duration_fixture(room: bool, overrides: bool, captured: bool) -> DurationFixture {
    let (base, provider) = mono_original(4);
    let mut wire = serde_json::to_value(&base).unwrap();
    let gap = audio_at_rate(321, 987, 44_100);
    wire["nodes"]["lead"] = serde_json::to_value(hold(1)).unwrap();
    wire["nodes"]["tail"] = wire["nodes"]["source"].take();
    wire["nodes"].as_object_mut().unwrap().remove("source");
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "repeat", "tail"]);
    for (prefix, audio_start) in [
        ("", 100),
        ("alt-", 4000),
        ("owned-", 8000),
        ("owned-alt-", 12_000),
    ] {
        if !overrides && !prefix.is_empty() {
            continue;
        }
        let name = |suffix: &str| format!("{prefix}{suffix}");
        for (suffix, start) in [("before", audio_start), ("after", audio_start + 3000)] {
            let mut leaf = base.nodes()[&id("source")].clone();
            let NodeKind::Source { source } = &mut leaf.kind else {
                panic!()
            };
            let audio = audio_at_rate(start, start + 20_000, 44_100);
            source.duration = duration(2);
            source.audio_mapping =
                SourceAudioMapping::natural_rate(audio.span, base.presentation_basis().frame_rate)
                    .unwrap();
            source.audio = Some(audio);
            wire["nodes"][name(suffix)] = serde_json::to_value(leaf).unwrap();
        }
        wire["nodes"][name("hold")] = serde_json::to_value(if room {
            room_tone(2, gap.clone())
        } else {
            hold(2)
        })
        .unwrap();
        wire["nodes"][name("inner-body")] = serde_json::to_value(BeatNode::sequence(
            "Source / Hold / Source",
            ["before", "hold", "after"]
                .map(|suffix| id(&name(suffix)))
                .into(),
        ))
        .unwrap();
    }
    for prefix in if overrides {
        vec!["", "owned-"]
    } else {
        vec![""]
    } {
        let name = |suffix: &str| format!("{prefix}{suffix}");
        wire["nodes"][name("inner")] =
            serde_json::to_value(repeated(&name("inner-body"), 2, &gap)).unwrap();
        wire["nodes"][name("room")] = serde_json::to_value(room_tone(2, gap.clone())).unwrap();
        wire["nodes"][name("outer-body")] = serde_json::to_value(BeatNode::sequence(
            "Outer body",
            vec![id(&name("inner")), id(&name("room"))],
        ))
        .unwrap();
        if overrides {
            wire["overrides"][name("inner")] = serde_json::to_value(
                PlayOverrides::try_from(vec![PlayOverride {
                    iteration: IterationId {
                        allocation: RevisionId::new(name("inner-body")).unwrap(),
                        ordinal: 1,
                    },
                    root: id(&name("alt-inner-body")),
                }])
                .unwrap(),
            )
            .unwrap();
        }
    }
    wire["nodes"]["repeat"] = serde_json::to_value(repeated("outer-body", 2, &gap)).unwrap();
    if overrides {
        wire["overrides"]["repeat"] = serde_json::to_value(
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("outer-body").unwrap(),
                    ordinal: 1,
                },
                root: id("owned-outer-body"),
            }])
            .unwrap(),
        )
        .unwrap();
    }
    let mut document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    if captured {
        let bindings = capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("duration-capture").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        let mut wire = serde_json::to_value(&document).unwrap();
        wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
        document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        document = split_command(&document, &id("before"), 1, "duration-fragment");
        // A partially bound package is valid. The temporal edit must capture
        // this still-unbound downstream Source before it changes root time.
        let mut wire = serde_json::to_value(&document).unwrap();
        wire["audio_bindings"]["bindings"]
            .as_object_mut()
            .unwrap()
            .remove("tail");
        document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert!(
            !document
                .audio_bindings()
                .bindings()
                .contains_key(&id("tail"))
        );
    }
    let mut allocations: Vec<_> = std::iter::once(0..1).collect();
    for outer in 0..2 {
        let start = 1 + outer * 18;
        for inner in 0..2 {
            let start = start + inner * 8;
            if captured && (!overrides || outer == 0) && (!overrides || inner == 0) {
                allocations.extend([start..start + 1, start + 1..start + 2]);
            } else {
                allocations.push(start..start + 2);
            }
            allocations.extend([start + 2..start + 4, start + 4..start + 6]);
            if inner == 0 {
                allocations.push(start + 6..start + 8);
            }
        }
        allocations.push(start + 14..start + 16);
        if outer == 0 {
            allocations.push(start + 16..start + 18);
        }
    }
    allocations.push(35..39);
    assert_eq!(document.duration().unwrap().frames(), 39);
    DurationFixture {
        document,
        provider,
        allocations,
        overrides,
    }
}

fn duration_instance(fixture: &DurationFixture, outer: u32, inner: u32) -> InstancePath {
    let prefix = if fixture.overrides && outer == 1 {
        "owned-"
    } else {
        ""
    };
    let inner_name = format!("{prefix}inner");
    let hold_prefix = if fixture.overrides && inner == 1 {
        format!("{prefix}alt-")
    } else {
        prefix.into()
    };
    let iteration = |name: &str, ordinal| {
        let NodeKind::Repeat { iterations, .. } = &fixture.document.nodes()[&id(name)].kind else {
            panic!()
        };
        RepeatInstance {
            node: id(name),
            iteration: iterations.at(ordinal).unwrap(),
        }
    };
    InstancePath {
        node: id(&format!("{hold_prefix}hold")),
        repeats: vec![iteration("repeat", outer), iteration(&inner_name, inner)],
    }
}

fn resize_occurrence(
    document: &ProjectDocument,
    instance: InstancePath,
    frames: i64,
) -> ProjectDocument {
    let required = document
        .occurrence_hold_duration_requirements(&instance, duration(frames))
        .unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("duration-resize").unwrap(),
        command: Command::EditOccurrence {
            instance,
            edit: OccurrenceEdit::SetHoldDuration {
                duration: duration(frames),
            },
            identities: OccurrenceIdentities {
                nodes: (0..required.nodes)
                    .map(|n| id(&format!("duration-isolate-{n}")))
                    .collect(),
                marks: (0..required.marks)
                    .map(|n| MarkId::new(format!("duration-mark-{n}")).unwrap())
                    .collect(),
            },
        },
    };
    let transaction = apply(document, &request).unwrap();
    assert_eq!(transaction.duration_delta, frames - 2);
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn check_duration_pcm(room: bool) {
    for (overrides, captured) in [(false, false), (true, true)] {
        let mut fixture = duration_fixture(room, overrides, captured);
        let old = record(&fixture.document, &mut fixture.provider, false, &[256]);
        assert!(
            old.samples
                .iter()
                .flatten()
                .any(|sample| sample.abs() > 0.01)
        );
        for outer in 0..2 {
            for inner in 0..2 {
                for frames in [1, 3] {
                    let start = 3 + i64::from(outer) * 18 + i64::from(inner) * 8;
                    let selected = start..start + 2;
                    let after = resize_occurrence(
                        &fixture.document,
                        duration_instance(&fixture, outer, inner),
                        frames,
                    );
                    let actual = record(&after, &mut fixture.provider, false, &[127, 3, 251]);
                    assert_eq!(actual.samples.len(), ntsc_boundary(37 + frames));
                    let label = format!(
                        "room={room} overrides={overrides} captured={captured} play={outer}/{inner} frames={frames}"
                    );
                    for allocation in &fixture.allocations {
                        if *allocation == selected {
                            continue;
                        }
                        let shift = if allocation.start >= selected.end {
                            frames - 2
                        } else {
                            0
                        };
                        assert_moved(
                            &actual,
                            &old,
                            allocation.start,
                            allocation.end,
                            shift,
                            false,
                            &label,
                        );
                        let before = ntsc_boundary(allocation.start)..ntsc_boundary(allocation.end);
                        let moved = ntsc_boundary(allocation.start + shift)
                            ..ntsc_boundary(allocation.end + shift);
                        for (offset, sample) in moved.enumerate() {
                            let previous = before.start + offset;
                            assert_eq!(
                                actual.suppressed[sample],
                                if previous < before.end {
                                    old.suppressed[previous]
                                } else {
                                    true
                                },
                                "{label} suppression at {sample}"
                            );
                        }
                    }
                    if room {
                        // A longer room-tone Hold changes its endpoint filter
                        // support. Compare a fresh recipe at the old absolute
                        // origin, rather than freezing the old shorter output.
                        let reference = document_with_asset(
                            fixture.document.presentation_basis().frame_rate,
                            &["quiet", "room"],
                            [
                                ("quiet", hold(start)),
                                ("room", room_tone(frames, audio_at_rate(321, 987, 44_100))),
                            ],
                            BTreeMap::new(),
                            audio_at_rate(0, 44_117, 44_100).span,
                        );
                        let expected = record(&reference, &mut fixture.provider, false, &[199, 7]);
                        assert_resumed(
                            &actual,
                            &expected,
                            ntsc_boundary(start)..ntsc_boundary(start + frames),
                            ntsc_boundary(start),
                        );
                    } else {
                        assert_pause(&actual, ntsc_boundary(start)..ntsc_boundary(start + frames));
                    }
                    assert_eq!(
                        record(&after, &mut fixture.provider, false, &[1, 239]),
                        actual,
                        "{label} chunk independence"
                    );
                }
            }
        }
    }
}

#[test]
fn explicit_nested_play_hold_duration_keeps_44100_source_gap_and_unbound_suffix_pcm() {
    check_duration_pcm(false);
}

#[test]
fn explicit_nested_play_room_tone_duration_keeps_phase_and_current_endpoint_support() {
    check_duration_pcm(true);
}
