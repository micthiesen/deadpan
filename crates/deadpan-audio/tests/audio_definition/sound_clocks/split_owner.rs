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

#[test]
fn a_plain_split_keeps_attached_sound_on_both_fragments_with_identical_pcm() {
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
        let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
        // Begin from a retained placement clock, as a real project would.
        let before = insert_prefix(&initial);
        let split = edit(
            &before,
            "plain-split",
            Command::Split {
                node: id("owner"),
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: (0..4).map(|n| id(&format!("plain-split-{n}"))).collect(),
                },
            },
        );
        assert_eq!(split.beat_sounds().len(), 2);
        assert_eq!(split.duration().unwrap(), before.duration().unwrap());
        // A pure Split changes no time and no audible sample.
        let total = frame_sample_boundary(before.duration().unwrap().frames());
        let read = |document: &ProjectDocument, sample: i64, count: u32| {
            let mut reader = StageAudio::new(compile(document, false));
            let mut provider = ClockProvider::new(document);
            bus(&mut reader, &mut provider, sample, count).samples
        };
        let mut start = 0;
        while start < total {
            let count = u32::try_from((total - start).min(1021)).unwrap();
            assert_eq!(
                read(&split, start, count),
                read(&before, start, count),
                "samples from {start}, treated {treated}"
            );
            start += i64::from(count);
        }
        assert!(
            read(&before, frame_sample_boundary(3), 512)
                .iter()
                .flatten()
                .any(|value| value.abs() > 0.001)
        );
    }
}

#[test]
fn a_longer_pause_before_an_attached_sound_owner_moves_its_sound_on_its_own_clock() {
    let before = insert_prefix(&ntsc(AudioEdgePolicy::Hard));
    let longer = edit(
        &before,
        "longer-lead",
        Command::SetHoldDuration {
            node: id("lead"),
            duration: FrameDuration::new(3).unwrap(),
        },
    );
    assert_eq!(longer.beat_sounds().len(), 1);
    let read = |document: &ProjectDocument, sample: i64| {
        let mut reader = StageAudio::new(compile(document, false));
        let mut provider = ClockProvider::new(document);
        bus(&mut reader, &mut provider, sample, 256).samples
    };
    // The owner moves from frame 3 to 4 and resumes on its own boundary.
    let shift = frame_sample_boundary(4) - frame_sample_boundary(3);
    for offset in [256, 512, 2048] {
        let start = frame_sample_boundary(3) + offset;
        let expected = read(&before, start);
        assert!(expected.iter().flatten().any(|v| v.abs() > 0.001));
        compare_copy(
            &expected,
            &read(&longer, start + shift),
            false,
            "longer lead",
        );
    }
    // The longer pause itself stays silent.
    assert!(
        render(&longer, 1, 4)
            .iter()
            .flatten()
            .all(|sample| *sample == 0.0)
    );
}

/// Slowing a beat before the owner moves the attached sound by the exact
/// sample shift of the new boundary, with its PCM unchanged; changing that
/// speed again appends a second clock.
#[test]
fn speed_changes_before_an_owner_move_its_attached_sound_exactly() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let wrapped = edit(
        &before,
        "slowed",
        Command::WrapRetime {
            node: id("lead"),
            id: id("speed"),
            duration: FrameDuration::new(3).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let changed = edit(
        &wrapped,
        "slower",
        Command::SetRetime {
            node: id("speed"),
            duration: FrameDuration::new(5).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let read = |document: &ProjectDocument, sample: i64| {
        let mut reader = StageAudio::new(compile(document, false));
        let mut provider = ClockProvider::new(document);
        bus(&mut reader, &mut provider, sample, 256).samples
    };
    for (document, lead) in [(&wrapped, 3), (&changed, 5)] {
        let start = frame_sample_boundary(2) + 512;
        let expected = read(&before, start);
        assert!(expected.iter().flatten().any(|v| v.abs() > 0.001));
        let shift = frame_sample_boundary(lead) - frame_sample_boundary(2);
        compare_copy(
            &expected,
            &read(document, start + shift),
            false,
            "slowed lead",
        );
    }
}
