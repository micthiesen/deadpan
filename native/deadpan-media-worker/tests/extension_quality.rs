//! Real decoder/converter coverage for extension pixel rejection. These
//! synthetic inputs establish exact inspection behavior, not model quality.

use std::fs;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::ExtensionDirection;
use deadpan_jobs::{HostMessage, MotionAmount};
use deadpan_models::{
    ExtensionEndpointJoin, ExtensionJoinRole, ExtensionPixelReport, MotionObservation,
    QualificationError, inspect_extension_pixels,
};

#[path = "extension_quality/fixture.rs"]
mod fixture;
#[cfg(target_os = "macos")]
#[path = "extension_quality/geometry.rs"]
mod geometry;
use fixture::{
    CONTEXT, Fixture, GENERATED, HEIGHT, WIDTH, convert, plan, plan_with_dimensions,
    retained_inputs,
};

const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn inspect(fixture: &mut Fixture) -> Result<ExtensionPixelReport, QualificationError> {
    inspect_extension_pixels(
        &fixture.media,
        &mut fixture.conditioning,
        &fixture.request,
        Instant::now() + Duration::from_secs(30),
        &AtomicBool::new(false),
    )
}

fn rejection(fixture: &mut Fixture) -> String {
    match inspect(fixture) {
        Err(QualificationError::Quality(reason)) => reason,
        Err(error) => panic!("expected pixel quality rejection, got {error}"),
        Ok(_) => panic!("fixture should have been rejected"),
    }
}

fn assert_join(join: &ExtensionEndpointJoin, role: ExtensionJoinRole, ordinal: u32) {
    let ExtensionEndpointJoin::Measured {
        role: observed_role,
        endpoint,
        quality,
        ..
    } = join
    else {
        panic!("present join must have measurements");
    };
    assert_eq!(*observed_role, role);
    assert_eq!(endpoint.sampled_frame, ordinal);
    // Independent millisecond rounding of exact 24 fps output picture PTS.
    assert_eq!(endpoint.sampled_pts, i64::from((ordinal * 1000 + 12) / 24));
    assert_eq!(endpoint.mean_absolute_rgb_difference, 0.0);
    assert_eq!(endpoint.gross_cell_fraction, 0.0);
    assert_eq!(quality.after_frame, ordinal);
    assert!(matches!(quality.motion, MotionObservation::Unavailable));
}

#[test]
fn extension_pixels_inspect_only_generated_pairs_and_label_actual_joins() {
    for direction in DIRECTIONS {
        for output in [1, 8] {
            for opposite in [false, true] {
                // Native context has repeated flashes and its nearest picture
                // is white. Genuine retained black is the edit's seam input.
                let mut fixture = Fixture::new(
                    direction,
                    output,
                    [0; GENERATED as usize],
                    [255, 0, 255, 0, 255, 0, 255, 0, 255],
                    opposite,
                );
                // Worker-writable files cease to be authority after capture.
                fs::write(
                    fixture.directory.path().join("inputs/black.png"),
                    b"changed by worker",
                )
                .unwrap();
                let report = inspect(&mut fixture).unwrap();
                let motion = report.motion();
                assert_eq!(motion.transitions().len(), (GENERATED - 1) as usize);
                assert_eq!(motion.unavailable_motion_pairs(), (GENERATED - 1) as usize);
                assert_eq!(motion.measured_motion_pairs(), 0);
                assert!((motion.pair_seconds() - f64::from(output) / (24.0 * 8.0)).abs() < 1e-15);
                let generated_start = match direction {
                    ExtensionDirection::FromLeft => CONTEXT,
                    ExtensionDirection::FromRight => 0,
                };
                for (offset, pair) in motion.transitions().iter().enumerate() {
                    assert_eq!(pair.after_frame, generated_start + 1 + offset as u32);
                    assert_eq!(pair.mean_absolute_luma_change, 0.0);
                    assert_eq!(pair.mean_luma_shift, 0.0);
                }
                let endpoints = report.endpoints();
                let (conditioned, unconditioned, conditioned_ordinal, opposite_ordinal) =
                    match direction {
                        ExtensionDirection::FromLeft => {
                            (endpoints.entry(), endpoints.exit(), 0, output - 1)
                        }
                        ExtensionDirection::FromRight => {
                            (endpoints.exit(), endpoints.entry(), output - 1, 0)
                        }
                    };
                assert_join(
                    conditioned,
                    ExtensionJoinRole::Conditioned,
                    conditioned_ordinal,
                );
                if opposite {
                    assert_join(
                        unconditioned,
                        ExtensionJoinRole::Unconditioned,
                        opposite_ordinal,
                    );
                } else {
                    assert!(matches!(unconditioned, ExtensionEndpointJoin::Absent {}));
                }
                let decoded: ExtensionPixelReport =
                    serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
                assert_eq!(decoded, report);
                decoded
                    .validate_for(&fixture.media, &fixture.conditioning, &fixture.request)
                    .unwrap();
            }
        }
    }
}

#[test]
fn extension_conditioned_join_uses_retained_png_not_the_worker_native_context() {
    for direction in DIRECTIONS {
        let mut fixture = Fixture::new(
            direction,
            8,
            [255; GENERATED as usize],
            [255; CONTEXT as usize],
            false,
        );
        let reason = rejection(&mut fixture);
        assert!(reason.contains("deadpan-extension-endpoints-1"), "{reason}");
        assert!(reason.contains("Conditioned"), "{reason}");
        assert!(
            reason.contains(match direction {
                ExtensionDirection::FromLeft => "entry",
                ExtensionDirection::FromRight => "exit",
            }),
            "{reason}"
        );
    }
}

#[test]
fn extension_native_flash_is_rejected_even_when_downsampling_omits_it() {
    for direction in DIRECTIONS {
        // N=1 samples native generated position 3.5. The flash at 1 therefore
        // enters neither the sampled movie nor its matching black joins.
        let mut fixture = Fixture::new(
            direction,
            1,
            [0, 255, 0, 0, 0, 0, 0, 0],
            [0; CONTEXT as usize],
            true,
        );
        let reason = rejection(&mut fixture);
        assert!(
            reason.contains("deadpan-extension-motion-lighting-1"),
            "{reason}"
        );
        assert!(reason.contains("abrupt lighting"), "{reason}");
        let after = match direction {
            ExtensionDirection::FromLeft => CONTEXT + 1,
            ExtensionDirection::FromRight => 1,
        };
        assert!(
            reason.contains(&format!("native frame {after}:")),
            "{reason}"
        );
    }
}

#[test]
fn extension_present_opposite_is_checked_and_stays_unconditioned() {
    for direction in DIRECTIONS {
        let mut ramp = [0, 16, 32, 48, 64, 80, 96, 112];
        if direction == ExtensionDirection::FromRight {
            ramp.reverse();
        }
        // Each change of 16 stays below the abrupt-lighting threshold of 32.
        // Only the far, explicitly unconditioned join has gross discontinuity.
        let mut absent = Fixture::new(direction, 8, ramp, [255; CONTEXT as usize], false);
        inspect(&mut absent).unwrap();
        let mut present = Fixture::new(direction, 8, ramp, [255; CONTEXT as usize], true);
        let reason = rejection(&mut present);
        assert!(reason.contains("deadpan-extension-endpoints-1"), "{reason}");
        assert!(reason.contains("Unconditioned"), "{reason}");
        assert!(
            reason.contains(match direction {
                ExtensionDirection::FromLeft => "exit",
                ExtensionDirection::FromRight => "entry",
            }),
            "{reason}"
        );
    }
}

#[test]
fn extension_one_output_frame_checks_sampled_middle_instead_of_native_endpoints() {
    for direction in DIRECTIONS {
        // Both native generated endpoints match black. Exact N=1 sampling
        // chooses 72 at 3.5, above the gross endpoint threshold of 64, while
        // every internal step is 24, below the abrupt-lighting threshold of 32.
        let mut fixture = Fixture::new(
            direction,
            1,
            [0, 24, 48, 72, 72, 48, 24, 0],
            [0; CONTEXT as usize],
            false,
        );
        let reason = rejection(&mut fixture);
        assert!(reason.contains("deadpan-extension-endpoints-1"), "{reason}");
        assert!(reason.contains("Conditioned"), "{reason}");
        assert!(reason.contains("sampled frame 0"), "{reason}");
    }
}

#[test]
fn extension_inspection_preserves_control_and_rejects_stale_sampling_or_reports() {
    let direction = ExtensionDirection::FromLeft;
    let mut fixture = Fixture::new(
        direction,
        8,
        [0; GENERATED as usize],
        [255; CONTEXT as usize],
        true,
    );
    assert!(matches!(
        inspect_extension_pixels(
            &fixture.media,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now() + Duration::from_secs(30),
            &AtomicBool::new(true),
        ),
        Err(QualificationError::Cancelled)
    ));
    assert!(matches!(
        inspect_extension_pixels(
            &fixture.media,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now(),
            &AtomicBool::new(false),
        ),
        Err(QualificationError::Deadline)
    ));
    let report = inspect(&mut fixture).unwrap();
    let wrong_sampling = convert(&fixture.native, &plan(direction, 1));
    assert!(
        report
            .validate_for(&wrong_sampling, &fixture.conditioning, &fixture.request)
            .is_err()
    );
    assert!(matches!(
        inspect_extension_pixels(
            &wrong_sampling,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now() + Duration::from_secs(30),
            &AtomicBool::new(false),
        ),
        Err(QualificationError::Quality(_))
    ));
    let different_native = Fixture::new(
        direction,
        8,
        [0; GENERATED as usize],
        [254; CONTEXT as usize],
        true,
    );
    assert_eq!(
        fixture.media.sampled().report().output_rgb_sha256,
        different_native.media.sampled().report().output_rgb_sha256
    );
    assert_ne!(
        fixture.media.native().object(),
        different_native.media.native().object()
    );
    assert!(
        report
            .motion()
            .validate(
                &plan(direction, 8),
                different_native.media.native().object(),
                MotionAmount::Still,
            )
            .is_err()
    );
    assert!(
        report
            .validate_for(
                &different_native.media,
                &fixture.conditioning,
                &fixture.request
            )
            .is_err()
    );
    let mut changed_request = fixture.request.clone();
    let HostMessage::GenerateExtension { constraints, .. } = &mut changed_request else {
        unreachable!();
    };
    constraints.motion = MotionAmount::Subtle;
    assert!(
        report
            .validate_for(&fixture.media, &fixture.conditioning, &changed_request)
            .is_err()
    );
    for mutation in ["missing_pair", "changed_role", "weaker_policy"] {
        let mut wire = serde_json::to_value(&report).unwrap();
        match mutation {
            "missing_pair" => {
                wire["motion"]["transitions"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            "changed_role" => wire["endpoints"]["entry"]["role"] = "unconditioned".into(),
            "weaker_policy" => wire["motion"]["thresholds"]["abrupt_luma_shift"] = 255.0.into(),
            _ => unreachable!(),
        }
        let changed: ExtensionPixelReport = serde_json::from_value(wire).unwrap();
        assert!(
            changed
                .validate_for(&fixture.media, &fixture.conditioning, &fixture.request)
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn extension_report_cannot_relabel_private_media_with_another_raster() {
    for direction in DIRECTIONS {
        let mut fixture = Fixture::new(
            direction,
            8,
            [0; GENERATED as usize],
            [0; CONTEXT as usize],
            false,
        );
        let report = inspect(&mut fixture).unwrap();
        let changed_plan = plan_with_dimensions(direction, 8, WIDTH * 2, HEIGHT * 2);
        let directory = tempfile::tempdir().unwrap();
        let (changed_request, changed_conditioning) =
            retained_inputs(directory.path(), &changed_plan, false);

        // These are coherent new inputs with genuine PNGs and receipts. Their
        // sampling is identical, so the prior sampling-only check accepted
        // them against the old, smaller CanonicalExtension.
        changed_request.validate().unwrap();
        changed_conditioning.validate_for(&changed_request).unwrap();
        assert_eq!(fixture.media.sampling(), changed_plan.sampling_map());
        assert_ne!(
            fixture.media.native().report().video.width,
            changed_plan.native_dimensions().width()
        );
        let mut wire = serde_json::to_value(&report).unwrap();
        wire["motion"]["plan"] = serde_json::to_value(&changed_plan).unwrap();
        wire["endpoints"]["plan"] = serde_json::to_value(&changed_plan).unwrap();
        wire["endpoints"]["sampled"]["width"] = (WIDTH * 2).into();
        wire["endpoints"]["sampled"]["height"] = (HEIGHT * 2).into();
        wire["endpoints"]["conditioning"] =
            serde_json::to_value(changed_conditioning.receipt()).unwrap();
        wire["endpoints"]["presentation"] =
            serde_json::to_value(changed_conditioning.context().presentation()).unwrap();
        let (side, anchor_index) = match direction {
            ExtensionDirection::FromLeft => ("entry", CONTEXT as usize - 1),
            ExtensionDirection::FromRight => ("exit", 0),
        };
        wire["endpoints"][side]["object"] =
            serde_json::to_value(changed_conditioning.receipt().context()[anchor_index].object())
                .unwrap();
        let rewritten: ExtensionPixelReport = serde_json::from_value(wire).unwrap();
        // Native object identity and observations still agree. Only comparing
        // the private media's actual video contract detects the new raster.
        rewritten
            .motion()
            .validate(
                &changed_plan,
                fixture.media.native().object(),
                MotionAmount::Still,
            )
            .unwrap();
        let error = rewritten
            .validate_for(&fixture.media, &changed_conditioning, &changed_request)
            .expect_err("same sampling cannot rebind a different private raster");
        assert!(matches!(error, QualificationError::Quality(_)), "{error}");
        assert!(
            error.to_string().contains("deadpan-extension-pixels-1"),
            "{error}"
        );
    }
}
