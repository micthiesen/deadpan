use super::partial::{compare_copy, render};
use super::*;

fn pause(document: &ProjectDocument, name: &str, at: i64) -> ProjectDocument {
    let count = document
        .insert_time_target(ProjectFrame(at))
        .unwrap()
        .split
        .unwrap()
        .required_ids;
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    edit(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: recipe,
            id: id(name),
            identities: SplitIdentities {
                nodes: (0..count)
                    .map(|n| id(&format!("{name}-split-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(name).unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn witness(treated: bool, with_original: bool) {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    if with_original {
        let original = audio(0..6407);
        wire["nodes"]["owner"]["kind"]["source"]["audio"] =
            serde_json::to_value(&original).unwrap();
        wire["nodes"]["owner"]["kind"]["source"]["audio_mapping"] = serde_json::to_value(
            SourceAudioMapping::natural_rate(original.span, FrameRate::new(30000, 1001).unwrap())
                .unwrap(),
        )
        .unwrap();
    }
    if treated {
        let eq = EqualizerRecipe::new(
            [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
            None,
            None,
        )
        .unwrap();
        let treatment =
            serde_json::to_value(AudioTreatments::default().with_equalizer(Some(eq)).unwrap())
                .unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = treatment.clone();
        wire["beat_sounds"]["owner"]["effect"]["treatments"] = treatment;
    }
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    // Begin with a retained placement clock, then cut the complete owner twice.
    let before = insert_prefix(&initial);
    let once = pause(&before, "pause-one", 5);
    let twice = pause(&once, "pause-two", 7);
    assert_eq!(once.beat_sounds().len(), 2);
    assert_eq!(twice.beat_sounds().len(), 3);
    let read = |document: &ProjectDocument, sample: i64| {
        let mut reader = StageAudio::new(compile(document, false));
        let mut provider = ClockProvider::new(document);
        bus(&mut reader, &mut provider, sample, 256).samples
    };
    // Interior samples exclude the intentional automatic cuts around silence.
    // Compose integral shifts at the actual insertion boundaries. At NTSC,
    // B(6) + (B(6)-B(5)) + (B(8)-B(7)) is B(8)+1, not B(8).
    let first_shift = frame_sample_boundary(6) - frame_sample_boundary(5);
    let second_shift = frame_sample_boundary(8) - frame_sample_boundary(7);
    for (old, shift) in [(3, 0), (5, first_shift), (6, first_shift + second_shift)] {
        let start = frame_sample_boundary(old) + 512;
        let expected = read(&before, start);
        assert!(expected.iter().flatten().any(|v| v.abs() > 0.001));
        compare_copy(
            &expected,
            &read(&twice, start + shift),
            treated,
            "split owner",
        );
    }
    for (start, end) in [(5, 6), (7, 8)] {
        assert!(
            render(&twice, start, end)
                .iter()
                .flatten()
                .all(|sample| *sample == 0.0)
        );
    }
    // A two-ended deletion keeps both full owners, but only surviving delivery.
    let range = FrameRange::new(ProjectFrame(4), ProjectFrame(6)).unwrap();
    let count = before
        .range_deletion(before.root(), range)
        .unwrap()
        .required_ids;
    let deleted = edit(
        &before,
        "delete-middle",
        Command::DeleteRange {
            parent: before.root().clone(),
            range,
            identities: SplitIdentities {
                nodes: (0..count)
                    .map(|n| id(&format!("delete-split-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("delete-middle").unwrap(),
                ordinal: 0,
            },
        },
    );
    assert_eq!(deleted.beat_sounds().len(), 2);
    let start = frame_sample_boundary(6) + 512;
    let shift = frame_sample_boundary(4) - frame_sample_boundary(6);
    compare_copy(
        &read(&before, start),
        &read(&deleted, start + shift),
        treated,
        "deleted middle",
    );

    let destination = deadpan_core::MoveRangeDestination::Seam {
        parent: before.root().clone(),
        index: 4,
    };
    let count = before
        .range_move(before.root(), range, &destination)
        .unwrap()
        .required_ids;
    let moved = edit(
        &before,
        "move-middle",
        Command::MoveRange {
            source_revision: before.revision_id().clone(),
            source_parent: before.root().clone(),
            range,
            destination,
            identities: SplitIdentities {
                nodes: (0..count).map(|n| id(&format!("move-split-{n}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("move-middle").unwrap(),
                ordinal: 0,
            },
        },
    );
    assert_eq!(moved.beat_sounds().len(), 3);
    compare_copy(
        &read(&before, start),
        &read(&moved, start + shift),
        treated,
        "moved suffix",
    );
    let start = frame_sample_boundary(4) + 512;
    let shift = frame_sample_boundary(7) - frame_sample_boundary(4);
    compare_copy(
        &read(&before, start),
        &read(&moved, start + shift),
        treated,
        "moved selection",
    );
}

#[test]
fn interior_pauses_keep_dry_attached_sound_on_every_surviving_fragment() {
    witness(false, false);
}

#[test]
fn interior_pauses_keep_attached_and_shared_equalizer_history() {
    witness(true, false);
}

#[test]
fn split_edits_keep_original_and_attached_sound_in_the_shared_equalizer() {
    witness(true, true);
}
