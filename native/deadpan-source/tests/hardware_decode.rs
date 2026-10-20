//! VideoToolbox must preserve the software decoder's canonical pictures and
//! source observations. These are real macOS decoder tests, not mocked frames.
#![cfg(target_os = "macos")]

#[path = "../../../crates/deadpan-cli/tests/support/physical_mac.rs"]
mod physical_mac;

use deadpan_source::{
    DecodeAcceleration, DecodeControl, DecodeLimits, SourceDecodeError, SourceDecoder,
};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(20),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str) -> File {
    File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn open(name: &str, hardware: bool, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        fixture(name),
        DecodeLimits {
            acceleration: if hardware {
                DecodeAcceleration::PreferVideoToolbox
            } else {
                DecodeAcceleration::Software
            },
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}, hardware={hardware}, threads={threads}: {error}"))
}

#[derive(Debug, PartialEq, Eq)]
struct Picture {
    pts: i64,
    duration: Option<i64>,
    keyframe: bool,
    rgba8: blake3::Hash,
    rgba16: blake3::Hash,
}
fn current(decoder: &mut SourceDecoder) -> Picture {
    let eight = decoder.copy_current_rgba(control()).unwrap();
    let sixteen = decoder.copy_current_rgba16(control()).unwrap();
    assert_eq!(eight.metadata, sixteen.metadata);
    Picture {
        pts: eight.metadata.pts,
        duration: eight.metadata.reported_duration,
        keyframe: eight.metadata.keyframe,
        rgba8: blake3::hash(&eight.rgba),
        rgba16: blake3::hash(&sixteen.rgba),
    }
}
fn reference(name: &str) -> (deadpan_source::SourceStreamInfo, Vec<Picture>) {
    let mut decoder = open(name, false, 1);
    let mut pictures = Vec::new();
    while decoder.next_metadata(control()).unwrap().is_some() {
        pictures.push(current(&mut decoder));
    }
    assert_eq!(decoder.work().hardware_frames, 0);
    (decoder.info().clone(), pictures)
}

#[test]
fn videotoolbox_preserves_pixels_clocks_crop_and_color_at_both_thread_limits() {
    if physical_mac::skip_without_physical_mac("VideoToolbox picture exactness") {
        return;
    }
    for name in [
        "cfr-bframes.mp4",
        "offset-bframes.mp4",
        "pyramid-bframes.mp4",
        "vfr-long-terminal.mp4",
        "aperture.mp4",
        "aperture-fractional.mp4",
        "h264-sdr-10-limited.mp4",
        "h264-sdr-10-full.mp4",
        "hevc-sdr-8-limited.mp4",
        "hevc-sdr-8-full.mp4",
        "hevc-sdr-10-limited.mp4",
        "hevc-sdr-10-full.mp4",
        "hevc-pq-open-gop.mp4",
        "hdr-pq-av.mp4",
        "hdr-hlg-av.mp4",
        "aperture-fractional-hdr.mp4",
        "vp9-sdr-8-limited.mp4",
        "vp9-sdr-8-full.mp4",
        "vp9-sdr-10-limited.mp4",
        "vp9-sdr-10-full.mp4",
        "vp9-altref.mp4",
        "vp9-existing-8.mp4",
        "vp9-existing-10.mp4",
        "vp9-sdr-8-limited.webm",
        "vp9-vfr.webm",
        "vp9-pq-static.mp4",
        "vp9-pq-static.webm",
        "vp9-pq-topleft.webm",
        "vp9-hlg-anamorphic.webm",
    ] {
        let (info, expected) = reference(name);
        for threads in [1, 16] {
            let mut decoder = open(name, true, threads);
            assert_eq!(*decoder.info(), info, "{name}: source metadata");
            assert!(
                decoder.work().hardware_frames > 0,
                "{name}: VideoToolbox was not exercised"
            );
            assert_eq!(
                decoder.work().hardware_transfers,
                0,
                "opening must not download pixels"
            );
            let mut actual = Vec::new();
            while decoder.next_metadata(control()).unwrap().is_some() {
                let transfers = decoder.work().hardware_transfers;
                actual.push(current(&mut decoder));
                assert_eq!(decoder.work().hardware_transfers, transfers + 1);
                assert_eq!(current(&mut decoder), *actual.last().unwrap());
                assert_eq!(decoder.work().hardware_transfers, transfers + 1);
            }
            assert_eq!(actual, expected, "{name} at {threads} threads");
            assert_eq!(decoder.work().hardware_frames, expected.len() as u64);
            eprintln!(
                "{name} threads={threads}: {} exact pictures; {:?}",
                actual.len(),
                decoder.work()
            );
        }
    }
}

#[test]
fn hardware_metadata_preroll_seeks_and_following_steps_preserve_exact_pictures() {
    if physical_mac::skip_without_physical_mac("VideoToolbox picture exactness") {
        return;
    }
    for name in [
        "cfr-bframes.mp4",
        "pyramid-bframes.mp4",
        "vfr-long-terminal.mp4",
        "hevc-pq-open-gop.mp4",
        "vp9-altref.mp4",
        "vp9-vfr.webm",
    ] {
        let (_, expected) = reference(name);
        for threads in [1, 16] {
            let mut decoder = open(name, true, threads);
            for target in (0..expected.len()).rev() {
                let anchor = expected[..=target]
                    .iter()
                    .rposition(|p| p.keyframe)
                    .unwrap();
                let transfers = decoder.work().hardware_transfers;
                decoder
                    .seek_to(expected[anchor].pts, expected[target].pts, control())
                    .unwrap();
                loop {
                    let frame = decoder.next_metadata(control()).unwrap().unwrap();
                    if frame.pts >= expected[target].pts {
                        assert_eq!(frame.pts, expected[target].pts);
                        break;
                    }
                }
                assert_eq!(
                    decoder.work().hardware_transfers,
                    transfers,
                    "{name}: preroll downloaded pixels"
                );
                assert_eq!(
                    current(&mut decoder),
                    expected[target],
                    "{name} target {target}"
                );
                for next in expected.iter().skip(target + 1).take(2) {
                    decoder.next_metadata(control()).unwrap().unwrap();
                    assert_eq!(current(&mut decoder), *next, "{name} step after {target}");
                }
            }
            eprintln!(
                "{name} threads={threads}: {} reverse seeks passed; {:?}",
                expected.len(),
                decoder.work()
            );
        }
    }
}

#[test]
fn unsupported_hardware_layouts_and_field_cadences_use_qualified_software() {
    for name in [
        "rotated90.mp4",
        "full709.mkv",
        "fields-tff.mp4",
        "fields-bff.mp4",
        "fields-tff-bframes.mp4",
        "telecine-tff.mp4",
        "telecine-variable.mp4",
        "progressive-timing-hrd.mp4",
        "av1-grain.mp4",
        "prores-hq.mov",
    ] {
        let (info, expected) = reference(name);
        let mut decoder = open(name, true, 16);
        assert_eq!(*decoder.info(), info);
        let mut actual = Vec::new();
        while decoder.next_metadata(control()).unwrap().is_some() {
            actual.push(current(&mut decoder));
        }
        assert_eq!(actual, expected, "{name}");
        assert_eq!(decoder.work().hardware_frames, 0, "{name}");
        assert_eq!(decoder.work().hardware_transfers, 0);
        eprintln!("{name}: {} exact software fallback pictures", actual.len());
    }
}

#[test]
fn hardware_obeys_geometry_frame_and_cancellation_limits() {
    let error = SourceDecoder::open(
        fixture("cfr-bframes.mp4"),
        DecodeLimits {
            acceleration: DecodeAcceleration::PreferVideoToolbox,
            max_dimension: 319,
            ..DecodeLimits::default()
        },
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(error, SourceDecodeError::Native { code, .. } if code == "resource_limit"));
    let mut decoder = SourceDecoder::open(
        fixture("cfr-bframes.mp4"),
        DecodeLimits {
            acceleration: DecodeAcceleration::PreferVideoToolbox,
            max_frames: 2,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    assert!(decoder.work().hardware_frames > 0);
    decoder.next_metadata(control()).unwrap().unwrap();
    let first = current(&mut decoder);
    let work = decoder.work();
    let cancelled = AtomicBool::new(true);
    let stop = DecodeControl {
        cancelled: &cancelled,
        ..control()
    };
    for error in [
        decoder.next_metadata(stop).err().unwrap(),
        decoder.copy_current_rgba16(stop).err().unwrap(),
    ] {
        assert!(matches!(error, SourceDecodeError::Native { code, .. } if code == "cancelled"));
    }
    assert_eq!(decoder.work(), work);
    assert_eq!(current(&mut decoder), first);
    decoder.next_metadata(control()).unwrap().unwrap();
    let error = decoder.next_metadata(control()).err().unwrap();
    assert!(matches!(error, SourceDecodeError::Native { code, .. } if code == "resource_limit"));
    assert!(
        decoder.next_metadata(control()).is_err(),
        "failed decode must remain poisoned"
    );
}

#[test]
fn hevc_without_explicit_frame_only_constraints_stays_in_software() {
    use std::io::{Read, Seek, Write};
    let mut bytes = Vec::new();
    fixture("hevc-sdr-8-limited.mp4")
        .read_to_end(&mut bytes)
        .unwrap();
    let at = bytes.windows(4).position(|v| v == b"hvcC").unwrap() + 4;
    // Weaken the hvcC constraints without changing any encoded picture.
    // Unknown coding must not be guessed progressive for hardware selection.
    bytes[at + 6] &= !0x90;
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    file.rewind().unwrap();
    let (_, expected) = reference("hevc-sdr-8-limited.mp4");
    let mut decoder = SourceDecoder::open(
        file,
        DecodeLimits {
            acceleration: DecodeAcceleration::PreferVideoToolbox,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    let mut actual = Vec::new();
    while decoder.next_metadata(control()).unwrap().is_some() {
        actual.push(current(&mut decoder));
    }
    assert_eq!(actual, expected);
    assert_eq!(decoder.work().hardware_frames, 0);
}
