use deadpan_encode::{
    BFramePolicy, EncodeContract, EncodeLimits, EncoderMode, MAX_AUDIO_SAMPLES, MAX_OUTPUT_BYTES,
    MAX_PACKET_BYTES, MAX_PACKETS, MAX_VIDEO_FRAMES,
};

fn contract(raster: [u32; 2], rate: [u32; 2], frames: u64, samples: u64) -> EncodeContract {
    EncodeContract::new(
        raster,
        rate,
        frames,
        samples,
        EncoderMode::Hardware,
        BFramePolicy::TargetTwo,
    )
    .unwrap()
}

#[test]
fn specification_classes_and_rate_split_are_exact() {
    for (raster, low, high) in [
        ([640, 360], 1_500_000, 2_000_000),
        ([1280, 720], 5_000_000, 7_500_000),
        ([1920, 1080], 8_000_000, 12_000_000),
        ([2560, 1440], 16_000_000, 24_000_000),
        ([3840, 2160], 45_000_000, 68_000_000),
        ([7680, 4320], 160_000_000, 240_000_000),
    ] {
        assert_eq!(
            contract(raster, [30, 1], 1, 1600).policy().video_bitrate,
            low
        );
        assert_eq!(
            contract(raster, [60, 1], 1, 800).policy().video_bitrate,
            high
        );
        assert_eq!(
            contract(raster, [30000, 1001], 1, 1602)
                .policy()
                .video_bitrate,
            low
        );
        assert_eq!(
            contract(raster, [60000, 1001], 1, 801)
                .policy()
                .video_bitrate,
            high
        );
    }
    assert_eq!(
        contract([2, 2], [30, 1], 1, 1600).policy().video_bitrate,
        1_500_000
    );
    assert_eq!(
        contract([8192, 4096], [60, 1], 1, 800)
            .policy()
            .video_bitrate,
        240_000_000
    );
}

#[test]
fn non_widescreen_rasters_interpolate_by_pixel_area() {
    // 640x640 has exactly the nominal 16:9 480-line class area, without
    // inventing a rounded nominal width. Rotation preserves that policy.
    assert_eq!(
        contract([640, 640], [30, 1], 1, 1600)
            .policy()
            .video_bitrate,
        3_000_000
    );
    assert_eq!(
        contract([640, 640], [60, 1], 1, 800).policy().video_bitrate,
        4_000_000
    );
    assert_eq!(
        contract([1440, 640], [30, 1], 1, 1600)
            .policy()
            .video_bitrate,
        5_000_000
    );
    assert_eq!(
        contract([640, 1440], [30, 1], 1, 1600)
            .policy()
            .video_bitrate,
        5_000_000
    );
    // 1000x1440 lies 45% of the pixel distance from 720p to 1080p.
    assert_eq!(
        contract([1000, 1440], [30, 1], 1, 1600)
            .policy()
            .video_bitrate,
        6_350_000
    );
}

#[test]
fn exact_rate_gop_and_movie_clock_are_preserved() {
    for (rate, samples, gop, timescale) in [
        ([24000, 1001], 2002, 12, 48000),
        ([30000, 1001], 1602, 15, 240000),
        ([60000, 1001], 801, 30, 240000),
        ([25, 1], 1920, 13, 48000),
        ([1, 1], 48000, 1, 48000),
    ] {
        let contract = contract([320, 180], rate, 1, samples);
        assert_eq!(contract.frame_rate(), rate);
        assert_eq!(contract.picture_timing(0).unwrap(), (0, i64::from(rate[1])));
        assert!(contract.picture_timing(1).is_err());
        assert_eq!(contract.policy().gop_frames, gop);
        assert_eq!(contract.policy().movie_timescale, timescale);
        assert_eq!(contract.policy().audio_bitrate, 384_000);
        assert_eq!(contract.policy().b_frames, 2);
    }
    let software = EncodeContract::new(
        [320, 180],
        [30, 1],
        1,
        1600,
        EncoderMode::Software,
        BFramePolicy::None,
    )
    .unwrap();
    assert_eq!(software.mode(), EncoderMode::Software);
    assert_eq!(software.policy().b_frames, 0);
    assert!(
        serde_json::to_string(software.policy())
            .unwrap()
            .contains("384000")
    );
}

#[test]
fn caller_supplied_nonzero_origin_audio_count_is_not_replaced() {
    let selected = contract([320, 180], [30000, 1001], 1, 1601);
    assert_eq!(selected.audio_samples(), 1601);
    let origin = contract([320, 180], [30000, 1001], 1, 1602);
    assert_eq!(origin.audio_samples(), 1602);
    let plus_phase = contract([320, 180], [96_000, 1601], 2, 1602);
    assert_eq!(plus_phase.audio_samples(), 1602); // B(3)-B(1)=2402-800.
    assert!(
        EncodeContract::new(
            [320, 180],
            [30000, 1001],
            1,
            1600,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo
        )
        .is_err()
    );
}

#[test]
fn hard_geometry_clock_duration_and_allocation_bounds_fail_before_native_work() {
    for raster in [
        [0, 2],
        [2, 1],
        [319, 180],
        [320, 179],
        [8194, 2],
        [8192, 8192],
    ] {
        assert!(
            EncodeContract::new(
                raster,
                [30, 1],
                1,
                1600,
                EncoderMode::Hardware,
                BFramePolicy::TargetTwo
            )
            .is_err()
        );
    }
    for rate in [
        [0, 1],
        [30, 0],
        [60000, 2002],
        [61, 1],
        [u32::MAX, 100_000_000],
        [2_147_483_647, 40_000_000],
    ] {
        assert!(
            EncodeContract::new(
                [320, 180],
                rate,
                1,
                1600,
                EncoderMode::Hardware,
                BFramePolicy::TargetTwo
            )
            .is_err()
        );
    }
    for (frames, samples) in [
        (0, 0),
        (MAX_VIDEO_FRAMES + 1, 1600),
        (1, 0),
        (1, MAX_AUDIO_SAMPLES + 1),
    ] {
        assert!(
            EncodeContract::new(
                [320, 180],
                [30, 1],
                frames,
                samples,
                EncoderMode::Hardware,
                BFramePolicy::TargetTwo
            )
            .is_err()
        );
    }
    assert!(
        EncodeContract::new(
            [320, 180],
            [1, 1],
            86401,
            MAX_AUDIO_SAMPLES,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo
        )
        .is_err()
    );
    let default = EncodeLimits::default();
    default.validate().unwrap();
    for limits in [
        EncodeLimits {
            maximum_output_bytes: 0,
            ..default
        },
        EncodeLimits {
            maximum_output_bytes: MAX_OUTPUT_BYTES + 1,
            ..default
        },
        EncodeLimits {
            maximum_packets: 0,
            ..default
        },
        EncodeLimits {
            maximum_packets: MAX_PACKETS + 1,
            ..default
        },
        EncodeLimits {
            maximum_packet_bytes: 0,
            ..default
        },
        EncodeLimits {
            maximum_packet_bytes: MAX_PACKET_BYTES + 1,
            ..default
        },
    ] {
        assert!(limits.validate().is_err());
    }
    let short = contract([2, 2], [60, 1], 1, 800);
    assert!(
        EncodeLimits {
            maximum_packets: 3,
            ..default
        }
        .validate_for(&short)
        .is_err()
    );
    EncodeLimits {
        maximum_packets: 4,
        ..default
    }
    .validate_for(&short)
    .unwrap();
}
