//! Compare VideoToolbox with canonical software pictures on a supplied movie.
//! Qualification only: no project writes, proxies, audio, GPU or window.
//! Usage: compare_decode ABSOLUTE_MOVIE NEW_REPORT_JSON

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_media::source_session::interactive_decode_threads_for;
use deadpan_source::{
    DecodeAcceleration, DecodeControl, DecodeLimits, SourceDecoder, SourceFrameMetadata,
};
use serde_json::json;
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(60),
        cancelled: &CANCELLED,
    }
}
fn hash_file(file: &mut File) -> Result<String> {
    file.rewind()?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn pixels(decoder: &mut SourceDecoder) -> Result<blake3::Hash> {
    let frame = if decoder.info().pixel_format.contains("10") {
        decoder.copy_current_rgba16(control())?
    } else {
        decoder.copy_current_rgba(control())?
    };
    Ok(blake3::hash(&frame.rgba))
}
fn same_metadata(a: SourceFrameMetadata, b: SourceFrameMetadata) -> bool {
    // Packet-release DTS and recovery-point key flags can depend on seek
    // context. Actual picture PTS/duration and every output byte must match.
    a.pts == b.pts && a.reported_duration == b.reported_duration
}
fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 || args.iter().any(|path| !path.is_absolute()) {
        return Err("usage: compare_decode ABSOLUTE_MOVIE ABSOLUTE_NEW_REPORT_JSON".into());
    }
    let mut file = File::from(rustix::fs::open(
        &args[0],
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let stat = file.metadata()?;
    if !stat.is_file() || stat.len() == 0 || stat.len() > 512 * 1024 * 1024 {
        return Err("qualification movie must be a nonempty regular file at most 512 MiB".into());
    }
    let source_sha256 = hash_file(&mut file)?;
    let reference_start = Instant::now();
    let mut reference = SourceDecoder::open(file.try_clone()?, DecodeLimits::default(), control())?;
    let info = reference.info().clone();
    let mut expected = Vec::new();
    while let Some(metadata) = reference.next_metadata(control())? {
        if expected.len() >= 10000 {
            return Err("qualification movie exceeds 10,000 pictures".into());
        }
        expected.push((metadata, pixels(&mut reference)?));
    }
    let reference_seconds = reference_start.elapsed().as_secs_f64();
    drop(reference);
    if expected.is_empty() {
        return Err("no reference pictures".into());
    }
    let threads = interactive_decode_threads_for(info.width, info.height);
    let mut cases = Vec::new();
    for prefer_videotoolbox in [false, true] {
        let start = Instant::now();
        let mut decoder = SourceDecoder::open(
            file.try_clone()?,
            DecodeLimits {
                threads,
                acceleration: if prefer_videotoolbox {
                    DecodeAcceleration::PreferVideoToolbox
                } else {
                    DecodeAcceleration::Software
                },
                ..DecodeLimits::default()
            },
            control(),
        )?;
        if decoder.info() != &info {
            return Err("serving metadata differs from software reference".into());
        }
        for (ordinal, (metadata, hash)) in expected.iter().enumerate() {
            let actual = decoder
                .next_metadata(control())?
                .ok_or("early end of movie")?;
            if !same_metadata(actual, *metadata) || pixels(&mut decoder)? != *hash {
                return Err(format!(
                    "picture {ordinal} differs, VideoToolbox={prefer_videotoolbox}"
                )
                .into());
            }
        }
        if decoder.next_metadata(control())?.is_some() {
            return Err("extra output picture".into());
        }
        let sequential_seconds = start.elapsed().as_secs_f64();
        let mut seeks = Vec::new();
        let mut random = 3_u64;
        for _ in 0..100 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let target = usize::try_from(random % expected.len() as u64)?;
            let anchor = expected[..=target]
                .iter()
                .rposition(|(m, _)| m.keyframe)
                .ok_or("no preceding keyframe")?;
            let began = Instant::now();
            decoder.seek_to(expected[anchor].0.pts, expected[target].0.pts, control())?;
            let metadata = loop {
                let next = decoder
                    .next_metadata(control())?
                    .ok_or("seek missed picture")?;
                if next.pts >= expected[target].0.pts {
                    break next;
                }
            };
            let hash = pixels(&mut decoder)?;
            let milliseconds = began.elapsed().as_secs_f64() * 1000.0;
            if !same_metadata(metadata, expected[target].0) || hash != expected[target].1 {
                return Err(
                    format!("seek {target} differs, VideoToolbox={prefer_videotoolbox}").into(),
                );
            }
            seeks.push(json!({"target":target,"anchor":anchor,"milliseconds":milliseconds}));
        }
        let mut times: Vec<_> = seeks
            .iter()
            .map(|s| s["milliseconds"].as_f64().unwrap())
            .collect();
        times.sort_by(f64::total_cmp);
        let work = decoder.work();
        if prefer_videotoolbox && work.hardware_frames == 0 {
            return Err("VideoToolbox was not exercised".into());
        }
        cases.push(
            json!({"prefer_videotoolbox":prefer_videotoolbox,"threads":threads,
            "sequential_seconds":sequential_seconds,"seeks":seeks,
            "seek_ms":{"p50":times[49],"p95":times[94],"max":times[99]},
            "work":{"frames":work.frames,"packets":work.packets,"io_bytes":work.io_bytes,
                "software_buffers":work.decoded_pictures,"hardware_frames":work.hardware_frames,
                "hardware_transfers":work.hardware_transfers}}),
        );
        eprintln!(
            "VideoToolbox={prefer_videotoolbox}: {} exact pictures, 100 exact seeks; p95 {:.3} ms",
            expected.len(),
            times[94]
        );
    }
    if hash_file(&mut file)? != source_sha256 {
        return Err("source changed during qualification".into());
    }
    let report = json!({"source":args[0],"sha256":source_sha256,"bytes":stat.len(),
        "codec":info.codec,"pixel_format":info.pixel_format,"raster":[info.width,info.height],
        "pictures":expected.len(),"reference_seconds":reference_seconds,"cases":cases,
        "reference_pictures":expected.iter().map(|(m, hash)| json!({
            "pts":m.pts,"duration":m.reported_duration,"keyframe":m.keyframe,
            "rgba_blake3":hash.to_hex().as_str()
        })).collect::<Vec<_>>(),
        "scope":"Full canonical software pixel/PTS/duration equality, then 100 exact random seeks per serving path. Timing includes requested pixel copy and hash; no GPU, proxies or audio."});
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer_pretty(&mut output, &report)?;
    output.write_all(b"\n")?;
    Ok(())
}
