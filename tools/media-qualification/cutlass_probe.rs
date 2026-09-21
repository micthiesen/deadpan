//! Downstream comparison harness. Built only in a temporary standalone crate.
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

use cutlass_core::{PixelFormat, Rational, RationalTime, VideoFrame};
use cutlass_decoder::{OutputMode, open_video_decoder};

fn hash(frame: &VideoFrame) -> u64 {
    let mut hash = DefaultHasher::new();
    frame.format.hash(&mut hash);
    let image = frame.cpu().expect("CPU frame");
    for (index, plane) in image.planes.iter().enumerate() {
        let width = match frame.format {
            PixelFormat::Nv12 => frame.width() as usize,
            PixelFormat::Yuv420p if index > 0 => frame.width() as usize / 2,
            PixelFormat::Yuv420p => frame.width() as usize,
            other => panic!("unqualified comparison pixel format {other:?}"),
        };
        for row in 0..plane.rows {
            plane.data[row * plane.stride..row * plane.stride + width].hash(&mut hash);
        }
    }
    hash.finish()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3, "usage: cutlass_probe FILE cfr|vfr|offset");
    let mut decoder =
        open_video_decoder(Path::new(&args[1]), OutputMode::Cpu).expect("open decoder");
    let mut reference = Vec::new();
    let mut expected_pts = if args[2] == "offset" { 60060 } else { 0 };
    let start = Instant::now();
    while let Some(frame) = decoder.next_frame().expect("linear decode") {
        let index = reference.len();
        assert!(index < 120, "no extra frames");
        let expected = RationalTime::new(expected_pts, Rational::new(30000, 1));
        assert_eq!(
            frame.pts.compare(expected),
            Ordering::Equal,
            "exact PTS for frame {index}"
        );
        reference.push((frame.pts, hash(&frame)));
        expected_pts += 1001
            * if args[2] == "vfr" {
                1 + index as i64 % 3
            } else {
                1
            };
    }
    assert_eq!(reference.len(), 120, "complete fixture decoded");
    let linear_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut seek_ms = Vec::new();
    for index in [0, 119, 1, 60, 14, 16, 29, 30, 61, 5, 118, 0] {
        let start = Instant::now();
        let frame = decoder
            .frame_at(reference[index].0)
            .expect("seek decode")
            .expect("requested frame");
        assert_eq!(
            frame.pts.compare(reference[index].0),
            Ordering::Equal,
            "seek target PTS {index}"
        );
        assert_eq!(hash(&frame), reference[index].1, "seek hash {index}");
        seek_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "{{\"frames\":120,\"exact_seeks\":12,\"linear_decode_ms\":{linear_ms},\"seek_ms\":{seek_ms:?}}}"
    );
}
