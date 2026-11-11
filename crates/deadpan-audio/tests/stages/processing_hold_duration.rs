//! Canonical PCM regressions inspect the actual processor input tape.
use super::*;

#[path = "processing_hold_duration_output_sound.rs"]
mod output_sound;

fn voice() -> BeatNode {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut node = source(rate, 4, 0..1);
    let NodeKind::Source { source } = &mut node.kind else {
        panic!()
    };
    let audio = audio_at_rate(3100, 23_100, 44_100);
    source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
    source.audio = Some(audio);
    node
}

fn resize(document: &ProjectDocument, frames: i64, revision: &str) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command: Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(frames),
            },
        },
    )
    .unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    assert_eq!(
        after.nodes()[&id("processor")],
        document.nodes()[&id("processor")]
    );
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn same_recorded(actual: &Recorded, expected: &Recorded, start: i64, label: &str) {
    same_pcm(&actual.samples, &expected.samples, start, label);
    assert_eq!(
        actual.suppressed.len(),
        expected.suppressed.len(),
        "{label} mask length"
    );
    for (offset, (actual, expected)) in actual
        .suppressed
        .iter()
        .zip(&expected.suppressed)
        .enumerate()
    {
        assert_eq!(
            actual,
            expected,
            "{label} suppression at {}",
            start + offset as i64
        );
    }
}

fn same_pcm(actual: &[[f32; 2]], expected: &[[f32; 2]], start: i64, label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label} length");
    for (offset, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert_eq!(
                actual[channel],
                expected[channel],
                "{label} input sample {} channel {channel}",
                start + offset as i64
            );
        }
    }
}

// Whole CURRENT selected operand, with literal old PointCeil phase and the
// current native ceil source crop. Historical allocations are also PointCeil.
