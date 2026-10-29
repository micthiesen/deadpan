use super::*;
#[path = "cached_policy_tests.rs"]
mod cached_policy;

#[test]
fn producer_predicates_keep_lazy_mask_checks_and_zero_gain_permission_distinct() {
    let mut probes = 0;
    assert!(!original_allowed(false, AudioOwnerSupport::Active, || {
        probes += 1;
        true
    }));
    assert!(!original_allowed(true, AudioOwnerSupport::Inactive, || {
        probes += 1;
        true
    }));
    assert_eq!(probes, 0);
    assert!(original_allowed(true, AudioOwnerSupport::Active, || {
        probes += 1;
        true
    }));
    assert_eq!(probes, 1);
    assert!(!original_allowed(true, AudioOwnerSupport::Active, || false));
    assert_eq!(
        sound_allowed(Some(7), 0, || panic!(
            "zero-length gate does not inspect raw policy"
        )),
        None
    );
    assert_eq!(
        sound_allowed(None, 1, || {
            probes += 1;
            true
        }),
        None
    );
    assert_eq!(
        probes, 2,
        "the old positive-length producer inspects policy even without an admitted id"
    );
    assert_eq!(sound_allowed(Some(7), u64::MAX, || true), Some(7));
    assert_eq!(sound_allowed(Some(7), 1, || false), None);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn real_wav_prediction_is_independent_of_observed_rows_and_detects_adapter_faults() {
    use crate::stages::controlled_reads::fixture_document_at_rate;
    use deadpan_core::FrameRate;
    for (selected, corrupt) in [(false, false), (true, false), (false, true), (true, true)] {
        let (document, mut provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
        let plan = Arc::new(RenderPlan::compile(&document).unwrap());
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(30),
            work: &work,
        };
        let range = AudioSample(0)..AudioSample(64);
        let request = renderer
            .prepare_original_root(&plan, range.start, 64, control, true)
            .unwrap();
        let owners = plan
            .audio_gain_owners(range.clone(), Default::default())
            .unwrap();
        let mut mix = SharedMix::new(range.start, 64, None, None, true, None, &[]);
        let routes = owners
            .spans()
            .iter()
            .map(|span| {
                assert_eq!(span.support(), AudioOwnerSupport::Active);
                assert!(!span.original_is_silent());
                Some(
                    mix.route(
                        span.owners(),
                        span.samples(),
                        (0, Contribution::Original),
                        control,
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        mix.begin_prediction().unwrap();
        let reserved = mix.resident_frames().unwrap() + 64 * 3;
        renderer.make_room(reserved, false, control).unwrap();
        renderer.active_frames += reserved;
        mix.allocate_policy();
        let before_population = work.borrow().plan_work;
        populate(
            &mut mix,
            Inputs {
                original: selected.then_some(Original {
                    policy: OriginalPolicy::Queries(request.queries()),
                    owners: &owners,
                    routes: &routes,
                    id: 0,
                }),
                roots: &[],
                root_range: range.clone(),
                root_ids: &[],
                root_routes: &[],
                beats: &[],
                beat_routes: &[],
            },
            |_| selected,
            control,
        )
        .unwrap();
        if !selected {
            assert_eq!(work.borrow().plan_work, before_population);
        }
        assert_eq!(
            provider.calls, 0,
            "planning never touches the real WAV provider"
        );
        assert_eq!(work.borrow().source_checks, 0);
        assert_eq!(work.borrow().prepared_frames, 0);
        if corrupt {
            // Falsify only the predicted adapter result. The actual request,
            // real input and producer observations remain untouched.
            mix.predict_range(
                routes[0].as_ref().unwrap(),
                AudioSample(16)..AudioSample(17),
                !selected,
                control,
            )
            .unwrap();
        }
        mix.allocate();
        let raw = renderer
            .read_prepared_original(&mut provider, request, control, 0)
            .unwrap();
        assert!(provider.calls > 0);
        for (span, route) in owners.spans().iter().zip(&routes) {
            for at in span.samples().start.0..span.samples().end.0 {
                // Independent output-policy oracle, not the prediction's
                // updated row and not the shared producer predicate.
                let permitted = selected
                    && !raw
                        .suppressed
                        .iter()
                        .chain(&raw.exhausted)
                        .any(|r| r.contains(&AudioSample(at)));
                mix.add(
                    route.as_ref().unwrap(),
                    at as usize,
                    raw.samples[at as usize],
                    0,
                    permitted.then_some(0),
                    control,
                )
                .unwrap();
            }
        }
        let verified = mix.verify_prediction(control);
        if corrupt {
            assert!(matches!(
                verified,
                Err(StageAudioError::Plan(PlanError::InvalidPlan(
                    "contribution permission prediction differs"
                )))
            ));
            drop(mix);
        } else {
            assert!(verified.is_ok());
            let result = mix
                .finish(control, |_, _, _, _, _| panic!("no temporal stages"))
                .unwrap();
            if selected {
                assert_eq!(
                    result.samples,
                    raw.samples
                        .iter()
                        .map(|v| v.map(f64::from))
                        .collect::<Vec<_>>()
                );
                assert!(result.suppressed.is_empty());
            } else {
                assert_eq!(result.samples, vec![[0.0; 2]; 64]);
                assert_eq!(result.suppressed, vec![range]);
            }
        }
        renderer.active_frames -= reserved;
        assert_eq!(renderer.active_frames, 0);
    }
}
