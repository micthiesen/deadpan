//! Compare bounded FFV1 slice decoders with a single-threaded pixel reference.
//! Usage: compare_ffv1_threads ABSOLUTE_MOVIE ABSOLUTE_NEW_REPORT_JSON

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
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
fn open(file: &File, threads: u32) -> Result<SourceDecoder> {
    Ok(SourceDecoder::open(
        file.try_clone()?,
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )?)
}
fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 || args.iter().any(|path| !path.is_absolute()) {
        return Err("usage: compare_ffv1_threads ABSOLUTE_MOVIE ABSOLUTE_NEW_REPORT_JSON".into());
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
    let mut reference = open(&file, 1)?;
    let info = reference.info().clone();
    if info.codec != "ffv1" {
        return Err("qualification requires an FFV1 movie".into());
    }
    let mut expected = Vec::new();
    while let Some(frame) = reference.next_rgba(control())? {
        if expected.len() >= 10000 {
            return Err("qualification movie exceeds 10,000 pictures".into());
        }
        expected.push((frame.metadata, blake3::hash(&frame.rgba)));
    }
    drop(reference);
    if expected.is_empty() {
        return Err("no reference pictures".into());
    }
    let mut cases = Vec::new();
    for threads in [1, 2, 4, 8, 12, 16] {
        for pass in 0..3 {
            let began = Instant::now();
            let mut decoder = open(&file, threads)?;
            let open_ms = began.elapsed().as_secs_f64() * 1000.0;
            let work = decoder.work();
            if decoder.info() != &info
                || work.codec_threads != threads
                || work.frame_threading
                || work.slice_threading != (threads > 1)
            {
                return Err("FFV1 metadata or actual slice-only configuration differs".into());
            }
            let mut sequential_ms = Vec::new();
            for (metadata, hash) in &expected {
                let began = Instant::now();
                let frame = decoder.next_rgba(control())?.ok_or("early end")?;
                sequential_ms.push(began.elapsed().as_secs_f64() * 1000.0);
                if frame.metadata != *metadata || blake3::hash(&frame.rgba) != *hash {
                    return Err("sequential picture differs from single-threaded reference".into());
                }
            }
            if decoder.next_metadata(control())?.is_some() {
                return Err("extra picture".into());
            }
            let mut seeks = Vec::new();
            let mut random = 3_u64;
            for _ in 0..50 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let target = usize::try_from(random % expected.len() as u64)?;
                let anchor = expected[..=target]
                    .iter()
                    .rposition(|(metadata, _)| metadata.keyframe)
                    .ok_or("no preceding keyframe")?;
                let began = Instant::now();
                decoder.seek(expected[anchor].0.pts, control())?;
                let metadata = loop {
                    let next = decoder
                        .next_metadata(control())?
                        .ok_or("seek missed picture")?;
                    if next.pts >= expected[target].0.pts {
                        break next;
                    }
                };
                let frame = decoder.copy_current_rgba(control())?;
                let ms = began.elapsed().as_secs_f64() * 1000.0;
                if metadata.pts != expected[target].0.pts
                    || metadata.reported_duration != expected[target].0.reported_duration
                    || blake3::hash(&frame.rgba) != expected[target].1
                {
                    return Err("seek picture differs from single-threaded reference".into());
                }
                seeks.push(json!({"target":target,"ms":ms}));
            }
            cases.push(json!({"threads":threads,"pass":pass,"open_ms":open_ms,
                "slice_threading":work.slice_threading,"frame_threading":work.frame_threading,
                "sequential_ms":sequential_ms,"seeks":seeks}));
        }
    }
    if hash_file(&mut file)? != source_sha256 {
        return Err("source changed during qualification".into());
    }
    let report = json!({"source":args[0],"sha256":source_sha256,"bytes":stat.len(),
        "codec":info.codec,"pixel_format":info.pixel_format,"raster":[info.width,info.height],
        "pictures":expected.len(),"cases":cases,
        "reference_pictures":expected.iter().map(|(m, hash)| json!({
            "pts":m.pts,"duration":m.reported_duration,"keyframe":m.keyframe,
            "rgba_blake3":hash.to_hex().as_str()
        })).collect::<Vec<_>>(),
        "scope":"Single-frame slice threading, full RGBA/metadata equality and 50 random seeks per case. Timing includes decode and pixel copy, excludes hashing. No GPU or audio."});
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer_pretty(&mut output, &report)?;
    output.write_all(b"\n")?;
    Ok(())
}
