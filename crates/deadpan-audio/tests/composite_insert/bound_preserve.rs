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

/// Bind every implicit clock without changing structure: equal PCM.
fn captured(original: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(original).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            original,
            AudioTimingId {
                allocation: revision("captured"),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    wire["revision_id"] = serde_json::json!("captured");
    ProjectDocument::from_json(&wire.to_string()).unwrap()
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

/// The authored-gain owner walk enters a bound Preserve's input clock. At
/// 30000/1001 the third play's first output sample starts 0.2 sample before
/// the play's exact origin, which maps to a fraction before input point 0.
/// It must still be attributed to that input domain, as unbound playback is.
#[test]
fn bound_repeated_preserve_keeps_its_gain_owners_at_a_rounded_down_start() {
    for original in [trimmed_preserve_repeat(), repeated_preserve()] {
        same_pcm(&original, &captured(&original));
    }
}

/// Ordinary edits that bind unrelated owners keep the limited bus readable.
#[test]
fn edits_binding_unrelated_owners_keep_a_gained_preserve_repeat_playable() {
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
