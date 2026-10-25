#![cfg(target_os = "macos")]
//! Real combined Apple Vision requests over independently generated non-face
//! video, through the qualified Original decoder and supervised worker.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::picture_proposals::{
    DetectorKind, PictureProposalDetection, PictureProposalRequest, ProposalError,
    ProposalSelection, SALIENCY_REQUEST_REVISION, detect_picture_proposals,
    prepare_picture_proposals,
};
use deadpan_cli::tracking::TrackingRuntime;
use deadpan_core::{AssetId, ExactRatio};
use deadpan_jobs::picture_proposals::{FACE_ENGINE, SALIENCY_ENGINE};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits, SourceAudioSample};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use deadpan_store::{AccessMode, ProjectStore};
use sha2::{Digest, Sha256};

const MAIN: &[u8] = include_bytes!("fixtures/salient-regions.mkv");
const CFR: &[u8] = include_bytes!("fixtures/salient-regions.mp4");
const SAR: &[u8] = include_bytes!("fixtures/salient-regions-sar.mkv");
const ROTATED: &[u8] = include_bytes!("fixtures/salient-mug-rotated-90.mp4");
const AUDIO: &[u8] = include_bytes!("fixtures/salient-audio.wav");
const APERTURE: &[u8] = include_bytes!("fixtures/salient-mug-aperture.mp4");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn runtime() -> TrackingRuntime {
    TrackingRuntime {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-track")),
        environment: Default::default(),
    }
}

fn cli(arguments: &[&str]) {
    assert_eq!(
        deadpan_cli::entry(arguments.iter().map(|s| s.to_string())),
        ExitCode::SUCCESS,
        "{arguments:?}"
    );
}

fn project(scratch: &Path, bytes: &[u8], audio: bool) -> PathBuf {
    let package = scratch.join("proposals.deadpan");
    let path = package.to_str().unwrap();
    cli(&[
        "project",
        "create",
        path,
        "--fps",
        "30000/1001",
        "--size",
        "640x360",
    ]);
    let source = scratch.join("source.media");
    std::fs::write(&source, bytes).unwrap();
    cli(&["project", "retain-original", path, source.to_str().unwrap()]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let records = store.original_records(None, 8).unwrap();
    let original = serde_json::to_value(records[0].object().content()).unwrap();
    let snapshot = store.snapshot().unwrap();
    drop(store);
    let streams = if audio {
        serde_json::json!({"type":"video_and_audio","audio_stream":1})
    } else {
        serde_json::json!({"type":"video_only"})
    };
    let registration = serde_json::json!({
        "protocol":1,
        "registration":{
            "expected_revision":snapshot.revision_id(),"new_revision":"registered",
            "original":original,"new_asset_id":"clip","label":"Clip",
            "insertion":{"parent":snapshot.root(),"index":0,"node":"clip-source","label":"Clip"}
        },
        "streams": streams
    });
    let request = scratch.join("register.json");
    std::fs::write(&request, serde_json::to_vec(&registration).unwrap()).unwrap();
    cli(&[
        "project",
        "register-source",
        path,
        "--request-json",
        request.to_str().unwrap(),
    ]);
    package
}

fn request(at_pts: i64, selection: ProposalSelection) -> PictureProposalRequest {
    PictureProposalRequest {
        asset: Some(AssetId::new("clip").unwrap()),
        selection,
        at_pts,
    }
}

fn detect(
    store: &ProjectStore,
    at_pts: i64,
    selection: ProposalSelection,
) -> PictureProposalDetection {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let prepared =
        prepare_picture_proposals(store, &request(at_pts, selection), &cancelled, deadline)
            .unwrap();
    let result =
        detect_picture_proposals(&runtime(), &prepared, "real-proposal", &cancelled, deadline)
            .unwrap();
    assert_eq!(result.selection, selection);
    assert_eq!(result.pts, prepared.pts);
    result.proposals.validate(selection).unwrap();
    if let Some(faces) = &result.proposals.faces {
        assert_eq!(faces.runtime.detector, DetectorKind::Faces);
        assert!(faces.runtime.request_revision > 0);
        assert_eq!(faces.runtime.engine, FACE_ENGINE);
    }
    if let Some(regions) = &result.proposals.salient_regions {
        assert_eq!(regions.runtime.detector, DetectorKind::SalientRegions);
        assert_eq!(regions.runtime.request_revision, SALIENCY_REQUEST_REVISION);
        assert_eq!(regions.runtime.engine, SALIENCY_ENGINE);
    }
    eprintln!(
        "actual-proposals: {}",
        serde_json::to_string(&result.proposals).unwrap()
    );
    result
}

fn assert_region_center(result: &PictureProposalDetection, expected: (f64, f64)) {
    assert!(result.proposals.faces.as_ref().unwrap().faces.is_empty());
    let regions = &result.proposals.salient_regions.as_ref().unwrap().regions;
    assert!(!regions.is_empty());
    assert!(regions.len() <= 64);
    assert!(
        regions.iter().any(|r| {
            let c = r.region.center();
            (c.0 - expected.0).abs() < 0.12
                && (c.1 - expected.1).abs() < 0.12
                && r.region.width() < 0.65
                && r.region.height() < 0.85
        }),
        "{regions:?}"
    );
}

#[test]
fn real_non_face_modes_blank_mug_and_two_objects_are_typed_and_atomic() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), MAIN, false);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let before = store.snapshot().unwrap();
    let blank = detect(&store, 0, ProposalSelection::Both);
    assert!(blank.proposals.faces.unwrap().faces.is_empty());
    assert!(blank.proposals.salient_regions.unwrap().regions.is_empty());
    let mug = detect(&store, 1335, ProposalSelection::Both);
    assert_region_center(&mug, (0.69, 0.54));
    let two = detect(&store, 2669, ProposalSelection::Both);
    assert!(two.proposals.faces.as_ref().unwrap().faces.is_empty());
    assert!(
        two.proposals
            .salient_regions
            .as_ref()
            .unwrap()
            .regions
            .len()
            >= 2
    );
    let salient = detect(&store, 1335, ProposalSelection::SalientRegions);
    assert!(salient.proposals.faces.is_none());
    assert_eq!(
        salient.proposals.salient_regions,
        mug.proposals.salient_regions
    );
    let faces = detect(&store, 1335, ProposalSelection::Faces);
    assert!(faces.proposals.salient_regions.is_none());
    assert_eq!(faces.proposals.faces, mug.proposals.faces);
    assert_eq!(store.snapshot().unwrap(), before);

    let running = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let previous = prepare_picture_proposals(
        &store,
        &request(1334, ProposalSelection::Both),
        &running,
        deadline,
    )
    .unwrap();
    assert_eq!((previous.ordinal, previous.pts), (39, 1301));
    let next = prepare_picture_proposals(
        &store,
        &request(1335, ProposalSelection::Both),
        &running,
        deadline,
    )
    .unwrap();
    assert_eq!((next.ordinal, next.pts, next.video_end), (40, 1335, 4004));
    assert!(matches!(
        detect_picture_proposals(
            &runtime(),
            &next,
            "cancel",
            &AtomicBool::new(true),
            deadline
        ),
        Err(ProposalError::Cancelled)
    ));
    assert!(matches!(
        detect_picture_proposals(&runtime(), &next, "deadline", &running, Instant::now()),
        Err(ProposalError::Deadline)
    ));
}

#[test]
fn exact_cfr_fixture_admits_default_original_and_real_proposals() {
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("salient-regions.mp4");
    std::fs::write(&source, CFR).unwrap();
    let package = scratch.path().join("default-original.deadpan");
    cli(&[
        "project",
        "create-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        before.presentation_basis().frame_rate,
        deadpan_core::FrameRate::new(30000, 1001).unwrap()
    );
    let running = AtomicBool::new(false);
    for (ordinal, count) in [(0, 0), (40, 1), (80, 2)] {
        let pts = ordinal as i64 * 1001;
        let deadline = Instant::now() + Duration::from_secs(120);
        let prepared = prepare_picture_proposals(
            &store,
            &PictureProposalRequest {
                asset: None,
                selection: ProposalSelection::Both,
                at_pts: pts,
            },
            &running,
            deadline,
        )
        .unwrap();
        assert_eq!(
            (prepared.ordinal, prepared.pts, prepared.video_end),
            (ordinal, pts, 120120)
        );
        assert_eq!(
            (
                prepared.stream().time_base_num,
                prepared.stream().time_base_den
            ),
            (1, 30000)
        );
        let result =
            detect_picture_proposals(&runtime(), &prepared, "exact-cfr", &running, deadline)
                .unwrap();
        result.proposals.validate(ProposalSelection::Both).unwrap();
        assert_eq!(result.pts, pts);
        let faces = result.proposals.faces.as_ref().unwrap();
        assert!(faces.faces.is_empty());
        assert_eq!(faces.runtime.engine, FACE_ENGINE);
        let salient = result.proposals.salient_regions.as_ref().unwrap();
        assert_eq!(salient.runtime.engine, SALIENCY_ENGINE);
        assert_eq!(salient.runtime.request_revision, SALIENCY_REQUEST_REVISION);
        assert_eq!(salient.regions.len(), count);
        if ordinal == 40 {
            assert_region_center(&result, (0.69, 0.54));
        }
        eprintln!(
            "actual-cfr-proposals pts={pts} ordinal={ordinal}: {}",
            serde_json::to_string(&result.proposals).unwrap()
        );
    }
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn combined_request_preserves_positive_default_face_results() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(
        scratch.path(),
        include_bytes!("fixtures/two-drawn-faces.mkv"),
        false,
    );
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let combined = detect(&store, 0, ProposalSelection::Both);
    let faces = detect(&store, 0, ProposalSelection::Faces);
    assert_eq!(combined.proposals.faces.as_ref().unwrap().faces.len(), 2);
    assert_eq!(combined.proposals.faces, faces.proposals.faces);
    assert!(combined.proposals.salient_regions.is_some());
}

#[test]
fn real_saliency_applies_sar_rotation_and_fractional_clean_aperture() {
    for (name, bytes, pts, aspect, turns, center) in [
        ("sar", SAR, 1335, (64, 27), 0, (0.69, 0.54)),
        ("rotation", ROTATED, 0, (9, 16), 1, (0.46, 0.69)),
        ("aperture", APERTURE, 0, (959, 639), 0, (0.586, 0.55)),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let package = project(scratch.path(), bytes, false);
        let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
        let running = AtomicBool::new(false);
        let prepared = prepare_picture_proposals(
            &store,
            &request(pts, ProposalSelection::Both),
            &running,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
        assert_eq!(
            prepared.display_aspect,
            Some(ExactRatio::new(aspect.0, aspect.1).unwrap()),
            "{name}"
        );
        assert_eq!(prepared.stream().rotation_quarter_turns, turns, "{name}");
        if name == "aperture" {
            assert_eq!(
                prepared.stream().clean_aperture,
                Some([
                    ExactRatio::new(641, 4).unwrap(),
                    ExactRatio::new(81, 4).unwrap(),
                    ExactRatio::new(959, 2).unwrap(),
                    ExactRatio::new(639, 2).unwrap()
                ])
            );
        }
        assert_region_center(&detect(&store, pts, ProposalSelection::Both), center);
    }
}

#[test]
fn qualified_decoder_preserves_generated_picture_and_pcm_clocks() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/salient-regions-fixtures.json")).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let running = AtomicBool::new(false);
    for (name, bytes) in [
        ("salient-regions.mkv", MAIN),
        ("salient-regions.mp4", CFR),
        ("salient-regions-sar.mkv", SAR),
        ("salient-mug-rotated-90.mp4", ROTATED),
        ("salient-mug-aperture.mp4", APERTURE),
    ] {
        let file = scratch.path().join(name);
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(
            hex(&Sha256::digest(bytes)),
            manifest["fixtures"][name]["sha256"].as_str().unwrap()
        );
        let control = || DecodeControl {
            timeout: Duration::from_secs(30),
            cancelled: &running,
        };
        let mut decoder = SourceDecoder::open(
            std::fs::File::open(file).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        let expected = manifest["fixtures"][name]["frames"].as_array().unwrap();
        for frame in expected {
            let actual = decoder.next_metadata(control()).unwrap().unwrap();
            assert_eq!(actual.pts, frame["pts"].as_i64().unwrap(), "{name}");
            assert_eq!(
                actual.reported_duration,
                frame["duration"].as_i64(),
                "{name}"
            );
            let rgba = decoder.copy_current_rgba(control()).unwrap();
            assert_eq!((rgba.width, rgba.height, rgba.sample_bits), (640, 360, 8));
        }
        assert!(decoder.next_metadata(control()).unwrap().is_none());
    }
    let identity =
        SourceContentIdentity::new(Sha256::digest(AUDIO).into(), AUDIO.len() as u64).unwrap();
    let mut input = AUDIO;
    let session = AudioSession::open_verified(
        &mut input,
        identity,
        0,
        AudioSessionLimits::default(),
        &running,
    )
    .unwrap();
    assert_eq!(session.index().decoded_samples(), 192192);
    assert_eq!(session.index().valid_samples(), 192192);
    let mut hash = Sha256::new();
    let mut start = 0_i64;
    while start < 192192 {
        let count = u32::try_from((192192 - start).min(32768)).unwrap();
        let block = session
            .read_samples(
                SourceAudioSample(start),
                count,
                Duration::from_secs(30),
                &running,
            )
            .unwrap();
        assert_eq!(block.sample_rate, 48000);
        assert_eq!(block.channel_layout.channels(), 2);
        assert_eq!(block.samples.len(), count as usize * 2);
        for value in block.samples {
            let exact = value * 32768.0;
            assert_eq!(exact, exact.round());
            hash.update((exact as i16).to_le_bytes());
        }
        start += i64::from(count);
    }
    assert_eq!(
        hex(&hash.finalize()),
        manifest["raw_pcm_s16le_sha256"].as_str().unwrap()
    );
}
