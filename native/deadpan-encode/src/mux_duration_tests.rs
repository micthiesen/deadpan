use std::sync::atomic::AtomicBool;

use super::*;
use crate::{BFramePolicy, EncoderMode, HdrTransfer};

const RETAINED: &[u8] =
    include_bytes!("../../../tools/media-qualification/evidence/2026-10-08-mux-duration/movie.mp4");

fn contract() -> EncodeContract {
    crate::probe::HdrEncoderProbe::new([640, 360], [30, 1], HdrTransfer::Pq)
        .unwrap()
        .contract(EncoderMode::Hardware, BFramePolicy::TargetTwo)
        .unwrap()
}

fn file(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file
}

fn bytes(file: &mut File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

fn control(cancelled: &AtomicBool) -> Control<'_> {
    Control {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

#[test]
fn retained_packet_order_corrects_only_mdhd_duration_and_is_idempotent() {
    let cancelled = AtomicBool::new(false);
    let mut output = file(RETAINED);
    let contract = contract();
    assert_eq!(contract.video_frames(), 46);
    let correction = finalize(&mut output, &contract, &control(&cancelled))
        .unwrap()
        .unwrap();
    assert_eq!(
        correction,
        VideoMediaDurationCorrection {
            previous_ticks: 47,
            corrected_ticks: 46,
            first_cts: 2,
            minimum_cts: 1,
        }
    );
    correction.validate(&contract).unwrap();
    let corrected = bytes(&mut output);
    assert_eq!(corrected.len(), RETAINED.len());
    let differences: Vec<_> = RETAINED
        .iter()
        .zip(&corrected)
        .enumerate()
        .filter_map(|(index, (a, b))| (a != b).then_some(index))
        .collect();
    assert_eq!(differences, [315]);
    assert_eq!(&corrected[312..316], &46_u32.to_be_bytes());
    assert_eq!(
        finalize(&mut output, &contract, &control(&cancelled)).unwrap(),
        None
    );
    assert_eq!(bytes(&mut output), corrected);
    let round_trip: VideoMediaDurationCorrection =
        serde_json::from_slice(&serde_json::to_vec(&correction).unwrap()).unwrap();
    assert_eq!(round_trip, correction);
}

fn atom(kind: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
    let mut result = u32::try_from(data.len() + 8)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    result.extend_from_slice(kind);
    result.extend(data);
    result
}

/// Minimal fixed headers exercise the no-correction path independently of
/// decoder metadata or hardware. Corrections always require the full parser.
fn headers(wide: bool) -> Vec<u8> {
    fn track(id: u32, kind: &[u8; 4], ticks: u64, scale: u32, wide: bool) -> Vec<u8> {
        let mut tkhd = vec![0; if wide { 96 } else { 84 }];
        tkhd[0] = u8::from(wide);
        tkhd[3] = 3;
        let offset = if wide { 20 } else { 12 };
        tkhd[offset..offset + 4].copy_from_slice(&id.to_be_bytes());
        let mut mdhd = vec![0; if wide { 36 } else { 24 }];
        mdhd[0] = u8::from(wide);
        let offset = if wide { 20 } else { 12 };
        mdhd[offset..offset + 4].copy_from_slice(&scale.to_be_bytes());
        if wide {
            mdhd[offset + 4..offset + 12].copy_from_slice(&ticks.to_be_bytes());
        } else {
            mdhd[offset + 4..offset + 8]
                .copy_from_slice(&u32::try_from(ticks).unwrap().to_be_bytes());
        }
        let mut hdlr = vec![0; 24];
        hdlr[8..12].copy_from_slice(kind);
        atom(
            b"trak",
            [
                atom(b"tkhd", tkhd),
                atom(b"mdia", [atom(b"mdhd", mdhd), atom(b"hdlr", hdlr)].concat()),
            ]
            .concat(),
        )
    }
    atom(
        b"moov",
        [
            track(1, b"vide", 46, 30, wide),
            track(2, b"soun", 74624, 48000, wide),
        ]
        .concat(),
    )
}

#[test]
fn exact_version_zero_and_one_headers_are_unchanged() {
    let cancelled = AtomicBool::new(false);
    for wide in [false, true] {
        let original = headers(wide);
        let mut output = file(&original);
        assert_eq!(
            finalize(&mut output, &contract(), &control(&cancelled)).unwrap(),
            None
        );
        assert_eq!(bytes(&mut output), original);
    }
}

#[test]
fn version_one_duration_correction_keeps_every_other_byte() {
    // Widen only the retained video's mdhd and repair its containing extents
    // and absolute chunk offsets. Media payload and all timing runs stay exact.
    let mut original = RETAINED.to_vec();
    let mut body = vec![0; 36];
    body[0] = 1;
    body[20..24].copy_from_slice(&30_u32.to_be_bytes());
    body[24..32].copy_from_slice(&47_u64.to_be_bytes());
    body[32..36].copy_from_slice(&original[316..320]);
    original.splice(288..320, atom(b"mdhd", body));
    for offset in [28, 144, 280] {
        let old = u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap());
        original[offset..offset + 4].copy_from_slice(&(old + 12).to_be_bytes());
    }
    for start in [1431 + 12, 2850 + 12] {
        assert_eq!(&original[start + 4..start + 8], b"stco");
        let count = u32::from_be_bytes(original[start + 12..start + 16].try_into().unwrap());
        for row in 0..count as usize {
            let offset = start + 16 + row * 4;
            let old = u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap());
            original[offset..offset + 4].copy_from_slice(&(old + 12).to_be_bytes());
        }
    }
    let cancelled = AtomicBool::new(false);
    let mut output = file(&original);
    let correction = finalize(&mut output, &contract(), &control(&cancelled))
        .unwrap()
        .unwrap();
    assert_eq!(correction.corrected_ticks, 46);
    let corrected = bytes(&mut output);
    let differences: Vec<_> = original
        .iter()
        .zip(&corrected)
        .enumerate()
        .filter_map(|(index, (a, b))| (a != b).then_some(index))
        .collect();
    assert_eq!(differences, [327]);
    assert_eq!(&corrected[320..328], &46_u64.to_be_bytes());
    assert_eq!(
        finalize(&mut output, &contract(), &control(&cancelled)).unwrap(),
        None
    );
}

#[test]
fn malformed_and_unproven_duration_changes_are_rejected_before_writing() {
    let cancelled = AtomicBool::new(false);
    let mut cases = Vec::new();
    let mut wrong_duration = RETAINED.to_vec();
    wrong_duration[312..316].copy_from_slice(&48_u32.to_be_bytes());
    cases.push(wrong_duration);
    let mut wrong_scale = RETAINED.to_vec();
    wrong_scale[308..312].copy_from_slice(&31_u32.to_be_bytes());
    cases.push(wrong_scale);
    let mut wrong_version = RETAINED.to_vec();
    wrong_version[296] = 2;
    cases.push(wrong_version);
    let mut wrong_ctts_version = RETAINED.to_vec();
    wrong_ctts_version[811] = 2;
    cases.push(wrong_ctts_version);
    let mut duplicate_track = RETAINED.to_vec();
    duplicate_track[172..176].copy_from_slice(&2_u32.to_be_bytes());
    cases.push(duplicate_track);
    let mut missing_sample = RETAINED.to_vec();
    missing_sample[759..763].copy_from_slice(&45_u32.to_be_bytes());
    cases.push(missing_sample);
    let mut duplicate_pts = RETAINED.to_vec();
    duplicate_pts[831..835].copy_from_slice(&2_u32.to_be_bytes());
    cases.push(duplicate_pts);
    let mut incomplete_pts = RETAINED.to_vec();
    incomplete_pts[823..827].copy_from_slice(&3_u32.to_be_bytes());
    cases.push(incomplete_pts);
    let mut wrong_format = RETAINED.to_vec();
    let hvc1 = wrong_format
        .windows(4)
        .position(|bytes| bytes == b"hvc1")
        .unwrap();
    wrong_format[hvc1..hvc1 + 4].copy_from_slice(b"avc1");
    cases.push(wrong_format);
    let mut out_of_bounds = RETAINED.to_vec();
    out_of_bounds[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    cases.push(out_of_bounds);
    cases.push(RETAINED[..300].to_vec());
    for (index, original) in cases.into_iter().enumerate() {
        let mut output = file(&original);
        assert!(
            finalize(&mut output, &contract(), &control(&cancelled)).is_err(),
            "accepted mutation {index}"
        );
        assert_eq!(
            bytes(&mut output),
            original,
            "changed rejected mutation {index}"
        );
    }
}

#[test]
fn duplicate_headers_and_box_work_bound_fail_without_writes() {
    let cancelled = AtomicBool::new(false);
    let empty = atom(b"free", vec![]);
    let mut too_many = empty.repeat(1025);
    too_many.extend(headers(false));
    let normal = headers(false);
    let duplicate_movie = [normal.clone(), normal].concat();
    for original in [too_many, duplicate_movie] {
        let mut output = file(&original);
        assert!(finalize(&mut output, &contract(), &control(&cancelled)).is_err());
        assert_eq!(bytes(&mut output), original);
    }
}

#[test]
fn correction_arithmetic_rejects_invented_values_and_no_b_frame_policy() {
    let valid = VideoMediaDurationCorrection {
        previous_ticks: 47,
        corrected_ticks: 46,
        first_cts: 2,
        minimum_cts: 1,
    };
    let mut variants = Vec::new();
    let mut wrong = valid;
    wrong.previous_ticks = 48;
    variants.push(wrong);
    let mut wrong = valid;
    wrong.corrected_ticks = 45;
    variants.push(wrong);
    let mut wrong = valid;
    wrong.minimum_cts = 2;
    variants.push(wrong);
    let mut wrong = valid;
    wrong.first_cts = 65;
    wrong.previous_ticks = 110;
    variants.push(wrong);
    let mut wrong = valid;
    wrong.first_cts = 3;
    wrong.minimum_cts = 2;
    variants.push(wrong);
    for wrong in variants {
        assert!(wrong.validate(&contract()).is_err());
    }
    let no_b = crate::probe::HdrEncoderProbe::new([640, 360], [30, 1], HdrTransfer::Pq)
        .unwrap()
        .contract(EncoderMode::Hardware, BFramePolicy::None)
        .unwrap();
    assert!(valid.validate(&no_b).is_err());
    let cancelled = AtomicBool::new(false);
    let mut output = file(RETAINED);
    assert!(finalize(&mut output, &no_b, &control(&cancelled)).is_err());
    assert_eq!(bytes(&mut output), RETAINED);
    let mut json = serde_json::to_value(valid).unwrap();
    json["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<VideoMediaDurationCorrection>(json).is_err());
    let fractional = EncodeContract::new_v1(
        [640, 360],
        [30000, 1001],
        46,
        73674,
        EncoderMode::Hardware,
        BFramePolicy::TargetTwo,
    )
    .unwrap();
    let integral = VideoMediaDurationCorrection {
        previous_ticks: 47 * 1001,
        corrected_ticks: 46 * 1001,
        first_cts: 2 * 1001,
        minimum_cts: 1001,
    };
    integral.validate(&fractional).unwrap();
    let nonintegral = VideoMediaDurationCorrection {
        first_cts: integral.first_cts - 1,
        minimum_cts: integral.minimum_cts - 1,
        ..integral
    };
    assert!(nonintegral.validate(&fractional).is_err());
}

#[test]
fn cancelled_expired_and_nonregular_inputs_are_rejected() {
    let cancelled = AtomicBool::new(true);
    let mut output = file(RETAINED);
    assert!(matches!(
        finalize(&mut output, &contract(), &control(&cancelled)),
        Err(EncodeError::Cancelled)
    ));
    cancelled.store(false, std::sync::atomic::Ordering::Release);
    let expired = Control {
        cancelled: &cancelled,
        deadline: Instant::now(),
    };
    assert!(matches!(
        finalize(&mut output, &contract(), &expired),
        Err(EncodeError::Deadline)
    ));
    assert_eq!(bytes(&mut output), RETAINED);
    let directory = tempfile::tempdir().unwrap();
    let mut directory_file = File::open(directory.path()).unwrap();
    assert!(finalize(&mut directory_file, &contract(), &control(&cancelled)).is_err());
}
