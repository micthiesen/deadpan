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
