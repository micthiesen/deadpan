use deadpan_encode::{
    AUDIO_FRAME_SAMPLES, BFramePolicy, EncodeFailureKind, EncoderMode,
    probe::{EncoderProbe, MAX_PROBE_FRAMES, MAX_PROBE_SECONDS, PROBE_VERSION},
};

#[test]
fn clocks_preserve_fractional_rates_and_round_from_the_origin() {
    for (rate, gop, frames, samples) in [
        ([60, 1], 30, 91, 72_800),
        ([30_000, 1_001], 15, 46, 73_674),
        ([24, 1], 12, 37, 74_000),
        ([32_000, 1_001], 16, 49, 73_574),
        ([32_000, 1_003], 16, 49, 73_720),
        ([1, 2], 1, 4, 384_000),
    ] {
        let probe = EncoderProbe::new([322, 182], rate).unwrap();
        let config = probe.config();
        assert_eq!(config.version, PROBE_VERSION);
        assert_eq!(config.raster, [322, 182]);
        assert_eq!(config.frame_rate, rate);
        assert_eq!(config.gop_frames, gop);
        assert_eq!(config.video_frames, frames);
        assert_eq!(config.audio_samples, samples);
        assert!(frames >= 2 * u64::from(gop));
        assert!(frames <= MAX_PROBE_FRAMES);
        assert!(frames * u64::from(rate[1]) <= MAX_PROBE_SECONDS * u64::from(rate[0]));
        assert_eq!(config.picture_bytes, 322 * 182 * 3 / 2);
    }
}

#[test]
fn invalid_geometry_rates_and_long_probes_fail_before_allocation() {
    for (raster, rate) in [
        ([1, 180], [30, 1]),
        ([321, 180], [30, 1]),
        ([320, 181], [30, 1]),
        ([8_192, 8_192], [30, 1]),
        ([8_194, 2], [30, 1]),
        ([320, 180], [0, 1]),
        ([320, 180], [30, 0]),
        ([320, 180], [60, 2]),
        ([320, 180], [61, 1]),
        ([320, 180], [1, 3]),
    ] {
        assert_eq!(
            EncoderProbe::new(raster, rate).unwrap_err().kind(),
            EncodeFailureKind::Configuration
        );
    }
    let error = EncoderProbe::new([320, 180], [1, 3]).unwrap_err();
    assert!(error.to_string().contains("eight seconds"));
}

#[test]
fn encoder_choices_do_not_change_the_fixture_or_its_clocks() {
    let probe = EncoderProbe::new([1_920, 1_080], [30_000, 1_001]).unwrap();
    for mode in [EncoderMode::Hardware, EncoderMode::Software] {
        for b_frames in [BFramePolicy::TargetTwo, BFramePolicy::None] {
            let contract = probe.contract(mode, b_frames).unwrap();
            assert_eq!(contract.raster(), probe.config().raster);
            assert_eq!(contract.frame_rate(), probe.config().frame_rate);
            assert_eq!(contract.video_frames(), probe.config().video_frames);
            assert_eq!(contract.audio_samples(), probe.config().audio_samples);
            assert_eq!(contract.picture_bytes(), probe.config().picture_bytes);
            assert_eq!(contract.policy().gop_frames, probe.config().gop_frames);
            assert_eq!(contract.mode(), mode);
            assert_eq!(
                contract.policy().b_frames,
                if b_frames == BFramePolicy::None { 0 } else { 2 }
            );
        }
    }
}

#[test]
fn version_one_has_fixed_ordinal_colors_and_visible_motion() {
    let probe = EncoderProbe::new([320, 180], [60, 1]).unwrap();
    assert_eq!(probe.pixel(0, [0, 0]).unwrap(), [32, 128, 128]);
    assert_eq!(probe.pixel(1, [0, 0]).unwrap(), [224, 128, 128]);
    assert_eq!(probe.pixel(1, [46, 0]).unwrap(), [32, 128, 128]);
    assert_eq!(probe.pixel(2, [46, 0]).unwrap(), [224, 128, 128]);
    assert_eq!(probe.pixel(0, [0, 60]).unwrap(), [210, 70, 200]);
    assert_eq!(probe.pixel(0, [280, 60]).unwrap(), [52, 128, 128]);
    assert_eq!(probe.pixel(90, [0, 60]).unwrap(), [40, 128, 128]);
    assert_eq!(probe.pixel(90, [280, 60]).unwrap(), [210, 70, 200]);
    for (x, color) in [
        (0, [16, 128, 128]),
        (64, [235, 128, 128]),
        (128, [63, 102, 240]),
        (192, [173, 42, 26]),
        (256, [32, 240, 118]),
    ] {
        assert_eq!(probe.pixel(0, [x, 150]).unwrap(), color);
    }
}

#[test]
fn tight_planes_cover_odd_chroma_dimensions_and_minimal_rasters() {
    for raster in [[2, 2], [6, 10], [34, 26], [322, 182]] {
        let probe = EncoderProbe::new(raster, [60, 1]).unwrap();
        let [width, height] = raster.map(|value| usize::try_from(value).unwrap());
        let pixels = width * height;
        let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
        for ordinal in [0, 1, probe.config().video_frames - 1] {
            probe.fill_picture(ordinal, &mut picture).unwrap();
            assert!(
                picture[..pixels]
                    .iter()
                    .all(|code| (16..=235).contains(code))
            );
            assert!(
                picture[pixels..]
                    .iter()
                    .all(|code| (16..=240).contains(code))
            );
            for row in 0..height {
                for column in 0..width {
                    let expected = probe
                        .pixel(
                            ordinal,
                            [u32::try_from(column).unwrap(), u32::try_from(row).unwrap()],
                        )
                        .unwrap();
                    let chroma = (row / 2) * (width / 2) + column / 2;
                    assert_eq!(picture[row * width + column], expected[0]);
                    assert_eq!(picture[pixels + chroma], expected[1]);
                    assert_eq!(picture[pixels + pixels / 4 + chroma], expected[2]);
                }
            }
        }
    }
}

#[test]
fn every_ordinal_is_distinct_and_reproducible_in_one_reused_frame() {
    let probe = EncoderProbe::new([320, 180], [60, 1]).unwrap();
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
    let mut strips = std::collections::BTreeSet::new();
    for ordinal in 0..probe.config().video_frames {
        probe.fill_picture(ordinal, &mut picture).unwrap();
        assert!(strips.insert(picture[..320].to_vec()));
    }
    probe.fill_picture(0, &mut picture).unwrap();
    let expected = picture.clone();
    probe.fill_picture(40, &mut picture).unwrap();
    assert_ne!(picture, expected);
    probe.fill_picture(0, &mut picture).unwrap();
    assert_eq!(picture, expected);
}

#[test]
fn stereo_events_have_fixed_independent_coordinates_and_amplitudes() {
    let probe = EncoderProbe::new([320, 180], [60, 1]).unwrap();
    assert_eq!(
        probe.markers().map(|marker| marker.samples),
        [[100, 137], [36_400, 36_437], [72_600, 72_637]]
    );
    assert_eq!(
        probe.markers().map(|marker| marker.amplitudes),
        [[0.75, -0.625], [0.625, -0.75], [0.6875, -0.5625]]
    );
    let mut left = [0.0; 1024];
    let mut right = [0.0; 1024];
    probe.fill_audio(0, &mut left, &mut right).unwrap();
    assert_eq!(left[100], 0.75);
    assert_eq!(right[137], -0.625);
    assert_eq!(left[137], 0.0);
    assert_eq!(right[100], 0.0);
    assert_eq!(left.iter().filter(|sample| **sample != 0.0).count(), 1);
    assert_eq!(right.iter().filter(|sample| **sample != 0.0).count(), 1);
}

#[test]
fn audio_block_boundaries_preserve_each_marker_once_and_no_other_samples() {
    let probe = EncoderProbe::new([320, 180], [30_000, 1_001]).unwrap();
    for block_size in [1, 137, u64::from(AUDIO_FRAME_SAMPLES)] {
        let mut observed = [Vec::new(), Vec::new()];
        let mut first = 0;
        let mut left = [0.0; 1024];
        let mut right = [0.0; 1024];
        while first < probe.config().audio_samples {
            let count =
                usize::try_from(block_size.min(probe.config().audio_samples - first)).unwrap();
            probe
                .fill_audio(first, &mut left[..count], &mut right[..count])
                .unwrap();
            for (channel, values) in [&left, &right].into_iter().enumerate() {
                for (offset, sample) in values[..count].iter().copied().enumerate() {
                    assert!(sample.is_finite());
                    if sample != 0.0 {
                        observed[channel].push((first + u64::try_from(offset).unwrap(), sample));
                    }
                }
            }
            first += u64::try_from(count).unwrap();
        }
        for (channel, values) in observed.iter().enumerate() {
            let expected: Vec<_> = probe
                .markers()
                .iter()
                .map(|marker| (marker.samples[channel], marker.amplitudes[channel]))
                .collect();
            assert_eq!(values, &expected);
        }
    }
}

#[test]
fn invalid_requests_leave_caller_buffers_unchanged() {
    let probe = EncoderProbe::new([32, 24], [60, 1]).unwrap();
    let mut picture = vec![7; usize::try_from(probe.config().picture_bytes).unwrap()];
    for ordinal in [probe.config().video_frames, u64::MAX] {
        assert!(probe.fill_picture(ordinal, &mut picture).is_err());
        assert!(picture.iter().all(|code| *code == 7));
    }
    assert!(probe.fill_picture(0, &mut picture[..5]).is_err());
    assert!(picture.iter().all(|code| *code == 7));
    assert!(probe.pixel(0, [32, 0]).is_err());
    assert!(probe.pixel(0, [0, 24]).is_err());
    let mut left = [0.25; 1025];
    let mut right = [-0.25; 1025];
    for (start, left_count, right_count) in [
        (0, 0, 0),
        (0, 1, 2),
        (0, 1025, 1025),
        (probe.config().audio_samples, 1, 1),
        (u64::MAX, 1, 1),
    ] {
        assert!(
            probe
                .fill_audio(start, &mut left[..left_count], &mut right[..right_count])
                .is_err()
        );
        assert!(left.iter().all(|sample| *sample == 0.25));
        assert!(right.iter().all(|sample| *sample == -0.25));
    }
}
