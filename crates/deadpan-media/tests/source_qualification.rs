#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{AssetId, ExactRatio, FrameRate};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::{
    DecodedSourceQualification, MAX_SOURCE_QUALIFICATION_JSON_BYTES, QUALIFIED_SOURCE_ASSET_ID,
    SOURCE_IMPORT_TIMING_POLICY_VERSION, SOURCE_QUALIFICATION_DECODER_CONTRACT,
    SOURCE_QUALIFICATION_VERSION, SourceQualificationError, SourceQualificationSnapshot,
};
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::{ColorMatrix, ColorRange, ColorTransfer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn input(folder: &str, name: &str) -> VerifiedSourceInput {
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native")
            .join(folder)
            .join(name),
    )
    .unwrap();
    let identity =
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap();
    VerifiedSourceInput::copy_verified(
        &mut Cursor::new(bytes),
        identity,
        identity.byte_length(),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn video(input: VerifiedSourceInput, alias: &str) -> SourceSession {
    SourceSession::open_input(
        input,
        AssetId::new(alias).unwrap(),
        SourceSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn audio(input: VerifiedSourceInput, stream: u32) -> AudioSession {
    AudioSession::open_input(
        input,
        stream,
        AudioSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn av(name: &str) -> (SourceSession, AudioSession) {
    let input = input("deadpan-source/tests/fixtures", name);
    (video(input.clone(), "caller-asset"), audio(input, 1))
}

fn snapshot_value() -> Value {
    let (video, audio) = av("offset-bframes.mp4");
    let captured = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).unwrap();
    serde_json::from_slice(&captured.snapshot().to_json().unwrap()).unwrap()
}

#[test]
fn field_receipts_keep_progressive_cadence_audio_and_temporal_seek_anchors() {
    use deadpan_core::SourceFrameId;
    for (name, rate, samples, count) in [
        ("fields-tff.mp4", FrameRate::new(50, 1).unwrap(), 23040, 24),
        (
            "fields-tff-bframes.mp4",
            FrameRate::new(50, 1).unwrap(),
            23040,
            24,
        ),
        (
            "fields-bff.mp4",
            FrameRate::new(60000, 1001).unwrap(),
            19219,
            24,
        ),
        ("fields-single.mp4", FrameRate::new(50, 1).unwrap(), 1920, 2),
    ] {
        let (mut video, audio) = av(name);
        let captured =
            DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).unwrap();
        let bytes = captured.snapshot().to_json().unwrap();
        let restored = SourceQualificationSnapshot::from_json(&bytes).unwrap();
        assert_eq!(restored, *captured.snapshot());
        if count == 24 {
            let mut damaged: Value = serde_json::from_slice(&bytes).unwrap();
            let frames = damaged["video"]["index"]["index"]["frames"]
                .as_array_mut()
                .unwrap();
            frames[6]["seek_from"] = json!(6);
            assert!(
                SourceQualificationSnapshot::from_json(&serde_json::to_vec(&damaged).unwrap())
                    .is_err()
            );
        }
        assert!(restored.video().unwrap().interpretation().bwdif_fields);
        let mut unsupported: Value = serde_json::from_slice(&bytes).unwrap();
        unsupported["video"]["interpretation"]["pixel_format"] = json!("nv12");
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&unsupported).unwrap())
                .is_err()
        );
        assert_eq!(
            restored
                .basis_candidate()
                .unwrap()
                .unwrap()
                .basis
                .frame_rate,
            rate
        );
        let timing = restored.derive_timing(rate).unwrap();
        assert_eq!(
            timing.video.unwrap().duration_frames,
            ExactRatio::integer(count)
        );
        assert_eq!(timing.audio.unwrap().span.start().ticks, 0);
        assert_eq!(timing.audio.unwrap().span.end().ticks, samples);
        let mut pixels = Vec::new();
        for id in 0..u64::try_from(count).unwrap() {
            pixels.push(
                video
                    .frame(
                        SourceFrameId(id),
                        Duration::from_secs(5),
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                    .rgba,
            );
            // First field of a GOP can be filtered only with the preceding GOP.
            let expected_anchor = if id < 6 { 0 } else { (id / 6 - 1) * 6 };
            assert_eq!(
                video.index().index().frames()[id as usize].seek_from,
                Some(SourceFrameId(expected_anchor))
            );
        }
        for id in (0..u64::try_from(count).unwrap()).rev() {
            assert_eq!(
                video
                    .frame(
                        SourceFrameId(id),
                        Duration::from_secs(5),
                        &AtomicBool::new(false)
                    )
                    .unwrap()
                    .rgba,
                pixels[id as usize],
                "{name} field {id}"
            );
        }
    }
}

#[test]
fn real_offset_and_vfr_capture_preserve_complete_interpretation_and_timing_after_roundtrip() {
    for (name, picture_frames, audio_frames, occupancy) in [
        (
            "offset-bframes.mp4",
            120,
            ExactRatio::new(120760, 1001).unwrap(),
            121,
        ),
        ("vfr.mp4", 238, ExactRatio::integer(240), 240),
    ] {
        let (video, audio) = av(name);
        let captured =
            DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).unwrap();
        let snapshot = captured.snapshot();
        assert_eq!(snapshot.content(), video.index().content());
        assert_eq!(snapshot.audio().unwrap(), audio.index());
        assert_eq!(snapshot.video().unwrap().interpretation(), video.info());
        assert_eq!(
            snapshot.video().unwrap().index().index().asset().as_str(),
            QUALIFIED_SOURCE_ASSET_ID
        );
        assert_eq!(
            snapshot.video().unwrap().index().index().frames(),
            video.index().index().frames()
        );
        let bytes = snapshot.to_json().unwrap();
        let restored = SourceQualificationSnapshot::from_json(&bytes).unwrap();
        assert_eq!(restored, *snapshot);
        assert_eq!(restored.to_json().unwrap(), bytes);
        let metadata: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(metadata["schema_version"], SOURCE_QUALIFICATION_VERSION);
        assert_eq!(
            metadata["decoder_contract"],
            SOURCE_QUALIFICATION_DECODER_CONTRACT
        );
        assert_eq!(
            metadata["timing_policy_version"],
            SOURCE_IMPORT_TIMING_POLICY_VERSION
        );
        let timing = restored
            .derive_timing(FrameRate::new(30000, 1001).unwrap())
            .unwrap();
        assert_eq!(
            timing.video.unwrap().duration_frames,
            ExactRatio::integer(picture_frames)
        );
        assert_eq!(timing.audio.unwrap().duration_frames, audio_frames);
        assert_eq!(timing.duration.frames(), occupancy);
        assert_eq!(restored.origin_seconds(), timing.origin_seconds);
        assert!(restored.basis_candidate().unwrap().is_some());
        if name == "offset-bframes.mp4" {
            assert_eq!(
                snapshot.origin_seconds(),
                ExactRatio::new(2971, 1500).unwrap()
            );
            assert_eq!(
                metadata["origin_seconds"],
                json!({"numerator":"2971", "denominator":"1500"})
            );
            assert_eq!(timing.audio.unwrap().span.start().ticks, 95072);
            assert_eq!(timing.audio.unwrap().span.end().ticks, 288288);
            assert!(
                restored.audio().unwrap().observations()[0]
                    .skip_samples
                    .is_none()
            );
        }
    }
}

#[test]
fn fractional_aperture_roundtrips_exact_bounds_and_reopens_the_retained_source() {
    use std::sync::Arc;
    let (video, audio) = av("aperture-fractional.mp4");
    let captured = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).unwrap();
    let bytes = captured.snapshot().to_json().unwrap();
    let restored = SourceQualificationSnapshot::from_json(&bytes).unwrap();
    assert_eq!(&restored, captured.snapshot());
    let expected =
        [(53, 4), (37, 4), (599, 2), (319, 2)].map(|(n, d)| ExactRatio::new(n, d).unwrap());
    let qualified = restored.video().unwrap();
    assert_eq!(qualified.interpretation().clean_aperture, Some(expected));
    assert_eq!(
        restored.basis_candidate().unwrap().unwrap().basis.width,
        300
    );
    let cancelled = AtomicBool::new(false);
    let mut reopened = SourceSession::open_input_indexed(
        input("deadpan-source/tests/fixtures", "aperture-fractional.mp4"),
        Arc::new(qualified.index().clone()),
        qualified.interpretation(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let picture = reopened
        .frame(
            deadpan_core::SourceFrameId(119),
            Duration::from_secs(10),
            &cancelled,
        )
        .unwrap();
    assert_eq!((picture.width, picture.height), (320, 180));
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for rect in [
        [
            ExactRatio::integer(-1),
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ONE,
        ],
        [
            ExactRatio::ZERO,
            ExactRatio::ZERO,
            ExactRatio::integer(321),
            ExactRatio::ONE,
        ],
        [
            ExactRatio::new(i128::MAX, 1).unwrap(),
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ONE,
        ],
    ] {
        let mut changed = original.clone();
        changed["video"]["interpretation"]["clean_aperture"] = serde_json::to_value(rect).unwrap();
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&changed).unwrap()).is_err()
        );
    }
    let mut proxy = video.info().clone();
    proxy.width = 160;
    proxy.height = 90;
    proxy.clean_aperture = None;
    for rotation in 0..4 {
        proxy.rotation_quarter_turns = rotation;
        let shown = deadpan_media::proxy::presentation_info(&proxy, video.info()).unwrap();
        assert_eq!(
            shown.clean_aperture,
            Some(expected.map(|v| v.checked_div(ExactRatio::integer(2)).unwrap()))
        );
        assert_eq!(shown.rotation_quarter_turns, rotation);
    }
    proxy.clean_aperture = Some(expected);
    assert!(deadpan_media::proxy::presentation_info(&proxy, video.info()).is_err());
}

#[test]
fn persisted_normalization_origin_is_required_and_bound_to_measured_spans() {
    let original = snapshot_value();
    for malformed in [
        Value::Null,
        json!({"numerator":"0", "denominator":"1"}),
        json!({"numerator":"2971", "denominator":"0"}),
        json!({"numerator":"2971", "denominator":"1500", "unknown":null}),
        json!({"numerator":2971, "denominator":1500}),
    ] {
        let mut value = original.clone();
        value["origin_seconds"] = malformed;
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    let mut missing = original;
    missing.as_object_mut().unwrap().remove("origin_seconds");
    assert!(
        SourceQualificationSnapshot::from_json(&serde_json::to_vec(&missing).unwrap()).is_err()
    );
}

#[test]
fn exact_negative_origin_roundtrips_without_changing_relative_av_placement() {
    let original = snapshot_value();
    let original_snapshot =
        SourceQualificationSnapshot::from_json(&serde_json::to_vec(&original).unwrap()).unwrap();
    let original_timing = original_snapshot
        .derive_timing(FrameRate::new(30000, 1001).unwrap())
        .unwrap();
    let mut translated = original;
    // Translate retained observations by ten seconds in each original clock.
    // This tests metadata validation, and does not mint a live decode token.
    let shift = |value: &mut Value, ticks: i64| {
        if let Some(original) = value.as_i64() {
            *value = (original - ticks).into();
        }
    };
    for frame in translated["video"]["index"]["index"]["frames"]
        .as_array_mut()
        .unwrap()
    {
        shift(&mut frame["pts"], 300000);
        shift(&mut frame["decode_timestamp"], 300000);
    }
    shift(
        &mut translated["video"]["index"]["index"]["terminal_end"],
        300000,
    );
    shift(
        &mut translated["video"]["interpretation"]["stream_start"],
        300000,
    );
    shift(
        &mut translated["video"]["interpretation"]["container_start"],
        10000000,
    );
    shift(
        &mut translated["video"]["interpretation"]["audio_streams"][0]["stream_start"],
        480000,
    );
    shift(&mut translated["audio"]["stream"]["stream_start"], 480000);
    for frame in translated["audio"]["observations"].as_array_mut().unwrap() {
        shift(&mut frame["pts"], 480000);
        shift(&mut frame["decode_timestamp"], 480000);
    }
    translated["origin_seconds"] = json!({"numerator":"-12029", "denominator":"1500"});
    let restored =
        SourceQualificationSnapshot::from_json(&serde_json::to_vec(&translated).unwrap()).unwrap();
    assert_eq!(
        restored.origin_seconds(),
        ExactRatio::new(-12029, 1500).unwrap()
    );
    let bytes = restored.to_json().unwrap();
    assert_eq!(
        SourceQualificationSnapshot::from_json(&bytes).unwrap(),
        restored
    );
    let timing = restored
        .derive_timing(FrameRate::new(30000, 1001).unwrap())
        .unwrap();
    assert_eq!(timing.origin_seconds, restored.origin_seconds());
    assert_eq!(timing.duration, original_timing.duration);
    assert_eq!(
        timing.video.unwrap().start_frames,
        original_timing.video.unwrap().start_frames
    );
    assert_eq!(
        timing.video.unwrap().duration_frames,
        original_timing.video.unwrap().duration_frames
    );
    assert_eq!(
        timing.audio.unwrap().start_frames,
        original_timing.audio.unwrap().start_frames
    );
    assert_eq!(
        timing.audio.unwrap().duration_frames,
        original_timing.audio.unwrap().duration_frames
    );
}

#[test]
fn canonical_capture_is_independent_of_callers_asset_alias() {
    let input = input("deadpan-media-worker/tests/fixtures", "rgb25_24.mp4");
    let first = video(input.clone(), "first-label");
    let second = video(input, "other-label");
    let first = DecodedSourceQualification::from_sessions(Some(&first), None).unwrap();
    let second = DecodedSourceQualification::from_sessions(Some(&second), None).unwrap();
    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(
        first.snapshot().to_json().unwrap(),
        second.snapshot().to_json().unwrap()
    );
}

#[test]
fn source_color_and_geometry_are_retained_separately_from_sdr_project_policy() {
    for (folder, name, matrix, range, transfer) in [
        (
            "deadpan-source/tests/fixtures",
            "limited709.mkv",
            ColorMatrix::Bt709,
            ColorRange::Limited,
            ColorTransfer::Bt709,
        ),
        (
            "deadpan-source/tests/fixtures",
            "full709.mkv",
            ColorMatrix::Bt709,
            ColorRange::Full,
            ColorTransfer::Bt709,
        ),
        (
            "deadpan-media-worker/tests/fixtures",
            "rgb1_24.mp4",
            ColorMatrix::Rgb,
            ColorRange::Full,
            ColorTransfer::Srgb,
        ),
        (
            "deadpan-source/tests/fixtures",
            "anamorphic.mkv",
            ColorMatrix::Bt709,
            ColorRange::Limited,
            ColorTransfer::Bt709,
        ),
        (
            "deadpan-source/tests/fixtures",
            "rotated90.mp4",
            ColorMatrix::Rgb,
            ColorRange::Full,
            ColorTransfer::Srgb,
        ),
    ] {
        let video = video(input(folder, name), "display-source");
        let capture = DecodedSourceQualification::from_sessions(Some(&video), None).unwrap();
        let snapshot =
            SourceQualificationSnapshot::from_json(&capture.snapshot().to_json().unwrap()).unwrap();
        let info = snapshot.video().unwrap().interpretation();
        assert_eq!(info, video.info());
        assert_eq!(
            (info.color.matrix, info.color.range, info.color.transfer),
            (matrix, range, transfer)
        );
        assert_eq!(
            snapshot
                .basis_candidate()
                .unwrap()
                .unwrap()
                .basis
                .color_policy,
            deadpan_core::ColorPolicy::SdrRec709
        );
    }
}

#[test]
fn audio_only_capture_keeps_original_clock_without_creating_picture_evidence() {
    let audio = audio(
        input("deadpan-source/tests/audio-fixtures", "pcm-mono-44100.wav"),
        0,
    );
    let captured = DecodedSourceQualification::from_sessions(None, Some(&audio)).unwrap();
    let restored =
        SourceQualificationSnapshot::from_json(&captured.snapshot().to_json().unwrap()).unwrap();
    assert_eq!(restored.audio().unwrap(), audio.index());
    assert!(restored.video().is_none());
    assert!(restored.basis_candidate().unwrap().is_none());
    let timing = restored
        .derive_timing(FrameRate::new(30, 1).unwrap())
        .unwrap();
    assert_eq!(
        timing.audio.unwrap().duration_frames,
        ExactRatio::new(44117, 1470).unwrap()
    );
    assert_eq!(timing.audio.unwrap().span.end().ticks, 44117);
    assert_eq!(timing.duration.frames(), 31);
    assert!(DecodedSourceQualification::from_sessions(None, None).is_err());
}

#[test]
fn unlabelled_sound_channels_need_an_explicit_retained_interpretation() {
    use deadpan_media::audio_index::{AudioChannelLayout, AudioLayoutInterpretation as Choice};
    use deadpan_media::source_qualification::SourceQualificationError;
    let stereo = audio(
        input(
            "deadpan-source/tests/audio-fixtures",
            "pcm-stereo-48000.wav",
        ),
        0,
    );
    let mono = audio(
        input("deadpan-source/tests/audio-fixtures", "pcm-mono-44100.wav"),
        0,
    );
    assert_eq!(
        stereo.index().stream().channel_layout,
        AudioChannelLayout::Unspecified { channels: 2 }
    );
    // A sound registration refuses silence about speakers and a wrong count.
    for choice in [None, Some(Choice::Mono)] {
        let Err(SourceQualificationError::AudioLayoutInterpretation(message)) =
            DecodedSourceQualification::for_registration(None, Some(&stereo), choice)
        else {
            panic!("an unlabelled stereo sound needs its explicit interpretation");
        };
        assert!(
            message.contains("Stereo L/R (stereo_left_right)"),
            "{message}"
        );
        assert!(!message.contains("Mono (mono)"), "{message}");
    }
    assert!(matches!(
        DecodedSourceQualification::for_registration(
            None,
            Some(&mono),
            Some(Choice::StereoLeftRight)
        ),
        Err(SourceQualificationError::AudioLayoutInterpretation(_))
    ));
    // The chosen reading is retained beside the unchanged measurement and
    // survives the canonical round trip; it changes the receipt bytes.
    let plain = DecodedSourceQualification::from_sessions(None, Some(&stereo)).unwrap();
    let chosen = DecodedSourceQualification::for_registration(
        None,
        Some(&stereo),
        Some(Choice::StereoLeftRight),
    )
    .unwrap();
    let bytes = chosen.snapshot().to_json().unwrap();
    assert_ne!(bytes, plain.snapshot().to_json().unwrap());
    assert!(
        !String::from_utf8_lossy(&plain.snapshot().to_json().unwrap())
            .contains("audio_interpretation")
    );
    let restored = SourceQualificationSnapshot::from_json(&bytes).unwrap();
    assert_eq!(restored.audio().unwrap(), stereo.index());
    assert_eq!(
        restored.audio_interpretation(),
        Some(Choice::StereoLeftRight)
    );
    assert_eq!(
        restored.audio_layout(),
        Some(AudioChannelLayout::Native {
            channels: 2,
            mask: 3
        })
    );
    assert_eq!(
        plain.snapshot().audio_layout(),
        Some(AudioChannelLayout::Unspecified { channels: 2 })
    );
    let mono_chosen =
        DecodedSourceQualification::for_registration(None, Some(&mono), Some(Choice::Mono))
            .unwrap();
    assert_eq!(
        mono_chosen.snapshot().audio_layout(),
        Some(AudioChannelLayout::Native {
            channels: 1,
            mask: 4
        })
    );
    // Persisted bytes cannot attach a reading to the wrong channel count.
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["audio_interpretation"] = json!("mono");
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    value["audio_interpretation"] = Value::Null;
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    value["audio_interpretation"] = json!("surround");
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    // A declared layout is used as is: the choice is not retained there, and
    // an Original video keeps its existing admission without one.
    let (video, declared) = av("cfr-bframes.mp4");
    assert!(matches!(
        declared.index().stream().channel_layout,
        AudioChannelLayout::Native { .. }
    ));
    let original = DecodedSourceQualification::for_registration(
        Some(&video),
        Some(&declared),
        Some(Choice::StereoLeftRight),
    )
    .unwrap();
    assert_eq!(original.snapshot().audio_interpretation(), None);
    let mut value: Value = serde_json::from_slice(&original.snapshot().to_json().unwrap()).unwrap();
    value["audio_interpretation"] = json!("stereo_left_right");
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn sessions_from_different_originals_cannot_make_a_live_qualification() {
    let (video, _) = av("offset-bframes.mp4");
    let (_, audio) = av("cfr-bframes.mp4");
    assert!(DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).is_err());
}

#[test]
fn malformed_snapshots_reject_inconsistent_identity_clock_geometry_color_and_inventory() {
    let original = snapshot_value();
    for case in 0..42 {
        let mut value = original.clone();
        match case {
            0 => value["content"]["sha256"][0] = 0.into(),
            1 => value["video"]["index"]["content"]["byte_length"] = 1.into(),
            2 => value["audio"]["content"]["byte_length"] = 1.into(),
            3 => value["video"]["index"]["index"]["asset"] = "other-alias".into(),
            4 => value["video"]["interpretation"]["stream_index"] = 1.into(),
            5 => value["video"]["interpretation"]["time_base_den"] = 25.into(),
            6 => value["video"]["interpretation"]["width"] = 0.into(),
            7 => value["video"]["interpretation"]["height"] = 8193.into(),
            8 => value["video"]["interpretation"]["sample_aspect_num"] = 0.into(),
            9 => value["video"]["interpretation"]["sample_aspect_den"] = u32::MAX.into(),
            10 => value["video"]["interpretation"]["rotation_quarter_turns"] = 4.into(),
            11 => value["video"]["interpretation"]["codec"] = "hevc".into(),
            12 => value["video"]["interpretation"]["pixel_format"] = "yuv420p10le".into(),
            13 => value["video"]["interpretation"]["color"]["range"] = "unknown".into(),
            14 => value["video"]["interpretation"]["color"]["matrix"] = "rgb".into(),
            15 => value["video"]["interpretation"]["color"]["transfer"] = "pq".into(),
            16 => value["video"]["interpretation"]["color"]["primaries"] = "unknown".into(),
            17 => value["video"]["interpretation"]["audio_streams"] = json!([]),
            18 => value["video"]["interpretation"]["audio_streams"][0]["stream_index"] = 0.into(),
            19 => {
                value["video"]["interpretation"]["audio_streams"][0]["codec"] = "pcm_s16le".into()
            }
            20 => {
                value["video"]["interpretation"]["audio_streams"][0]["time_base_den"] = 44100.into()
            }
            21 => {
                value["video"]["interpretation"]["audio_streams"][0]["sample_rate"] = 44100.into()
            }
            22 => value["video"]["interpretation"]["audio_streams"][0]["channel_count"] = 31.into(),
            23 => value["video"]["interpretation"]["audio_streams"][0]["stream_start"] = 0.into(),
            24 => value["video"]["interpretation"]["stream_duration"] = 0.into(),
            25 => value["video"]["interpretation"]["container_duration"] = (-1).into(),
            26 => value["video"]["interpretation"]["stream_start"] = i64::MIN.into(),
            27 => {
                value["video"]["interpretation"]["audio_streams"][0]["codec"] =
                    "x".repeat(32).into()
            }
            28 => {
                let entry = value["video"]["interpretation"]["audio_streams"][0].clone();
                value["video"]["interpretation"]["audio_streams"] = json!([entry.clone(), entry]);
            }
            29 => value["video"]["index"]["index"]["terminal_provenance"] = "container_end".into(),
            30 => {
                let frames = value["video"]["index"]["index"]["frames"]
                    .as_array_mut()
                    .unwrap();
                frames.last_mut().unwrap()["reported_duration"] = Value::Null;
            }
            31 => value["audio"]["observations"][10]["discard"] = true.into(),
            32 => {
                value["audio"]["observations"][10]["skip_samples"] =
                    json!({"leading": 1, "trailing": 0, "leading_reason": 0, "trailing_reason": 0})
            }
            33 => {
                value["video"]["index"]["index"]["frames"][1]["decode_timestamp"] = i64::MIN.into()
            }
            34 => value["audio"]["observations"][10]["decode_timestamp"] = i64::MIN.into(),
            35 => {
                value["video"] = Value::Null;
                value["audio"]["stream"]["stream_start"] = i64::MIN.into();
            }
            36 => {
                value["video"] = Value::Null;
                value["audio"]["stream"]["stream_duration"] = (-1).into();
            }
            37 => {
                value["video"] = Value::Null;
                value["audio"]["stream"]["initial_padding"] = u32::MAX.into();
            }
            38 => {
                let entry = value["video"]["interpretation"]["audio_streams"][0].clone();
                value["video"]["interpretation"]["audio_streams"] = Value::Array(vec![entry; 33]);
            }
            39 => value["video"]["index"]["schema_version"] = 2.into(),
            40 => value["audio"]["decoder_contract"] = "unknown".into(),
            _ => {
                let frames = value["video"]["index"]["index"]["frames"]
                    .as_array_mut()
                    .unwrap();
                frames.last_mut().unwrap()["reported_duration"] = 2.into();
            }
        }
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn snapshots_reject_unknown_even_null_fields_and_unknown_or_null_versions() {
    let original = snapshot_value();
    for path in [
        "",
        "/video",
        "/video/interpretation",
        "/video/interpretation/color",
        "/video/interpretation/audio_streams/0",
        "/video/index",
        "/video/index/index",
        "/audio",
        "/audio/stream",
        "/audio/observations/0",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), Value::Null);
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err(),
            "path {path}"
        );
    }
    for field in [
        "schema_version",
        "decoder_contract",
        "timing_policy_version",
    ] {
        for replacement in [Value::Null, json!(999), json!("unknown")] {
            let mut value = original.clone();
            value[field] = replacement;
            assert!(
                SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap())
                    .is_err(),
                "field {field}"
            );
        }
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    for field in ["video", "audio"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    let mut value = original.clone();
    value["video"] = Value::Null;
    value["audio"] = Value::Null;
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value = original;
    value["video"]["interpretation"]
        .as_object_mut()
        .unwrap()
        .remove("container_start");
    assert!(SourceQualificationSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn combined_input_byte_limit_is_checked_before_json_parsing() {
    let oversized = vec![b' '; MAX_SOURCE_QUALIFICATION_JSON_BYTES + 1];
    assert!(matches!(
        SourceQualificationSnapshot::from_json(&oversized),
        Err(SourceQualificationError::Limit)
    ));
}

#[test]
fn hdr_interpretation_and_static_metadata_round_trip_and_select_hdr_policy() {
    use deadpan_core::{ColorPolicy, SourceFrameId};
    use deadpan_media::proxy::{
        ProxyIneligible, ProxyPlan, ProxyReason, proxy_plan, proxy_request,
    };
    use deadpan_source::{ColorPrimaries, ContentLight, MasteringDisplay};
    let mastering = MasteringDisplay {
        primaries: [[34_000, 16_000], [13_250, 34_500], [7_500, 3_000]],
        white_point: [15_635, 16_450],
        max_luminance: 10_000_000,
        min_luminance: 1,
    };
    for (name, transfer, policy, static_metadata) in [
        (
            "hevc-pq.mp4",
            ColorTransfer::Pq,
            ColorPolicy::HdrRec2020Pq,
            true,
        ),
        (
            "hevc-hlg.mp4",
            ColorTransfer::Hlg,
            ColorPolicy::HdrRec2020Hlg,
            false,
        ),
        (
            "hdr-pq-av.mp4",
            ColorTransfer::Pq,
            ColorPolicy::HdrRec2020Pq,
            true,
        ),
    ] {
        let mut video = video(input("deadpan-source/tests/fixtures", name), "hdr-source");
        let capture = DecodedSourceQualification::from_sessions(Some(&video), None).unwrap();
        let json = capture.snapshot().to_json().unwrap();
        let snapshot = SourceQualificationSnapshot::from_json(&json).unwrap();
        let info = snapshot.video().unwrap().interpretation();
        assert_eq!(info, video.info(), "{name}");
        assert_eq!(info.color.transfer, transfer);
        assert_eq!(info.color.primaries, ColorPrimaries::Bt2020);
        assert_eq!(info.color.matrix, ColorMatrix::Bt2020NonConstant);
        assert_eq!(info.color.mastering, static_metadata.then_some(mastering));
        assert_eq!(
            info.color.content_light,
            static_metadata.then_some(ContentLight {
                max_cll: 1000,
                max_fall: 400
            })
        );
        let text = String::from_utf8(json).unwrap();
        assert_eq!(text.contains("\"mastering\":"), static_metadata, "{name}");
        assert_eq!(
            snapshot
                .basis_candidate()
                .unwrap()
                .unwrap()
                .basis
                .color_policy,
            policy,
            "{name}"
        );
        // The picture path reads HDR sources at sixteen bits.
        let picture = video
            .frame(
                SourceFrameId(0),
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(picture.sample_bits, 16);
        assert_eq!(picture.rgba.len(), (info.width * info.height * 8) as usize);
        // HDR Originals never get an eight-bit SDR proxy, even when planned.
        assert_eq!(
            proxy_plan(info, video.index().index()),
            Err(ProxyIneligible::HighDynamicRange)
        );
        let forced = ProxyPlan {
            width: 64,
            height: 36,
            reason: ProxyReason::Requested,
        };
        assert_eq!(
            proxy_request(100, info, 8, &forced, Duration::from_secs(1)),
            Err(ProxyIneligible::HighDynamicRange)
        );
    }
}

#[test]
fn sdr_color_wire_is_unchanged_and_static_metadata_fields_are_closed() {
    let mut video = video(
        input("deadpan-source/tests/fixtures", "limited709.mkv"),
        "sdr-source",
    );
    let capture = DecodedSourceQualification::from_sessions(Some(&video), None).unwrap();
    let json = String::from_utf8(capture.snapshot().to_json().unwrap()).unwrap();
    // Identical to the pre-HDR wire: no optional static metadata keys.
    assert!(json.contains(
        "\"color\":{\"range\":\"limited\",\"matrix\":\"bt709\",\"transfer\":\"bt709\",\"primaries\":\"bt709\"}"
    ));
    assert!(!json.contains("mastering") && !json.contains("content_light"));
    // Static metadata on an SDR interpretation is refused as stored evidence.
    let injected = json.replace(
        "\"primaries\":\"bt709\"}",
        "\"primaries\":\"bt709\",\"content_light\":{\"max_cll\":1,\"max_fall\":1}}",
    );
    assert_ne!(injected, json);
    assert!(SourceQualificationSnapshot::from_json(injected.as_bytes()).is_err());
    let picture = video
        .frame(
            deadpan_core::SourceFrameId(0),
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(picture.sample_bits, 8);

    let pq = video_snapshot_json("hevc-pq.mp4");
    for (from, to) in [
        ("\"max_cll\":1000", "\"max_cll\":1000,\"extra\":1"),
        ("\"min_luminance\":1", "\"min_luminance\":1,\"extra\":1"),
        ("\"transfer\":\"pq\"", "\"transfer\":\"st2084\""),
        ("\"max_fall\":400", "\"max_fall\":65536"),
    ] {
        assert!(pq.contains(from), "{from}");
        assert!(
            SourceQualificationSnapshot::from_json(pq.replace(from, to).as_bytes()).is_err(),
            "{to}"
        );
    }
}

fn video_snapshot_json(name: &str) -> String {
    let video = video(input("deadpan-source/tests/fixtures", name), "hdr-source");
    let capture = DecodedSourceQualification::from_sessions(Some(&video), None).unwrap();
    String::from_utf8(capture.snapshot().to_json().unwrap()).unwrap()
}

#[test]
fn ignored_static_metadata_is_a_recorded_note_and_never_coexists_with_values() {
    let session = video(
        input(
            "deadpan-source/tests/fixtures",
            "hevc-pq-invalid-static.mp4",
        ),
        "invalid-static",
    );
    let capture = DecodedSourceQualification::from_sessions(Some(&session), None).unwrap();
    let json = String::from_utf8(capture.snapshot().to_json().unwrap()).unwrap();
    assert!(json.contains(
        "\"primaries\":\"bt2020\",\"ignored_static\":{\"mastering\":true,\"content_light\":true}}"
    ));
    assert!(!json.contains("\"mastering\":{") && !json.contains("\"content_light\":{"));
    let snapshot = SourceQualificationSnapshot::from_json(json.as_bytes()).unwrap();
    let color = snapshot.video().unwrap().interpretation().color;
    assert_eq!((color.mastering, color.content_light), (None, None));
    assert!(color.ignored_static.mastering && color.ignored_static.content_light);
    assert_eq!(
        String::from_utf8(snapshot.to_json().unwrap()).unwrap(),
        json
    );

    let reject = |text: String| {
        assert_ne!(text, json);
        assert!(
            SourceQualificationSnapshot::from_json(text.as_bytes()).is_err(),
            "{text}"
        );
    };
    // A note cannot sit beside a value it says was ignored.
    reject(json.replace(
        "\"ignored_static\":{\"mastering\":true,\"content_light\":true}",
        "\"content_light\":{\"max_cll\":1000,\"max_fall\":400},\"ignored_static\":{\"mastering\":true,\"content_light\":true}",
    ));
    // Stored values must satisfy the shared rule set (MaxFALL above MaxCLL).
    reject(json.replace(
        "\"ignored_static\":{\"mastering\":true,\"content_light\":true}",
        "\"content_light\":{\"max_cll\":100,\"max_fall\":400}",
    ));
    // A mastering peak below 50 cd/m2 is invalid stored evidence.
    reject(json.replace(
        "\"ignored_static\":{\"mastering\":true,\"content_light\":true}",
        "\"mastering\":{\"primaries\":[[34000,16000],[13250,34500],[7500,3000]],\"white_point\":[15635,16450],\"max_luminance\":100000,\"min_luminance\":1}",
    ));
    reject(json.replace(
        "\"content_light\":true}",
        "\"content_light\":true,\"other\":true}",
    ));

    // An SDR interpretation carries no note.
    let sdr = video(
        input("deadpan-source/tests/fixtures", "limited709.mkv"),
        "sdr-note",
    );
    let sdr = String::from_utf8(
        DecodedSourceQualification::from_sessions(Some(&sdr), None)
            .unwrap()
            .snapshot()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let injected = sdr.replace(
        "\"primaries\":\"bt709\"}",
        "\"primaries\":\"bt709\",\"ignored_static\":{\"mastering\":true}}",
    );
    assert_ne!(injected, sdr);
    assert!(SourceQualificationSnapshot::from_json(injected.as_bytes()).is_err());
}
