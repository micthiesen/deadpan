//! A sting attached to a beat moves with that beat: its complete PCM lands
//! at the beat's new boundary, shifted by exactly the difference between the
//! old and new absolute sample boundaries at 30000/1001.
use super::partial::{compare_copy, render};
use super::*;

fn move_owner(document: &ProjectDocument, name: &str, from: i64, index: usize) -> ProjectDocument {
    edit(
        document,
        name,
        Command::MoveRange {
            source_revision: document.revision_id().clone(),
            source_parent: document.root().clone(),
            range: FrameRange::new(ProjectFrame(from), ProjectFrame(from + 4)).unwrap(),
            destination: deadpan_core::MoveRangeDestination::Seam {
                parent: document.root().clone(),
                index,
            },
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new(name).unwrap(),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn a_moved_sting_beat_plays_its_complete_sound_at_the_new_boundary() {
    for treated in [false, true] {
        let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
        if treated {
            let eq = EqualizerRecipe::new(
                [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
                None,
                None,
            )
            .unwrap();
            wire["beat_sounds"]["owner"]["effect"]["treatments"] =
                serde_json::to_value(AudioTreatments::default().with_equalizer(Some(eq)).unwrap())
                    .unwrap();
        }
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        // lead 0..2, owner 2..6, tail 6..8: put the sting beat after the tail.
        let later = move_owner(&before, "later", 2, 3);
        assert_eq!(later.beat_sounds(), before.beat_sounds());
        let original = render(&before, 2, 6);
        assert!(original.iter().flatten().any(|v| v.abs() > 0.001));
        compare_copy(&original, &render(&later, 4, 8), treated, "moved later");
        // Nothing of the sting remains at its old position.
        assert!(render(&later, 0, 4).iter().flatten().all(|v| *v == 0.0));
        // Move it back to the front: a second retained clock, same sound.
        let front = move_owner(&later, "front", 4, 0);
        compare_copy(&original, &render(&front, 0, 4), treated, "moved to front");
        assert!(render(&front, 4, 8).iter().flatten().all(|v| *v == 0.0));
    }
}
