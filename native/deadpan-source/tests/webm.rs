//! WebM keeps its actual millisecond clock and the copied VP9 picture content.
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name)).unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn webm_and_matroska_keep_pixels_measured_clocks_and_threaded_reverse_seeks() {
    for (name, original, count) in [
        ("vp9-sdr-8-limited.webm", "vp9-sdr-8-limited", 12),
        ("vp9-sdr-8-full.webm", "vp9-sdr-8-full", 12),
        ("vp9-sdr-10-limited.webm", "vp9-sdr-10-limited", 12),
        ("vp9-sdr-10-full.webm", "vp9-sdr-10-full", 12),
        ("vp9-sdr-8-limited.mkv", "vp9-sdr-8-limited", 12),
        ("vp9-anamorphic.webm", "vp9-sdr-8-limited", 12),
        ("vp9-altref.webm", "vp9-altref", 60),
        ("vp9-existing-8.webm", "vp9-existing-8", 2),
        ("vp9-existing-10.webm", "vp9-existing-10", 2),
        ("vp9-vfr.webm", "vp9-sdr-10-limited", 12),
    ] {
        let mut reference = open(&format!("{original}.mp4"), 1);
        let mut webm = open(name, 1);
        assert_eq!(
            (webm.info().time_base_num, webm.info().time_base_den),
            (1, 1000)
        );
        assert!(webm.info().audio_streams.is_empty());
        assert_eq!(webm.info().color, reference.info().color);
        assert_eq!(
            (webm.info().sample_aspect_num, webm.info().sample_aspect_den),
            if name == "vp9-anamorphic.webm" {
                (4, 3)
            } else {
                (1, 1)
            },
            "{name}",
        );
        let mut frames = Vec::new();
        for ordinal in 0..count {
            let expected = reference.next_rgba16(control()).unwrap().unwrap();
            let actual = webm.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(actual.rgba, expected.rgba, "{name} {ordinal}");
            assert_eq!(actual.row_stride_bytes, expected.row_stride_bytes);
            let (pts, duration) = if name == "vp9-vfr.webm" {
                (
                    ordinal * 50 + ordinal % 2 * 10,
                    if ordinal % 2 == 0 { 60 } else { 40 },
                )
            } else {
                ((ordinal * 1001 + 15) / 30, 33)
            };
            assert_eq!(actual.metadata.pts, pts, "{name} {ordinal}");
            assert_eq!(
                actual.metadata.reported_duration,
                Some(duration),
                "{name} {ordinal}"
            );
            frames.push(actual);
        }
        assert!(webm.next_rgba16(control()).unwrap().is_none());
        assert!(reference.next_rgba16(control()).unwrap().is_none());
        for threads in [1, 8, 16] {
            let mut decoder = open(name, threads);
            for expected in &frames {
                assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
            }
            for expected in frames.iter().rev() {
                decoder.seek(expected.metadata.pts, control()).unwrap();
                loop {
                    let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                    if actual.metadata.pts == expected.metadata.pts {
                        assert_eq!(&actual, expected, "{name} {threads}");
                        break;
                    }
                    assert!(
                        actual.metadata.pts < expected.metadata.pts,
                        "{name} {threads}"
                    );
                }
            }
        }
    }
}

#[test]
fn contradictory_or_missing_webm_interpretation_fails_before_decoding() {
    use std::io::Write;
    let original = std::fs::read(fixture("vp9-sdr-8-limited.webm")).unwrap();
    for (anchor, replacement, expected) in [
        (
            &b"V_VP9"[..],
            &b"V_AV1"[..],
            "AV1 requires av1C CodecPrivate",
        ),
        (&b"V_VP9"[..], &b"V_XXX"[..], "unsupported_codec"),
        (
            &[0xb0, 0x81, 96][..],
            &[0xb0, 0x81, 97][..],
            "raster disagree",
        ),
        (&[0x9a, 0x81, 2][..], &[0x9a, 0x81, 1][..], "field, stereo"),
        (
            &[0x55, 0xba, 0x81, 1][..],
            &[0x55, 0xba, 0x81, 16][..],
            "qualified SDR color",
        ),
        (
            &[0x55, 0xb1, 0x81, 1][..],
            &[0x55, 0xb1, 0x81, 5][..],
            "color interpretation disagree",
        ),
        (
            &[0x55, 0xb9, 0x81, 1][..],
            &[0x55, 0xb9, 0x81, 2][..],
            "color interpretation disagree",
        ),
        (
            &[0x55, 0xb7, 0x81, 1][..],
            &[0x55, 0xb7, 0x81, 2][..],
            "chroma siting",
        ),
        (
            &[0x55, 0xbb, 0x81, 1][..],
            &[0x55, 0xbb, 0x81, 2][..],
            "qualified SDR color",
        ),
        // Replace explicit chroma siting with a same-sized Void.
        (
            &[0x55, 0xb7, 0x81, 1][..],
            &[0xec, 0x82, 0, 0][..],
            "chroma siting",
        ),
    ] {
        let positions: Vec<_> = original
            .windows(anchor.len())
            .enumerate()
            .filter_map(|(at, bytes)| (bytes == anchor).then_some(at))
            .collect();
        assert_eq!(positions.len(), 1);
        let mut bytes = original.clone();
        bytes[positions[0]..positions[0] + anchor.len()].copy_from_slice(replacement);
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let error = SourceDecoder::open(file, DecodeLimits::default(), control())
            .err()
            .unwrap();
        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
    assert!(
        SourceDecoder::open(
            File::open(fixture("vp9-sdr-8-limited.webm")).unwrap(),
            DecodeLimits {
                max_dimension: 95,
                ..DecodeLimits::default()
            },
            control()
        )
        .is_err()
    );
}
