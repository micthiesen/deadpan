use deadpan_encode::probe::{
    EncoderProbe, HDR_PROBE_CONTENT_LIGHT, HDR_PROBE_MASTERING, HDR_PROBE_VERSION, HdrEncoderProbe,
};
use deadpan_encode::{BFramePolicy, EncodeFailureKind, EncoderMode, HdrTransfer, VideoFormat};

const TRANSFERS: [HdrTransfer; 2] = [HdrTransfer::Pq, HdrTransfer::Hlg];

fn sample(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]])
}

#[test]
fn clocks_and_markers_equal_the_sdr_probe_with_doubled_picture_bytes() {
    for rate in [[60, 1], [30_000, 1_001], [24, 1], [1, 2]] {
        let sdr = EncoderProbe::new([322, 182], rate).unwrap();
        for transfer in TRANSFERS {
            let probe = HdrEncoderProbe::new([322, 182], rate, transfer).unwrap();
            let config = probe.config();
            assert_eq!(config.version, HDR_PROBE_VERSION);
            assert_eq!(config.video_frames, sdr.config().video_frames);
            assert_eq!(config.audio_samples, sdr.config().audio_samples);
            assert_eq!(config.gop_frames, sdr.config().gop_frames);
            assert_eq!(config.picture_bytes, 322 * 182 * 3);
            assert_eq!(probe.markers(), sdr.markers());
            assert_eq!(probe.transfer(), transfer);
        }
    }
    for (raster, rate) in [
        ([321, 180], [30, 1]),
        ([320, 180], [61, 1]),
        ([320, 180], [1, 3]),
    ] {
        assert_eq!(
            HdrEncoderProbe::new(raster, rate, HdrTransfer::Pq)
                .unwrap_err()
                .kind(),
            EncodeFailureKind::Configuration
        );
    }
}

#[test]
fn contracts_carry_the_transfer_and_pq_only_metadata() {
    for transfer in TRANSFERS {
        let probe = HdrEncoderProbe::new([1920, 1080], [30_000, 1_001], transfer).unwrap();
        for mode in [EncoderMode::Hardware, EncoderMode::Software] {
            for b_frames in [BFramePolicy::TargetTwo, BFramePolicy::None] {
                let contract = probe.contract(mode, b_frames).unwrap();
                assert_eq!(contract.picture_bytes(), probe.config().picture_bytes);
                assert_eq!(contract.video_frames(), probe.config().video_frames);
                assert_eq!(contract.mode(), mode);
                let hdr = contract.hdr().unwrap();
                assert_eq!(hdr.signal.transfer, transfer);
                match transfer {
                    HdrTransfer::Pq => {
                        assert_eq!(contract.video_format(), VideoFormat::HevcMain10Rec2100Pq);
                        assert_eq!(hdr.signal.mastering, Some(HDR_PROBE_MASTERING));
                        assert_eq!(probe.content_light(), Some(HDR_PROBE_CONTENT_LIGHT));
                    }
                    HdrTransfer::Hlg => {
                        assert_eq!(contract.video_format(), VideoFormat::HevcMain10Rec2100Hlg);
                        assert_eq!(hdr.signal.mastering, None);
                        assert_eq!(probe.content_light(), None);
                    }
                }
            }
        }
    }
    HDR_PROBE_MASTERING.validate().unwrap();
    HDR_PROBE_CONTENT_LIGHT.validate().unwrap();
}

#[test]
fn version_one_has_known_reference_codes_and_motion() {
    let pq = HdrEncoderProbe::new([320, 180], [60, 1], HdrTransfer::Pq).unwrap();
    let hlg = HdrEncoderProbe::new([320, 180], [60, 1], HdrTransfer::Hlg).unwrap();
    assert_eq!(pq.pixel(0, [0, 0]).unwrap(), [64, 512, 512]);
    assert_eq!(pq.pixel(1, [0, 0]).unwrap(), [573, 512, 512]);
    assert_eq!(hlg.pixel(1, [0, 0]).unwrap(), [721, 512, 512]);
    assert_eq!(pq.pixel(2, [46, 0]).unwrap(), [573, 512, 512]);
    assert_eq!(pq.pixel(0, [0, 60]).unwrap(), [543, 252, 533]);
    assert_eq!(pq.pixel(90, [280, 60]).unwrap(), [543, 252, 533]);
    assert_eq!(hlg.pixel(0, [0, 60]).unwrap(), [682, 176, 539]);
    // Ramp: black at the left, 1000 cd/m² (PQ) or peak (HLG) at the right.
    assert_eq!(pq.pixel(90, [0, 60]).unwrap(), [64, 512, 512]);
    assert_eq!(pq.pixel(0, [319, 100]).unwrap(), [723, 512, 512]);
    assert_eq!(hlg.pixel(0, [319, 100]).unwrap(), [940, 512, 512]);
    for (x, pq_code, hlg_code) in [
        (0, [64, 512, 512], [64, 512, 512]),
        (40, [119, 512, 512], [97, 512, 512]),
        (80, [327, 512, 512], [287, 512, 512]),
        (120, [573, 512, 512], [721, 512, 512]),
        (160, [723, 512, 512], [940, 512, 512]),
        (200, [198, 439, 772], [237, 418, 848]),
        (240, [409, 325, 273], [509, 270, 203]),
        (280, [94, 772, 491], [103, 848, 485]),
    ] {
        assert_eq!(pq.pixel(0, [x, 150]).unwrap(), pq_code, "{x}");
        assert_eq!(hlg.pixel(0, [x, 150]).unwrap(), hlg_code, "{x}");
    }
}

#[test]
fn planar_little_endian_planes_match_pixels_and_limited_range() {
    for transfer in TRANSFERS {
        for raster in [[2, 2], [6, 10], [34, 26], [322, 182]] {
            let probe = HdrEncoderProbe::new(raster, [60, 1], transfer).unwrap();
            let [width, height] = raster.map(|value| usize::try_from(value).unwrap());
            let pixels = width * height;
            let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
            for ordinal in [0, 1, probe.config().video_frames - 1] {
                probe.fill_picture(ordinal, &mut picture).unwrap();
                for index in 0..pixels {
                    assert!((64..=940).contains(&sample(&picture, index)));
                }
                for index in pixels..pixels * 3 / 2 {
                    assert!((64..=960).contains(&sample(&picture, index)));
                }
                for row in 0..height {
                    for column in 0..width {
                        let expected = probe
                            .pixel(
                                ordinal,
                                [u32::try_from(column).unwrap(), u32::try_from(row).unwrap()],
                            )
                            .unwrap();
                        let chroma = (row / 2) * (width / 2) + column / 2;
                        assert_eq!(sample(&picture, row * width + column), expected[0]);
                        assert_eq!(sample(&picture, pixels + chroma), expected[1]);
                        assert_eq!(sample(&picture, pixels + pixels / 4 + chroma), expected[2]);
                    }
                }
            }
        }
    }
}

#[test]
fn every_ordinal_is_distinct_and_invalid_requests_leave_buffers_unchanged() {
    let probe = HdrEncoderProbe::new([320, 180], [60, 1], HdrTransfer::Pq).unwrap();
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
    let mut strips = std::collections::BTreeSet::new();
    for ordinal in 0..probe.config().video_frames {
        probe.fill_picture(ordinal, &mut picture).unwrap();
        assert!(strips.insert(picture[..640].to_vec()));
    }
    let mut untouched = vec![7; picture.len()];
    assert!(
        probe
            .fill_picture(probe.config().video_frames, &mut untouched)
            .is_err()
    );
    assert!(probe.fill_picture(0, &mut untouched[..5]).is_err());
    assert!(untouched.iter().all(|byte| *byte == 7));
    assert!(probe.pixel(0, [320, 0]).is_err());
    let mut left = [0.0; 1024];
    let mut right = [0.0; 1024];
    probe.fill_audio(0, &mut left, &mut right).unwrap();
    assert_eq!(left[100], 0.75);
    assert_eq!(right[137], -0.625);
}
