//! A semantic two-body duration Compound must preserve both old entry clocks.
use super::*;
use std::num::NonZeroU32;

fn selected_plan(
    fixture: &DurationFixture,
    shared_outer: bool,
    last_primary: bool,
    frames: i64,
) -> SemanticPlan {
    let order = if last_primary { [1, 0] } else { [0, 1] };
    let instances = order.map(|inner| duration_instance(fixture, 0, inner));
    let target = |instance: &InstancePath| ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .enumerate()
            .map(|(index, step)| RepeatEditStep {
                repeat: step.node.clone(),
                branch: if shared_outer && index == 0 {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    }
                },
            })
            .collect(),
    };
    let context = SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: id("repeat"),
            target: target(&instances[0]),
            also: vec![target(&instances[1])],
            presentation: Some(instances[0].clone()),
        })),
        parent: id("root"),
        cursor: ProjectFrame(if last_primary { 12 } else { 4 }),
        selected_child: Some(id("repeat")),
        visual_selection: None,
    };
    let planned = plan_semantic(
        &fixture.document,
        &context,
        &SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(u32::try_from(frames).unwrap()).unwrap(),
            },
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        RevisionId::new("selected-pcm-final").unwrap(),
        |request| {
            let SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } = request
            else {
                panic!("duration allocation");
            };
            Ok(SemanticAllocation::Isolation {
                new_revision: RevisionId::new(format!("selected-pcm-{step_index}")).unwrap(),
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| id(&format!("selected-pcm-{step_index}-node-{n}")))
                        .collect(),
                    marks: (0..marks)
                        .map(|n| {
                            MarkId::new(format!("selected-pcm-{step_index}-mark-{n}")).unwrap()
                        })
                        .collect(),
                },
            })
        },
        |_, _| panic!("no Original resolver needed"),
    )
    .unwrap();
    let request = planned.request.as_ref().unwrap();
    let replay = replay_compound::<EditError>(&fixture.document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        fixture.document
    );
    assert_eq!(
        ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
        planned.document
    );
    let proof = derive_scoped_isolation(&fixture.document, request, &planned.document).unwrap();
    let mapped = planned.context.selected_hold_duration.as_ref().unwrap();
    let original = context.selected_hold_duration.as_ref().unwrap();
    assert_eq!(proof.map_forward(&original.target).unwrap(), mapped.target);
    assert_eq!(
        proof.map_forward(&original.also[0]).unwrap(),
        mapped.also[0]
    );
    assert_eq!(
        proof.map_instance_forward(&instances[0]).unwrap(),
        *mapped.presentation.as_ref().unwrap()
    );
    assert_eq!(
        mapped.target.repeats[0].branch == RepeatEditBranch::Default,
        shared_outer
    );
    // Two inner Holds originally start3 and11. The second starts earlier or
    // later after the first resize, independent of which was the primary.
    assert_eq!(
        planned.context.cursor,
        ProjectFrame(if last_primary { 11 + frames - 2 } else { 3 })
    );
    planned
}

fn check_selected_pcm(room: bool) {
    // This existing fixture captures every old clock, splits the default
    // Source into historical fragments, then removes ONLY the tail binding.
    // The two inner bodies include an independent owned play; outer play1 is
    // independently owned and must keep all its Source/gap/RoomTone phases.
    let mut fixture = duration_fixture(room, true, true);
    let old = record(&fixture.document, &mut fixture.provider, false, &[256]);
    assert!(old.samples.iter().flatten().any(|v| v.abs() > 0.01));
    assert!(
        !fixture
            .document
            .audio_bindings()
            .bindings()
            .contains_key(&id("tail"))
    );
    let selected = [3..5, 11..13];
    for shared_outer in [false, true] {
        for last_primary in [false, true] {
            for frames in [1, 3] {
                let planned = selected_plan(&fixture, shared_outer, last_primary, frames);
                let actual = record(
                    &planned.document,
                    &mut fixture.provider,
                    false,
                    &[127, 3, 251],
                );
                assert_eq!(actual.samples.len(), ntsc_boundary(39 + 2 * (frames - 2)));
                let label = format!(
                    "selected room={room} shared_outer={shared_outer} last_primary={last_primary} frames={frames}"
                );
                for allocation in &fixture.allocations {
                    if selected.contains(allocation) {
                        continue;
                    }
                    // Literal authored cuts, never inferred from the changed
                    // plan or an event-alignment search.
                    let shift = i64::try_from(
                        selected
                            .iter()
                            .filter(|hold| hold.end <= allocation.start)
                            .count(),
                    )
                    .unwrap()
                        * (frames - 2);
                    assert_moved(
                        &actual,
                        &old,
                        allocation.start,
                        allocation.end,
                        shift,
                        false,
                        &label,
                    );
                    let previous = ntsc_boundary(allocation.start)..ntsc_boundary(allocation.end);
                    let moved = ntsc_boundary(allocation.start + shift)
                        ..ntsc_boundary(allocation.end + shift);
                    for (offset, sample) in moved.enumerate() {
                        assert_eq!(
                            actual.suppressed[sample],
                            if previous.start + offset < previous.end {
                                old.suppressed[previous.start + offset]
                            } else {
                                true
                            },
                            "{label} suppression {sample}"
                        );
                    }
                }
                for (index, interval) in selected.iter().enumerate() {
                    let shift = i64::try_from(index).unwrap() * (frames - 2);
                    let allocated = ntsc_boundary(interval.start + shift)
                        ..ntsc_boundary(interval.start + shift + frames);
                    if room {
                        // The changed Hold has current endpoint support but
                        // retains the old absolute entry and natural44.1k loop.
                        let reference = document_with_asset(
                            fixture.document.presentation_basis().frame_rate,
                            &["quiet", "room"],
                            [
                                ("quiet", hold(interval.start)),
                                ("room", room_tone(frames, audio_at_rate(321, 987, 44_100))),
                            ],
                            BTreeMap::new(),
                            audio_at_rate(0, 44_117, 44_100).span,
                        );
                        let expected = record(&reference, &mut fixture.provider, false, &[199, 7]);
                        assert_resumed(
                            &actual,
                            &expected,
                            allocated,
                            ntsc_boundary(interval.start),
                        );
                    } else {
                        assert_pause(&actual, allocated);
                    }
                }
                assert_eq!(
                    record(&planned.document, &mut fixture.provider, false, &[1, 239]),
                    actual,
                    "{label} chunk independence"
                );
            }
        }
    }
}

#[test]
fn selected_hold_duration_compound_preserves_partial_44100_source_and_gap_clocks() {
    check_selected_pcm(false);
}

#[test]
fn selected_room_tone_duration_compound_preserves_phase_and_current_endpoints() {
    check_selected_pcm(true);
}
