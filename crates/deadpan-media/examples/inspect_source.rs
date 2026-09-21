//! Developer evidence for measured source indexes and persistent random seeks.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Seek, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use deadpan_core::{AssetId, SourceFrameId};
    use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
    use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
    use rustix::fs::{Mode, OFlags, open};
    use serde_json::json;
    use sha2::{Digest, Sha256};

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: inspect_source ABSOLUTE_INPUT NEW_REPORT_JSON".into());
    }
    let source = PathBuf::from(&args[0]);
    let report_path = PathBuf::from(&args[1]);
    if !source.is_absolute() || !report_path.is_absolute() {
        return Err("qualification paths must be absolute".into());
    }
    let mut input = File::from(open(
        &source,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let length = input.metadata()?.len();
    if !input.metadata()?.is_file() || length == 0 || length > 256 * 1024 * 1024 {
        return Err("qualification input must be a nonempty regular file at most 256 MiB".into());
    }
    let mut hash = Sha256::new();
    let started = Instant::now();
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    loop {
        if started.elapsed() > Duration::from_secs(60) {
            return Err("input hashing exceeded 60 seconds".into());
        }
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or("hash length overflow")?;
        if copied > length {
            return Err("input grew while hashing".into());
        }
        hash.update(&buffer[..count]);
    }
    if copied != length {
        return Err("input changed while hashing".into());
    }
    let digest: [u8; 32] = hash.finalize().into();
    input.rewind()?;
    let cancelled = AtomicBool::new(false);
    let opened = Instant::now();
    let mut session = SourceSession::open_verified(
        &mut input,
        SourceContentIdentity::new(digest, length)?,
        AssetId::new("qualification-source")?,
        SourceSessionLimits::default(),
        &cancelled,
    )?;
    let opening_seconds = opened.elapsed().as_secs_f64();
    drop(input);
    let index = session.index().to_json()?;
    if &SourceIndexSnapshot::from_json(&index)? != session.index() {
        return Err("serialized source index changed meaning".into());
    }
    let info = session.info().clone();
    let count = session.index().index().frames().len() as u64;
    let last = count - 1;
    let mut results = Vec::new();
    let mut retained = None;
    for target in [0, last, 0, (last / 2), 1.min(last), last, 0] {
        let began = Instant::now();
        let frame = session.frame(SourceFrameId(target), Duration::from_secs(10), &cancelled)?;
        let digest = hex(&Sha256::digest(&frame.rgba));
        results.push(json!({"frame":target,"pts":frame.metadata.pts,
            "reported_duration":frame.metadata.reported_duration,"rgba_sha256":digest,
            "rgba_bytes":frame.rgba.len(),"seconds":began.elapsed().as_secs_f64()}));
        if target == 0 && retained.is_none() {
            retained = Some(frame);
        }
    }
    let frame_index = session.index().index();
    let result = json!({
        "scope":"actual source snapshot, measured presentation index and persistent decoder seeks; not playback, project import, audio, HDR or display qualification",
        "source":source,"content":session.index().content(),"opening_seconds":opening_seconds,
        "stream_index":info.stream_index,"codec":info.codec,"pixel_format":info.pixel_format,
        "dimensions":[info.width,info.height],"sample_aspect_ratio":[info.sample_aspect_num,info.sample_aspect_den],
        "rotation_quarter_turns":info.rotation_quarter_turns,"color":format!("{:?}",info.color),
        "time_base":[info.time_base_num,info.time_base_den],"stream_start":info.stream_start,
        "stream_duration":info.stream_duration,"container_start_microseconds":info.container_start,
        "container_duration_microseconds":info.container_duration,
        "frames":count,"first_pts":frame_index.frames()[0].pts,"terminal_end":frame_index.terminal_end(),
        "terminal_provenance":frame_index.terminal_provenance(),"seeks":results,
        "index_sha256":hex(&Sha256::digest(&index))
    });
    drop(session);
    let retained = retained.ok_or("missing retained frame")?;
    if hex(&Sha256::digest(&retained.rgba))
        != results[0]["rgba_sha256"]
            .as_str()
            .ok_or("missing pixel digest")?
    {
        return Err("retained source pixels changed after decoder destruction".into());
    }
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_path)?;
    serde_json::to_writer_pretty(&mut report, &result)?;
    writeln!(report)?;
    report.sync_all()?;
    let mut cache = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path.with_extension("index.json"))?;
    cache.write_all(&index)?;
    cache.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("source inspection requires a supported Unix host");
    std::process::exit(1);
}
