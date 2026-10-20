use super::*;
use deadpan_core::{OccurrenceIdentities, OccurrencePauseIdentities, PauseInsertionTarget};

fn pause(before: &ProjectDocument, at: i64) -> ProjectDocument {
    let PauseInsertionTarget::Occurrence(target) =
        before.pause_insertion_target(ProjectFrame(at)).unwrap()
    else {
        panic!()
    };
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        panic!()
    };
    edit(
        before,
        "pause",
        Command::InsertOccurrenceTime {
            at: ProjectFrame(at),
            within: None,
            target: None,
            hold: recipe,
            id: id("pause"),
            timing: timing("pause"),
            ai: false,
            all_plays: false,
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: (0..target.isolation_nodes)
                        .map(|n| id(&format!("clone-{n}")))
                        .collect(),
                    marks: (0..target.isolation_marks)
                        .map(|n| deadpan_core::MarkId::new(format!("mark-{n}")).unwrap())
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..target.split_nodes)
                        .map(|n| id(&format!("split-{n}")))
                        .collect(),
                },
                container: target.container.then(|| id("container")),
                gap: target.gap.then(|| id("gap")),
            },
        },
    )
}

#[test]
fn occurrence_pause_transports_fresh_and_saved_beat_sound_processing() {
    for saved in [false, true] {
        for treated in [false, true] {
            let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
            if treated {
                let eq = EqualizerRecipe::new(
                    [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
                    None,
                    None,
                )
                .unwrap();
                let treatment = serde_json::to_value(
                    AudioTreatments::default().with_equalizer(Some(eq)).unwrap(),
                )
                .unwrap();
                wire["nodes"]["owner"]["audio_treatments"] = treatment.clone();
                wire["beat_sounds"]["owner"]["effect"]["treatments"] = treatment;
            }
            let base = ProjectDocument::from_json(&wire.to_string()).unwrap();
            let base = if saved { insert_prefix(&base) } else { base };
            let before = wrap(&base, "owner", "repeated");
            let lead = 2 + i64::from(saved);
            for at in [lead + 1, lead + 4, lead + 6] {
                let after = pause(&before, at);
                assert_eq!(
                    after.duration().unwrap().frames(),
                    before.duration().unwrap().frames() + 1
                );
                assert!(render(&after, at, at + 1).iter().all(|v| *v == [0.; 2]));
                for allocation in [lead..lead + 4, lead + 4..lead + 8] {
                    for range in [
                        allocation.start..allocation.end.min(at),
                        allocation.start.max(at)..allocation.end,
                    ] {
                        if range.start >= range.end {
                            continue;
                        }
                        let shift = i64::from(range.start >= at);
                        let expected = render(&before, range.start, range.end);
                        let actual = render(&after, range.start + shift, range.end + shift);
                        // The current silent pause deliberately adds edge fades.
                        // Compare the retained DSP history away from those gates.
                        let end = expected.len().min(actual.len()) - 512;
                        assert!(expected[512..end].iter().flatten().any(|v| v.abs() > 0.001));
                        super::super::partial::compare_copy(
                            &expected[512..end],
                            &actual[512..end],
                            treated,
                            &format!("saved={saved} treated={treated} at={at} range={range:?}"),
                        );
                    }
                }
            }
        }
    }
}
