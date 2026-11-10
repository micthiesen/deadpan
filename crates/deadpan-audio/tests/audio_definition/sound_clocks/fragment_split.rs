//! Splitting a fragment (a Partition left by an earlier pause) that carries a
//! sting keeps the sting's complete recipe on both pieces behind delivery
//! crops: the mixed PCM is unchanged whether the sound lies before, after or
//! across the new cut, at 30000/1001.
use super::partial::{compare_copy, render};
use super::*;

#[test]
fn splitting_a_sting_fragment_leaves_its_samples_unchanged() {
    for treated in [false, true] {
        for (case, start) in [
            ("before", ExactRatio::ZERO),
            ("after", ExactRatio::integer(2)),
            ("spanning", ExactRatio::new(3, 2).unwrap()),
        ] {
            let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
            let mut sting = wire["beat_sounds"]["owner"].take();
            wire["beat_sounds"] = serde_json::json!({});
            let plain = ProjectDocument::from_json(&wire.to_string()).unwrap();
            // lead 0..2, owner 2..6: a pause at 5 leaves the fragment 2..5.
            let count = plain
                .insert_time_target(ProjectFrame(5))
                .unwrap()
                .split
                .unwrap()
                .required_ids;
            let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
                unreachable!()
            };
            let paused = edit(
                &plain,
                "paused",
                Command::InsertTime {
                    at: ProjectFrame(5),
                    hold: recipe,
                    id: id("pause"),
                    identities: SplitIdentities {
                        nodes: (0..count).map(|n| id(&format!("cut-{n}"))).collect(),
                    },
                    timing: AudioTimingId {
                        allocation: RevisionId::new("paused").unwrap(),
                        ordinal: 0,
                    },
                },
            );
            let fragment = id("cut-0");
            assert_eq!(paused.durations().unwrap()[&fragment].frames(), 3);
            let frames =
                serde_json::from_value::<SourceAudioMapping>(sting["effect"]["mapping"].clone())
                    .unwrap()
                    .duration_frames(FrameDuration::ZERO)
                    .unwrap();
            // One audible frame of the sound, starting `start` frames in.
            sting["effect"]["mapping"] =
                serde_json::to_value(SourceAudioMapping::SelectedPlacement {
                    start,
                    frames,
                    selection: ExactFrameRange {
                        start,
                        end: start.checked_add(ExactRatio::integer(1)).unwrap(),
                    },
                })
                .unwrap();
            if treated {
                let eq = EqualizerRecipe::new(
                    [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
                    None,
                    None,
                )
                .unwrap();
                sting["effect"]["treatments"] = serde_json::to_value(
                    AudioTreatments::default().with_equalizer(Some(eq)).unwrap(),
                )
                .unwrap();
            }
            let mut wire = serde_json::to_value(&paused).unwrap();
            wire["beat_sounds"] = serde_json::json!({ fragment.as_str(): sting });
            let attached = ProjectDocument::from_json(&wire.to_string()).unwrap();
            let split = edit(
                &attached,
                "split",
                Command::Split {
                    node: fragment.clone(),
                    at: FrameDuration::new(2).unwrap(),
                    identities: SplitIdentities {
                        nodes: (0..5).map(|n| id(&format!("split-{n}"))).collect(),
                    },
                },
            );
            assert_eq!(split.beat_sounds().len(), 2, "{case}");
            let original = render(&attached, 0, 9);
            assert!(
                original.iter().flatten().any(|v| v.abs() > 0.001),
                "{case}: the sting is audible"
            );
            compare_copy(&original, &render(&split, 0, 9), treated, case);
        }
    }
}
