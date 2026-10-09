//! Independently authored SDR code values, clocks and threaded HEVC seeks.
use deadpan_source::{ColorRange, ColorTransfer, DecodeControl, DecodeLimits, SourceDecoder};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}
fn pixel(frame: &deadpan_source::DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + c * 2], frame.rgba[at + c * 2 + 1]])
    })
}

#[test]
fn main_and_main10_preserve_limited_full_code_values_and_b_frame_timing() {
    for bits in [8, 10] {
        for full in [false, true] {
            let name = format!(
                "hevc-sdr-{bits}-{}.mp4",
                if full { "full" } else { "limited" }
            );
            let mut decoder = open(&name, 1);
            assert_eq!(decoder.info().codec, "hevc");
            assert_eq!(decoder.info().color.transfer, ColorTransfer::Bt709);
            assert_eq!(
                decoder.info().color.range,
                if full {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                }
            );
            assert_eq!(
                (decoder.info().time_base_num, decoder.info().time_base_den),
                (1, 60000)
            );
            assert!(!decoder.info().bwdif_fields);
            let scale = f64::from(1 << (bits - 8));
            let maximum = f64::from((1 << bits) - 1);
            let (offset, range, chroma_range) = if full {
                (0.0, maximum, maximum)
            } else {
                (16.0 * scale, 219.0 * scale, 224.0 * scale)
            };
            let mut reference = Vec::new();
            for ordinal in 0..12 {
                let frame = decoder.next_rgba16(control()).unwrap().unwrap();
                assert_eq!(frame.metadata.pts, ordinal * 2002);
                assert_eq!(frame.metadata.reported_duration, Some(2002));
                assert_eq!(frame.sample_bits, 16);
                let levels = [
                    offset,
                    81.0 * scale + (ordinal % 4) as f64,
                    120.0 * scale + (ordinal % 4) as f64,
                    if full { maximum } else { 235.0 * scale },
                ];
                for (patch, level) in levels.into_iter().enumerate() {
                    let y = (level - offset) / range;
                    let (cb, cr) = if patch == 2 {
                        (-23.0 * scale / chroma_range, 17.0 * scale / chroma_range)
                    } else {
                        (0.0, 0.0)
                    };
                    let r = y + 2.0 * (1.0 - 0.2126) * cr;
                    let b = y + 2.0 * (1.0 - 0.0722) * cb;
                    let g = (y - 0.2126 * r - 0.0722 * b) / (1.0 - 0.2126 - 0.0722);
                    for row in 4..28 {
                        for x in patch * 24 + 4..patch * 24 + 20 {
                            let actual = pixel(&frame, x, row);
                            assert_eq!(actual[3], 65535);
                            for (channel, expected) in [r, g, b].into_iter().enumerate() {
                                let expected = expected.clamp(0.0, 1.0) * 65535.0;
                                assert!(
                                    (f64::from(actual[channel]) - expected).abs() <= 1.0,
                                    "{name} {ordinal}/{patch} channel {channel}: {} vs {expected}",
                                    actual[channel]
                                );
                            }
                        }
                    }
                }
                reference.push(frame);
            }
            assert!(decoder.next_rgba16(control()).unwrap().is_none());
            if bits == 10 {
                assert_ne!(
                    pixel(&reference[0], 30, 8),
                    pixel(&reference[1], 30, 8),
                    "ten-bit low bits survive"
                );
            }
            for threads in [1, 8, 16] {
                let mut decoder = open(&name, threads);
                for expected in &reference {
                    assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
                }
                for expected in reference.iter().rev() {
                    decoder.seek(expected.metadata.pts, control()).unwrap();
                    loop {
                        let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                        if actual.metadata.pts == expected.metadata.pts {
                            assert_eq!(&actual, expected, "{name} {threads}");
                            break;
                        }
                        assert!(
                            actual.metadata.pts < expected.metadata.pts,
                            "{name} threads={threads} sought {} but got {}",
                            expected.metadata.pts,
                            actual.metadata.pts
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn previously_refused_ten_bit_sdr_is_now_admitted() {
    let mut decoder = open("hevc-ten-bit-sdr.mp4", 1);
    assert_eq!(decoder.info().color.transfer, ColorTransfer::Bt709);
    assert_eq!(
        decoder.next_rgba16(control()).unwrap().unwrap().sample_bits,
        16
    );
}

#[test]
fn high10_sdr_preserves_range_precision_and_repeated_seek_identity() {
    for full in [false, true] {
        let name = format!("h264-sdr-10-{}.mp4", if full { "full" } else { "limited" });
        let mut source = open(&name, 1);
        assert_eq!(source.info().codec, "h264");
        assert_eq!(source.info().pixel_format, "yuv420p10le");
        assert_eq!(source.info().color.transfer, ColorTransfer::Bt709);
        assert_eq!(
            source.info().color.range,
            if full {
                ColorRange::Full
            } else {
                ColorRange::Limited
            }
        );
        assert_eq!(
            (source.info().time_base_num, source.info().time_base_den),
            (1, 60000)
        );
        let mut frames = Vec::new();
        for ordinal in 0..12 {
            let frame = source.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(frame.sample_bits, 16);
            assert_eq!(frame.metadata.pts, ordinal * 2002);
            assert_eq!(frame.metadata.reported_duration, Some(2002));
            let level = (81 * 4 + ordinal % 4) as f64;
            let expected = if full {
                level / 1023.0
            } else {
                (level - 64.0) / 876.0
            } * 65535.0;
            let actual = pixel(&frame, 30, 8);
            assert_eq!(actual[3], 65535);
            // These High10 pictures use QP 1: allow one input code of quantization.
            for channel in &actual[..3] {
                assert!(
                    (f64::from(*channel) - expected).abs() <= 76.0,
                    "{name} {ordinal}: {actual:?} vs {expected}"
                );
            }
            frames.push(frame);
        }
        assert!(source.next_rgba16(control()).unwrap().is_none());
        assert_ne!(pixel(&frames[0], 30, 8), pixel(&frames[1], 30, 8));
        for threads in [1, 8, 16] {
            let mut decoder = open(&name, threads);
            for expected in &frames {
                assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
            }
            for target in (0..frames.len()).rev() {
                let anchor = frames[..=target]
                    .iter()
                    .rposition(|frame| frame.metadata.keyframe)
                    .unwrap();
                decoder
                    .seek_to(
                        frames[anchor].metadata.pts,
                        frames[target].metadata.pts,
                        control(),
                    )
                    .unwrap();
                loop {
                    let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                    if actual.metadata.pts >= frames[target].metadata.pts {
                        assert_eq!(actual, frames[target], "{name} {target} threads={threads}");
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn repeated_hdr_seeks_discard_every_old_pending_picture_before_reusing_the_decoder() {
    for name in ["hdr-pq-av.mp4", "hevc-pq-open-gop.mp4"] {
        let mut source = open(name, 1);
        let mut frames = Vec::new();
        while let Some(frame) = source.next_rgba16(control()).unwrap() {
            frames.push(frame);
        }
        for threads in [1, 8, 16] {
            let mut decoder = open(name, threads);
            for target in (0..frames.len()).rev() {
                let anchor = frames[..=target]
                    .iter()
                    .rposition(|frame| frame.metadata.keyframe)
                    .unwrap();
                decoder
                    .seek_to(
                        frames[anchor].metadata.pts,
                        frames[target].metadata.pts,
                        control(),
                    )
                    .unwrap();
                loop {
                    let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                    if actual.metadata.pts >= frames[target].metadata.pts {
                        assert_eq!(
                            actual.metadata.pts, frames[target].metadata.pts,
                            "{name} {target} threads={threads}"
                        );
                        assert_eq!(
                            actual.rgba, frames[target].rgba,
                            "{name} {target} threads={threads}"
                        );
                        break;
                    }
                }
            }
        }
    }
}
