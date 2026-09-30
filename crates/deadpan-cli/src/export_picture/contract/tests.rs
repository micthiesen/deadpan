use deadpan_media::source_import_timing::MAX_RASTER_AXIS_ERROR_PIXELS;
use deadpan_render::{MAX_DIMENSION, MAX_PIXELS, MAX_WORKING_FRAME_BYTES, RenderError};

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn basis(width: u32, height: u32, rate: (u32, u32)) -> PresentationBasis {
    PresentationBasis {
        width,
        height,
        frame_rate: FrameRate::new(rate.0, rate.1).unwrap(),
        color_policy: ColorPolicy::SdrRec709,
    }
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn contract(
    basis: &PresentationBasis,
    range: FrameRange,
) -> std::result::Result<ExportPictureContract, ExportPictureError> {
    ExportPictureContract::from_captured(
        &ProjectId::new("captured-project").unwrap(),
        &RevisionId::new("captured-revision").unwrap(),
        basis,
        range,
    )
}

#[test]
fn odd_and_tiny_axes_use_the_shared_even_rule_with_signed_aspect_evidence() -> Result {
    for (canvas, raster, error) in [
        ([320, 180], [320, 180], (0, 1)),
        ([319, 181], [318, 180], (23, 9_570)),
        ([319, 180], [318, 180], (-1, 319)),
        ([320, 181], [320, 180], (1, 180)),
        ([1, 1], [2, 2], (0, 1)),
        ([1, 2], [2, 2], (1, 1)),
        ([2, 1], [2, 2], (-1, 2)),
        ([3, 3], [2, 2], (0, 1)),
    ] {
        let basis = basis(canvas[0], canvas[1], (30_000, 1_001));
        let captured = contract(&basis, range(7, 10))?;
        assert_eq!(captured.canvas(), canvas);
        assert_eq!(captured.raster(), raster);
        assert_eq!(captured.frame_rate(), basis.frame_rate);
        assert_eq!(captured.color_policy(), basis.color_policy);
        assert_eq!(
            captured.relative_aspect_error(),
            ExactRatio::new(error.0, error.1)?
        );
        for (canvas_axis, raster_axis) in canvas.into_iter().zip(raster) {
            assert!(raster_axis.is_multiple_of(2));
            assert!(raster_axis >= 2);
            assert!(canvas_axis.abs_diff(raster_axis) <= MAX_RASTER_AXIS_ERROR_PIXELS);
        }
        assert_eq!([basis.width, basis.height], canvas);
    }
    Ok(())
}

#[test]
fn raster_and_committed_canvas_obey_renderer_bounds_before_allocation() -> Result {
    for canvas in [[8_192, 2_048], [4_096, 4_096], [8_190, 2_048], [1, 8_192]] {
        let captured = contract(&basis(canvas[0], canvas[1], (30, 1)), range(0, 1))?;
        let raster = captured.raster();
        assert!(raster[0] <= MAX_DIMENSION && raster[1] <= MAX_DIMENSION);
        assert!(u64::from(raster[0]) * u64::from(raster[1]) <= MAX_PIXELS);
        let row = u64::from(raster[0]) * 8;
        let padded_row = row.div_ceil(256) * 256;
        assert!(padded_row * u64::from(raster[1]) <= MAX_WORKING_FRAME_BYTES);
    }
    for canvas in [
        [0, 2],
        [2, 0],
        [8_193, 2],
        [2, 8_193],
        [8_192, 2_050],
        [4_097, 4_097],
        [u32::MAX, u32::MAX],
    ] {
        assert!(matches!(
            contract(&basis(canvas[0], canvas[1], (30, 1)), range(0, 1)),
            Err(ExportPictureError::Render(RenderError::Dimensions))
        ));
    }
    Ok(())
}

#[test]
fn output_timestamps_are_exact_relative_to_the_selected_project_range() -> Result {
    for rate in [
        (30_000, 1_001),
        (60_000, 1_001),
        (24_000, 1_001),
        (30, 1),
        (120, 1),
    ] {
        let basis = basis(320, 180, rate);
        let captured = contract(&basis, range(17, 10_018))?;
        assert_eq!(captured.range(), range(17, 10_018));
        assert_eq!(captured.frame_count(), 10_001);
        assert_eq!(captured.time_base().numerator(), 1);
        assert_eq!(captured.time_base().denominator(), rate.0);
        assert_eq!(captured.terminal_pts(), 10_001 * i64::from(rate.1));
        for ordinal in [0, 1, 2, 9_999, 10_000] {
            let timing = captured.timing(OutputFrameOrdinal(ordinal))?;
            assert_eq!(timing.output_frame(), OutputFrameOrdinal(ordinal));
            assert_eq!(
                timing.project_frame(),
                ProjectFrame(17 + i64::try_from(ordinal)?)
            );
            assert_eq!(timing.pts(), i64::try_from(ordinal)? * i64::from(rate.1));
            assert_eq!(timing.duration(), i64::from(rate.1));
            let seconds = ExactRatio::new(i128::from(timing.pts()), i128::from(rate.0))?;
            assert_eq!(
                seconds,
                ExactRatio::new(i128::from(ordinal) * i128::from(rate.1), i128::from(rate.0))?
            );
        }
        let last = captured.timing(OutputFrameOrdinal(10_000))?;
        assert_eq!(last.pts() + last.duration(), captured.terminal_pts());
        assert!(matches!(
            captured.timing(OutputFrameOrdinal(10_001)),
            Err(ExportPictureError::Range)
        ));
        assert!(matches!(
            captured.timing(OutputFrameOrdinal(u64::MAX)),
            Err(ExportPictureError::Range)
        ));
    }
    Ok(())
}

#[test]
fn audio_boundaries_round_absolute_project_coordinates_independently() -> Result {
    let captured = contract(&basis(320, 180, (30_000, 1_001)), range(1, 2))?;
    assert_eq!(captured.project_audio_start(), AudioSample(1_602));
    assert_eq!(captured.project_audio_end(), AudioSample(3_203));
    assert_eq!(
        captured.project_audio_end().0 - captured.project_audio_start().0,
        1_601
    );
    assert_eq!(captured.timing(OutputFrameOrdinal(0))?.pts(), 0);
    assert_eq!(
        captured.timing(OutputFrameOrdinal(0))?.project_frame(),
        ProjectFrame(1)
    );
    assert_ne!(
        captured.project_audio_end().0 - captured.project_audio_start().0,
        captured.frame_rate().audio_boundary(ProjectFrame(1))?.0
    );

    let next = contract(&basis(320, 180, (30_000, 1_001)), range(2, 3))?;
    assert_eq!(next.project_audio_start(), captured.project_audio_end());
    assert_eq!(next.project_audio_end(), AudioSample(4_805));
    Ok(())
}

#[test]
fn empty_negative_and_hdr_captures_fail_explicitly() {
    let mut basis = basis(320, 180, (30, 1));
    for range in [range(0, 0), range(19, 19), range(-1, 3), range(-3, -1)] {
        assert!(matches!(
            contract(&basis, range),
            Err(ExportPictureError::Range)
        ));
    }
    for color in [ColorPolicy::HdrRec2020Pq, ColorPolicy::HdrRec2020Hlg] {
        basis.color_policy = color;
        assert!(matches!(
            contract(&basis, range(0, 1)),
            Err(ExportPictureError::InvalidContract(_))
        ));
    }
}

#[test]
fn encoder_rational_bounds_reject_without_retiming_the_project() -> Result {
    for rate in [
        (u32::MAX, 1),
        (1, u32::MAX),
        (2_147_483_648, 1),
        (1, 2_147_483_648),
    ] {
        assert!(matches!(
            contract(&basis(320, 180, rate), range(0, 1)),
            Err(ExportPictureError::InvalidContract(_))
        ));
    }
    for rate in [(2_147_483_647, 1), (1, 2_147_483_647), (2_147_483_647, 2)] {
        let captured = contract(&basis(320, 180, rate), range(0, 1))?;
        assert_eq!(captured.frame_rate(), FrameRate::new(rate.0, rate.1)?);
    }
    // Encoder limits concern the exact normalized rate, not unreduced input.
    let reduced = contract(&basis(320, 180, (u32::MAX, u32::MAX)), range(0, 1))?;
    assert_eq!(reduced.frame_rate(), FrameRate::new(1, 1)?);
    Ok(())
}

#[test]
fn output_terminal_and_audio_boundary_overflow_are_separate_admission_failures() {
    for (rate, range) in [
        ((30_000, 1_001), range(0, i64::MAX)),
        ((1, 1), range(0, i64::MAX)),
        ((1, 1), range(i64::MAX - 1, i64::MAX)),
    ] {
        assert!(matches!(
            contract(&basis(320, 180, rate), range),
            Err(ExportPictureError::Time(TimeError::Overflow))
        ));
    }
}

#[test]
fn largest_representable_boundaries_remain_exact() -> Result {
    let captured = contract(&basis(2, 2, (48_000, 1)), range(0, i64::MAX))?;
    assert_eq!(captured.terminal_pts(), i64::MAX);
    assert_eq!(captured.project_audio_end(), AudioSample(i64::MAX));
    let last = captured.timing(OutputFrameOrdinal(u64::try_from(i64::MAX - 1)?))?;
    assert_eq!(last.project_frame(), ProjectFrame(i64::MAX - 1));
    assert_eq!(last.pts() + last.duration(), i64::MAX);

    let tail = contract(&basis(2, 2, (48_000, 1)), range(i64::MAX - 2, i64::MAX))?;
    assert_eq!(tail.project_audio_start(), AudioSample(i64::MAX - 2));
    let tail_last = tail.timing(OutputFrameOrdinal(1))?;
    assert_eq!(tail_last.project_frame(), ProjectFrame(i64::MAX - 1));
    assert_eq!(tail_last.pts(), 1);
    Ok(())
}

#[test]
fn serialized_evidence_retains_the_captured_identities_and_exact_clocks() -> Result {
    let captured = contract(&basis(319, 181, (30_000, 1_001)), range(1, 3))?;
    assert_eq!(captured.project_id(), &ProjectId::new("captured-project")?);
    assert_eq!(
        captured.revision_id(),
        &RevisionId::new("captured-revision")?
    );
    let serialized = serde_json::to_value(&captured)?;
    assert_eq!(serialized["project_id"], "captured-project");
    assert_eq!(serialized["revision_id"], "captured-revision");
    assert_eq!(serialized["canvas"], serde_json::json!([319, 181]));
    assert_eq!(serialized["raster"], serde_json::json!([318, 180]));
    assert_eq!(
        serialized["frame_rate"],
        serde_json::json!({"numerator": 30_000, "denominator": 1_001})
    );
    assert_eq!(
        serialized["time_base"],
        serde_json::json!({"numerator": 1, "denominator": 30_000})
    );
    assert_eq!(
        serialized["relative_aspect_error"],
        serde_json::json!({"numerator": "23", "denominator": "9570"})
    );
    assert_eq!(serialized["project_audio_start"], 1_602);
    assert_eq!(serialized["project_audio_end"], 4_805);
    assert_eq!(serialized["terminal_pts"], 2_002);
    Ok(())
}
