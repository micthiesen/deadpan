use deadpan_encode::{
    BFramePolicy, ContentLight, EncodeContract, EncodeFailureKind, EncoderMode,
    HDR_POLICY_VERSION_V1, HdrSignal, HdrTransfer, MasteringDisplay, VideoFormat,
};

const BT2020_1000: MasteringDisplay = MasteringDisplay {
    primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
    white_point: [15_635, 16_450],
    max_luminance: 10_000_000,
    min_luminance: 50,
};

fn pq(mastering: Option<MasteringDisplay>) -> HdrSignal {
    HdrSignal {
        transfer: HdrTransfer::Pq,
        mastering,
    }
}

fn hdr(
    raster: [u32; 2],
    rate: [u32; 2],
    frames: u64,
    samples: u64,
    signal: HdrSignal,
) -> Result<EncodeContract, deadpan_encode::EncodeError> {
    EncodeContract::new_hdr_v1(
        raster,
        rate,
        frames,
        samples,
        EncoderMode::Hardware,
        BFramePolicy::TargetTwo,
        signal,
    )
}

#[test]
fn frozen_hdr_version_one_bitrates_are_sdr_v1_times_five_quarters_half_up() {
    assert_eq!(HDR_POLICY_VERSION_V1, 1);
    for (raster, rate, samples, sdr, expected) in [
        ([1920, 1080], [30, 1], 1600, 8_000_000, 10_000_000),
        ([1920, 1080], [60, 1], 800, 12_000_000, 15_000_000),
        ([3840, 2160], [30, 1], 1600, 45_000_000, 56_250_000),
        ([3840, 2160], [60_000, 1001], 801, 68_000_000, 85_000_000),
        ([7680, 4320], [60, 1], 800, 240_000_000, 300_000_000),
        ([1000, 1440], [30, 1], 1600, 6_350_000, 7_937_500),
        // 3001250*5 = 15006250, /4 = 3751562.5 -> half up 3751563.
        ([854, 480], [30, 1], 1600, 3_001_250, 3_751_563),
        // 4002188*5 = 20010940, /4 = 5002735 exactly.
        ([854, 480], [60, 1], 800, 4_002_188, 5_002_735),
        // 5204167*5 = 26020835, /4 = 6505208.75 -> 6505209.
        ([1000, 1000], [30, 1], 1600, 5_204_167, 6_505_209),
    ] {
        let sdr_contract = EncodeContract::new_v1(
            raster,
            rate,
            1,
            samples,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo,
        )
        .unwrap();
        assert_eq!(sdr_contract.policy().video_bitrate, sdr);
        let contract = hdr(raster, rate, 1, samples, pq(Some(BT2020_1000))).unwrap();
        assert_eq!(contract.policy().video_bitrate, expected);
        assert_eq!(expected, (sdr * 5 + 2) / 4);
        assert_eq!(
            contract.policy().gop_frames,
            sdr_contract.policy().gop_frames
        );
        assert_eq!(
            contract.policy().movie_timescale,
            sdr_contract.policy().movie_timescale
        );
        assert_eq!(contract.policy().audio_bitrate, 384_000);
        assert_eq!(contract.policy().b_frames, 2);
        assert_eq!(contract.picture_bytes(), 2 * sdr_contract.picture_bytes());
        assert_eq!(contract.video_format(), VideoFormat::HevcMain10Rec2100Pq);
        assert_eq!(sdr_contract.video_format(), VideoFormat::H264Rec709I420);
        assert!(sdr_contract.hdr().is_none());
    }
}

#[test]
fn hdr_serialization_adds_only_the_hdr_object() {
    let signal = HdrSignal {
        transfer: HdrTransfer::Hlg,
        mastering: None,
    };
    let contract = EncodeContract::new_hdr_v1(
        [320, 180],
        [30, 1],
        1,
        1600,
        EncoderMode::Software,
        BFramePolicy::None,
        signal,
    )
    .unwrap();
    assert_eq!(contract.video_format(), VideoFormat::HevcMain10Rec2100Hlg);
    assert_eq!(
        serde_json::to_string(&contract).unwrap(),
        concat!(
            r#"{"raster":[320,180],"frame_rate":[30,1],"video_frames":1,"audio_samples":1600,"#,
            r#""mode":"software","policy":{"video_bitrate":1875000,"audio_bitrate":384000,"#,
            r#""gop_frames":15,"b_frames":0,"movie_timescale":48000},"picture_bytes":172800,"#,
            r#""hdr":{"policy_version":1,"signal":{"transfer":"hlg"}}}"#
        )
    );
    let pq_json = serde_json::to_string(&pq(Some(BT2020_1000))).unwrap();
    assert_eq!(
        pq_json,
        concat!(
            r#"{"transfer":"pq","mastering":{"primaries":[[35400,14600],[8500,39850],[6550,2300]],"#,
            r#""white_point":[15635,16450],"max_luminance":10000000,"min_luminance":50}}"#
        )
    );
    assert_eq!(
        serde_json::from_str::<HdrSignal>(&pq_json).unwrap(),
        pq(Some(BT2020_1000))
    );
    for json in [
        r#"{"transfer":"pq","extra":1}"#,
        r#"{"transfer":"sdr"}"#,
        r#"{"transfer":"pq","mastering":{"primaries":[[1,1],[1,1],[1,1]],"white_point":[1,1],"max_luminance":1}}"#,
    ] {
        assert!(serde_json::from_str::<HdrSignal>(json).is_err(), "{json}");
    }
    assert_eq!(
        serde_json::to_string(&ContentLight {
            max_cll: 1000,
            max_fall: 400
        })
        .unwrap(),
        r#"{"max_cll":1000,"max_fall":400}"#
    );
}

#[test]
fn hlg_mastering_and_invalid_mastering_are_rejected_before_native_work() {
    let reject = |signal: HdrSignal| {
        assert_eq!(
            hdr([320, 180], [30, 1], 1, 1600, signal)
                .unwrap_err()
                .kind(),
            EncodeFailureKind::Configuration
        );
    };
    reject(HdrSignal {
        transfer: HdrTransfer::Hlg,
        mastering: Some(BT2020_1000),
    });
    let with = |change: fn(&mut MasteringDisplay)| {
        let mut value = BT2020_1000;
        change(&mut value);
        pq(Some(value))
    };
    reject(with(|m| m.primaries[0][0] = 50_001));
    reject(with(|m| m.primaries[1] = [40_000, 10_001]));
    reject(with(|m| m.white_point = [0, 16_450]));
    reject(with(|m| m.primaries.swap(0, 1)));
    reject(with(|m| m.white_point = [40_000, 5_000]));
    reject(with(|m| m.max_luminance = 499_999));
    reject(with(|m| m.max_luminance = 100_000_001));
    reject(with(|m| m.min_luminance = m.max_luminance));
    reject(with(|m| m.min_luminance = 500_001));
    hdr([320, 180], [30, 1], 1, 1600, pq(None)).unwrap();
    hdr(
        [320, 180],
        [30, 1],
        1,
        1600,
        with(|m| {
            m.max_luminance = 500_000;
            m.min_luminance = 0;
        }),
    )
    .unwrap();
    // Ordinary SDR admission rules still apply to HDR contracts.
    assert!(hdr([321, 180], [30, 1], 1, 1600, pq(None)).is_err());
    assert!(hdr([320, 180], [61, 1], 1, 800, pq(None)).is_err());
    assert!(hdr([320, 180], [30, 1], 1, 1500, pq(None)).is_err());
}

#[test]
fn content_light_bounds() {
    for (light, valid) in [
        ((0, 0), true),
        ((10_000, 10_000), true),
        ((1_000, 400), true),
        ((10_001, 1), false),
        ((100, 101), false),
    ] {
        let value = ContentLight {
            max_cll: light.0,
            max_fall: light.1,
        };
        assert_eq!(value.validate().is_ok(), valid, "{light:?}");
    }
}

#[test]
fn mastering_and_content_light_rules_match_the_shared_core_rule_set() {
    for (index, (shared, expected)) in deadpan_core::mastering_rule_cases().into_iter().enumerate()
    {
        let local = MasteringDisplay {
            primaries: shared.primaries,
            white_point: shared.white_point,
            max_luminance: shared.max_luminance,
            min_luminance: shared.min_luminance,
        };
        assert_eq!(local.validate().is_ok(), expected.is_ok(), "case {index}");
        assert_eq!(
            pq(Some(local)).validate().is_ok(),
            expected.is_ok(),
            "case {index}"
        );
    }
    for (index, (shared, expected)) in deadpan_core::content_light_rule_cases()
        .into_iter()
        .enumerate()
    {
        let local = ContentLight {
            max_cll: shared.max_cll,
            max_fall: shared.max_fall,
        };
        assert_eq!(local.validate().is_ok(), expected, "case {index}");
    }
}
