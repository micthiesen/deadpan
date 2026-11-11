//! A gap isolation must not change the limited (authored gain) bus of a
//! Repeat whose plays contain a nonunity Preserve retime.
use super::explode::{escalated, same_pcm, with_gap};
use super::*;

fn repeated_preserve() -> ProjectDocument {
    document(
        ntsc(),
        &["lead", "repeat", "tail"],
        vec![
            ("lead", silence(1)),
            ("a", source(ntsc(), 2)),
            ("b", source(ntsc(), 5)),
            ("p", preserve("b", 3, 4)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("p")])),
            (
                "repeat",
                escalated(with_gap(repeat("group", 4), room(2, 100..321)), -3000),
            ),
            ("tail", source(ntsc(), 3)),
        ],
    )
}

#[test]
fn isolating_one_gap_keeps_every_ntsc_sample_around_a_repeated_preserve() {
    let original = repeated_preserve();
    let isolated = edit(
        &original,
        "isolate",
        Command::IsolateGap {
            node: id("repeat"),
            iteration: play(0),
            id: id("isolated-gap"),
            timing: AudioTimingId {
                allocation: revision("isolate"),
                ordinal: 0,
            },
        },
    );
    same_pcm(&original, &isolated);
}

fn trimmed_preserve_repeat() -> ProjectDocument {
    let mut p = preserve("b", 3, 4);
    p.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    document(
        ntsc(),
        &["lead", "repeat", "tail"],
        vec![
            ("lead", source(ntsc(), 2)),
            ("b", source(ntsc(), 5)),
            ("p", p),
            ("repeat", repeat("p", 3)),
            ("tail", source(ntsc(), 3)),
        ],
    )
}

/// Ordinary edits to unrelated owners keep the limited bus readable.
#[test]
fn edits_to_unrelated_owners_keep_a_gained_preserve_repeat_playable() {
    let original = trimmed_preserve_repeat();
    let paused = insert_pause(&original, 1, 2, "pause");
    let selection = SliceCaptureSelection::Child { node: id("tail") };
    let plan = original
        .repeat_selection(&id("root"), &selection, 2)
        .unwrap();
    let wrapped = edit(
        &original,
        "wrap",
        Command::RepeatSelection {
            parent: id("root"),
            selection,
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: id("wrapped"),
                group: plan.needs_group.then(|| id("wrapped-group")),
                split: SplitIdentities { nodes: vec![] },
            },
            timing: AudioTimingId {
                allocation: revision("wrap"),
                ordinal: 0,
            },
        },
    );
    let mut provider = Provider::new();
    let before = super::explode::whole(&original, &mut provider);
    for document in [&paused, &wrapped] {
        let after = super::explode::whole(document, &mut provider);
        // The raw prefix before both edits is sample-identical; the limited
        // bus reads through every play without an owner-clock refusal.
        assert_eq!(before[0][..1500], after[0][..1500]);
        assert!(after[2].iter().flatten().all(|sample| sample.is_finite()));
    }
}
