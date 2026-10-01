use super::*;

fn declared(document: &ProjectDocument, start: ExactRatio, end: ExactRatio) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["source"]["kind"]["source"]["edit_window"] =
        serde_json::to_value(SourceEditWindow::new(start, end).unwrap()).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn editorial_window_does_not_activate_dormant_audio_or_change_selected_pcm() {
    let dormant = document(
        FrameRate::new(30_000, 1001).unwrap(),
        3,
        ratio(-1, 7),
        ExactFrameRange {
            start: ratio(1, 3),
            end: ratio(1, 3),
        },
        3,
    );
    for original in [dormant.clone(), bound(&dormant)] {
        let with_window = declared(&original, ExactRatio::ZERO, ratio(1, 4));
        assert_eq!(with_window.audio_bindings(), original.audio_bindings());
        assert_silence(&original);
        assert_silence(&with_window);
    }

    let audible = document(
        FrameRate::new(30_000, 1001).unwrap(),
        3,
        ratio(-1, 7),
        ExactFrameRange::new(ratio(1, 5), ratio(3, 5)).unwrap(),
        3,
    );
    // The editorial interval is after the independent three-sample offset.
    let offset = ratio(15, 8008);
    let start = ratio(1, 5).checked_add(offset).unwrap();
    let end = ratio(3, 5).checked_add(offset).unwrap();
    let expected = expected_fractional_pcm(4805);
    assert!(
        expected[..323]
            .iter()
            .chain(&expected[964..])
            .flatten()
            .all(|value| value.to_bits() == 0)
    );
    assert!(expected[323..964].iter().any(|value| *value != [0.; 2]));
    for original in [audible.clone(), bound(&audible)] {
        let with_window = declared(&original, start, end);
        assert_eq!(with_window.audio_bindings(), original.audio_bindings());
        for candidate in [&original, &with_window] {
            let plan = Arc::new(RenderPlan::compile(candidate).unwrap());
            assert_eq!(plan.audio_duration().unwrap(), AudioSample(4805));
            let mut provider =
                FixtureProvider::new(BTreeSet::from([candidate.revision_id().clone()]));
            for chunks in [[199, 1, 127], [7, 251, 3]] {
                let mut renderer = StageAudio::new(Arc::clone(&plan));
                let mut actual = Vec::new();
                for requested in chunks.into_iter().cycle() {
                    if actual.len() == expected.len() {
                        break;
                    }
                    let count =
                        requested.min(u32::try_from(expected.len() - actual.len()).unwrap());
                    let block = renderer
                        .read(
                            &mut provider,
                            AudioSample(i64::try_from(actual.len()).unwrap()),
                            count,
                            TIMEOUT,
                            &AtomicBool::new(false),
                        )
                        .unwrap();
                    actual.extend(block.samples);
                }
                assert_eq!(actual, expected);
            }
            assert!(provider.calls > 0);
        }
    }
}
