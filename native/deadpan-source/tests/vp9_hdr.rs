//! Authored lossless HDR planes, container metadata, clocks and random seeks.
use deadpan_source::{
    ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, ContentLight, DecodeControl,
    DecodeLimits, MasteringDisplay, SourceDecoder,
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

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
fn copied(data: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(data).unwrap();
    file
}
fn tag(data: &[u8], tag: &[u8]) -> usize {
    let hits: Vec<_> = data
        .windows(tag.len())
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == tag).then_some(i))
        .collect();
    assert_eq!(hits.len(), 1, "unique {tag:?}");
    hits[0] + tag.len()
}
const MASTERING: MasteringDisplay = MasteringDisplay {
    primaries: [[34375, 15625], [12500, 34375], [6250, 3125]],
    white_point: [15625, 15625],
    max_luminance: 10_000_000,
    min_luminance: 10_000,
};
const CASES: &[&str] = &[
    "vp9-pq.mp4",
    "vp9-hlg.mp4",
    "vp9-pq-topleft.mp4",
    "vp9-hlg-anamorphic.mp4",
    "vp9-pq-vfr.mp4",
    "vp9-pq-static.mp4",
    "vp9-pq-mdcv.mp4",
    "vp9-pq.webm",
    "vp9-hlg.webm",
    "vp9-pq-topleft.webm",
    "vp9-hlg-anamorphic.webm",
    "vp9-pq-vfr.webm",
    "vp9-pq-static.webm",
];

fn authored(ordinal: u16) -> Vec<u16> {
    let patches = [64, 324 + ordinal % 4, 480 + ordinal % 4, 940];
    let mut result: Vec<u16> = (0..64)
        .flat_map(|y| {
            (0..96).map(move |x| {
                if y < 32 {
                    patches[usize::from(x / 24)]
                } else {
                    (40 + (2 * x + y + 7 * ordinal) % 180) * 4 + ordinal % 4
                }
            })
        })
        .collect();
    for plane in 0..2 {
        result.extend((0..32).flat_map(|y| {
            (0..48).map(move |x| {
                if y < 16 && (24..36).contains(&x) {
                    if plane == 0 { 420 } else { 580 }
                } else {
                    512
                }
            })
        }));
    }
    result
}

#[test]
fn pq_hlg_keep_authored_ten_bit_planes_scalar_color_clocks_and_threaded_seeks() {
    for name in CASES {
        let mut source = open(name, 1);
        let info = source.info();
        assert_eq!(info.codec, "vp9");
        assert_eq!(info.pixel_format, "yuv420p10le");
        assert!(!info.bwdif_fields);
        assert_eq!(
            info.color.transfer,
            if name.contains("hlg") {
                ColorTransfer::Hlg
            } else {
                ColorTransfer::Pq
            }
        );
        assert_eq!(info.color.matrix, ColorMatrix::Bt2020NonConstant);
        assert_eq!(info.color.primaries, ColorPrimaries::Bt2020);
        assert_eq!(info.color.range, ColorRange::Limited);
        let static_metadata = name.contains("static") || name.contains("mdcv");
        assert_eq!(
            info.color.mastering,
            static_metadata.then_some(MASTERING),
            "{name}"
        );
        assert_eq!(
            info.color.content_light,
            static_metadata.then_some(ContentLight {
                max_cll: 1000,
                max_fall: 400
            })
        );
        assert!(info.color.ignored_static.is_empty());
        assert_eq!(
            (info.sample_aspect_num, info.sample_aspect_den),
            if name.contains("anamorphic") {
                (3, 2)
            } else {
                (1, 1)
            }
        );
        let webm = name.ends_with("webm");
        assert_eq!(
            (info.time_base_num, info.time_base_den),
            (1, if webm { 1000 } else { 60000 })
        );
        let ordinals: Vec<u16> = (0..12)
            .filter(|i| !name.contains("vfr") || ![4, 7].contains(i))
            .collect();
        let mut reference = Vec::new();
        for &ordinal in &ordinals {
            let frame = source.next_yuv420p10(control()).unwrap().unwrap();
            let pts = if webm {
                (i64::from(ordinal) * 1001 + 15) / 30
            } else {
                i64::from(ordinal) * 2002
            };
            assert_eq!(frame.metadata.source.pts, pts, "{name}/{ordinal}");
            assert_eq!(
                frame.metadata.chroma_location,
                if name.contains("topleft") {
                    deadpan_source::ChromaLocation::TopLeft
                } else {
                    deadpan_source::ChromaLocation::Left
                }
            );
            assert_eq!(frame.samples, authored(ordinal), "{name}/{ordinal}");
            // Check the matrix independently of FFmpeg's decoded planes. The
            // middle colored patch is away from every interpolation boundary.
            let rgba = source.copy_current_rgba16(control()).unwrap();
            let y = f64::from(480 + ordinal % 4 - 64) / 876.0;
            let r = y + 2.0 * (1.0 - 0.2627) * 68.0 / 896.0;
            let b = y + 2.0 * (1.0 - 0.0593) * -92.0 / 896.0;
            let g = (y - 0.2627 * r - 0.0593 * b) / (1.0 - 0.2627 - 0.0593);
            let at = 8 * rgba.row_stride_bytes + 60 * 8;
            for (channel, value) in [r, g, b, 1.0].into_iter().enumerate() {
                let actual = u16::from_le_bytes([
                    rgba.rgba[at + channel * 2],
                    rgba.rgba[at + channel * 2 + 1],
                ]);
                assert!(
                    (f64::from(actual) - value.clamp(0.0, 1.0) * 65535.0).abs() <= 1.0,
                    "{name}/{ordinal}/{channel}"
                );
            }
            reference.push(frame);
        }
        assert!(source.next_yuv420p10(control()).unwrap().is_none());
        for threads in [1, 8, 16] {
            let mut source = open(name, threads);
            for expected in &reference {
                assert_eq!(
                    &source.next_yuv420p10(control()).unwrap().unwrap(),
                    expected
                );
            }
            for expected in reference.iter().rev() {
                source
                    .seek(expected.metadata.source.pts, control())
                    .unwrap();
                loop {
                    let actual = source.next_yuv420p10(control()).unwrap().unwrap();
                    if actual.metadata.source.pts == expected.metadata.source.pts {
                        assert_eq!(&actual, expected, "{name}/{threads}");
                        break;
                    }
                    assert!(actual.metadata.source.pts < expected.metadata.source.pts);
                }
            }
        }
    }
}

#[test]
fn static_metadata_is_exact_or_recorded_as_ignored_never_silently_dropped() {
    let mp4 = std::fs::read(fixture("vp9-pq-static.mp4")).unwrap();
    for (field, bytes, mastering) in [
        (
            tag(&mp4, b"SmDm") + 4,
            45057_u16.to_be_bytes().to_vec(),
            true,
        ),
        (
            tag(&mp4, b"CoLL") + 6,
            1400_u16.to_be_bytes().to_vec(),
            false,
        ),
    ] {
        let mut changed = mp4.clone();
        changed[field..field + bytes.len()].copy_from_slice(&bytes);
        let source =
            SourceDecoder::open(copied(&changed), DecodeLimits::default(), control()).unwrap();
        assert_eq!(source.info().color.ignored_static.mastering, mastering);
        assert_eq!(source.info().color.ignored_static.content_light, !mastering);
    }
    let webm = std::fs::read(fixture("vp9-pq-static.webm")).unwrap();
    // MaxFALL=0 is valid unknown data but FFmpeg drops the whole pair. Keep it.
    let mut zero = webm.clone();
    let fall = tag(&zero, &[0x55, 0xbd, 0x82]);
    zero[fall..fall + 2].copy_from_slice(&[0, 0]);
    let source = SourceDecoder::open(copied(&zero), DecodeLimits::default(), control()).unwrap();
    assert_eq!(
        source.info().color.content_light,
        Some(ContentLight {
            max_cll: 1000,
            max_fall: 0
        })
    );
    assert!(source.info().color.ignored_static.is_empty());
    // The demuxer drops this malformed mastering declaration entirely.
    let mut changed = webm.clone();
    for id in [0xd1, 0xd9, 0xda] {
        let at = tag(&changed, &[0x55, id, 0x88]);
        changed[at..at + 8].copy_from_slice(&0.0_f64.to_be_bytes());
    }
    let source = SourceDecoder::open(copied(&changed), DecodeLimits::default(), control()).unwrap();
    assert_eq!(source.info().color.mastering, None);
    assert!(source.info().color.ignored_static.mastering);
}

#[test]
fn malformed_or_duplicate_vp9_full_boxes_are_refused() {
    let original = std::fs::read(fixture("vp9-pq-static.mp4")).unwrap();
    for marker in [b"SmDm", b"CoLL"] {
        for offset in 0..4 {
            let mut changed = original.clone();
            let at = tag(&changed, marker);
            changed[at + offset] = 1;
            assert!(
                SourceDecoder::open(copied(&changed), DecodeLimits::default(), control()).is_err()
            );
        }
    }
    let mut changed = original.clone();
    let at = tag(&changed, b"CoLL");
    changed[at - 4..at].copy_from_slice(b"mdcv");
    let error = SourceDecoder::open(copied(&changed), DecodeLimits::default(), control())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("duplicate mastering display"),
        "{error}"
    );
    for offset in [20, 24] {
        let mut changed = original.clone();
        let at = tag(&changed, b"SmDm") + offset;
        changed[at..at + 4].copy_from_slice(&0x8000_0000_u32.to_be_bytes());
        let error = SourceDecoder::open(copied(&changed), DecodeLimits::default(), control())
            .err()
            .unwrap();
        assert!(
            error
                .to_string()
                .contains("demuxer rational representation"),
            "{error}"
        );
    }
}
