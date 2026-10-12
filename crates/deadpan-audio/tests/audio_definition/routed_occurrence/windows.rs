use super::*;
use deadpan_plan::AudioOccurrenceWindow;

#[test]
fn cropped_occurrence_transport_selects_complete_nested_preserve_output() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let expected = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    for play in [0, 1] {
        let voice = plan
            .source_voice_occurrence(
                occurrence("owner", play),
                voice_recipe(document.presentation_basis().frame_rate, 3840),
                Default::default(),
            )
            .unwrap();
        let start = voice.samples().start.0;
        let origin = ExactRatio::integer(-start);
        let route = SoundRoute::identity(ExactRatio::integer(1280))
            .unwrap()
            .ripple(keep(1280))
            .unwrap()
            .ripple(keep(1280))
            .unwrap()
            .ripple(keep(1280))
            .unwrap();
        // Crop after processing, move again, then expose a larger allocation.
        // The larger allocation cannot recover samples clipped by the crop.
        let windows = [
            (0, 1280, 0),
            (301, 997, 301),
            (301, 997, 301),
            (101, 1200, 101),
        ]
        .map(|(first, last, anchor)| AudioOccurrenceWindow {
            selection: ExactRatio::integer(first)..ExactRatio::integer(last),
            anchor: ExactRatio::integer(anchor),
        })
        .to_vec();
        let sampled = AudioSoundRoute::<AudioSample>::new(
            route,
            [0, 100, -37, 13]
                .map(|shift| {
                    AudioSampleGrid::new(
                        origin.checked_sub(ExactRatio::integer(shift)).unwrap(),
                        ExactRatio::ONE,
                        AudioBoundaryRule::RoundEven,
                    )
                    .unwrap()
                })
                .to_vec(),
        )
        .unwrap()
        .with_occurrence_windows(windows)
        .unwrap();
        let route = AudioRoutedRoot::occurrence(voice, sampled).unwrap();
        assert_eq!(
            route.samples(),
            AudioSample(start + 114)..AudioSample(start + 1213)
        );
        let mut provider = FixtureProvider::new();
        let mut reader = StageAudio::new(plan.clone());
        for offset in (101..1200).step_by(256) {
            let count = u32::try_from((1200 - offset).min(256)).unwrap();
            let actual = read(
                &mut reader,
                &mut provider,
                &route,
                start + 13 + offset,
                count,
            );
            let reference: Vec<_> = (offset..offset + i64::from(count))
                .map(|sample| {
                    if (301..997).contains(&sample) {
                        expected[sample as usize]
                    } else {
                        [0.0; 2]
                    }
                })
                .collect();
            assert_eq!(actual.samples, reference);
        }
        for (offset, count) in [(951, 127), (291, 53), (620, 89)] {
            let actual = read(
                &mut StageAudio::new(plan.clone()),
                &mut provider,
                &route,
                start + 13 + offset,
                count,
            );
            let reference: Vec<_> = (offset..offset + i64::from(count))
                .map(|sample| {
                    if (301..997).contains(&sample) {
                        expected[sample as usize]
                    } else {
                        [0.0; 2]
                    }
                })
                .collect();
            assert_eq!(actual.samples, reference);
        }
        // Delivery windows never trim the independent processing input.
        for (offset, count) in [(0, 127), (1000, 200), (201, 200)] {
            let retained = reader
                .read_retained_sound_input(
                    &mut provider,
                    &route,
                    AudioSample(start + offset),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                retained.allocation,
                AudioSample(start)..AudioSample(start + 1280)
            );
            assert_eq!(
                retained.samples,
                expected[offset as usize..offset as usize + count as usize]
            );
        }
        provider.unavailable = true;
        // A clipped, silent output still requires the complete admitted source.
        assert!(
            reader
                .read_routed_root(
                    &mut provider,
                    &route,
                    AudioSample(start + 114),
                    32,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .is_err()
        );
    }
}
