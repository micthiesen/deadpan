//! Developer qualification for HEVC Main10 HDR output. Encodes the deterministic
//! HDR probe for PQ/HLG x hardware/software x B-frame requests, keeps the files
//! and prints a JSON report including the pinned ffprobe's stream observations.
//! This is evidence collection, not Render or publication admission.
//!
//! cargo run -p deadpan-encode --example qualify_hdr -- OUTPUT_DIR [WIDTH HEIGHT FPS_NUM FPS_DEN]
use std::{
    error::Error,
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_encode::probe::HdrEncoderProbe;
use deadpan_encode::{
    BFramePolicy, EncodeLimits, EncoderMode, EncoderSession, HdrTransfer, NextInput,
};
use serde_json::{Value, json};

fn encode(
    probe: &HdrEncoderProbe,
    mode: EncoderMode,
    b_frames: BFramePolicy,
    path: &Path,
) -> Result<Value, Box<dyn Error>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();
    let contract = probe.contract(mode, b_frames)?;
    let bitrate = contract.policy().video_bitrate;
    let mut session = EncoderSession::open(
        file,
        contract,
        EncodeLimits::default(),
        &cancelled,
        started + Duration::from_secs(300),
    )?;
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes)?];
    let mut left = [0.0; 1024];
    let mut right = [0.0; 1024];
    loop {
        match session.next_input()? {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                probe.fill_picture(ordinal, &mut picture)?;
                session.push_picture(ordinal, pts, duration, &picture)?;
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let count = usize::try_from(samples)?;
                probe.fill_audio(first_sample, &mut left[..count], &mut right[..count])?;
                session.push_audio(first_sample, &left[..count], &right[..count])?;
            }
            NextInput::Finish => break,
        }
    }
    let output = session.finish_with_light(probe.content_light())?;
    let seconds = probe.config().video_frames as f64 * f64::from(probe.config().frame_rate[1])
        / f64::from(probe.config().frame_rate[0]);
    Ok(json!({
        "elapsed_ms": started.elapsed().as_millis(),
        "requested_video_bitrate": bitrate,
        "report": output.report(),
        "video_codec": output.video_codec(),
        "duration_seconds": seconds,
        "file_bitrate": output.report().output_bytes as f64 * 8.0 / seconds,
        "ffprobe": ffprobe(path)?,
    }))
}

fn ffprobe(path: &Path) -> Result<Value, Box<dyn Error>> {
    let prefix = std::env::var("DEADPAN_FFMPEG_PREFIX")?;
    let output = Command::new(format!("{prefix}/bin/ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_streams",
            "-show_entries",
            "packet=pts,dts,flags,size",
            "-of",
            "json",
        ])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let stream = &value["streams"][0];
    let packets = value["packets"].as_array().cloned().unwrap_or_default();
    let mut previous_pts = i64::MIN;
    let mut reordered = 0;
    let mut pts_before_dts = 0;
    let mut video_bytes = 0;
    for packet in &packets {
        let pts = packet["pts"].as_i64().unwrap_or_default();
        let dts = packet["dts"].as_i64().unwrap_or_default();
        reordered += u32::from(pts < previous_pts);
        pts_before_dts += u32::from(pts < dts);
        previous_pts = previous_pts.max(pts);
        video_bytes += packet["size"]
            .as_str()
            .and_then(|size| size.parse::<u64>().ok())
            .unwrap_or_default();
    }
    Ok(json!({
        "codec_name": stream["codec_name"],
        "codec_tag_string": stream["codec_tag_string"],
        "profile": stream["profile"],
        "pix_fmt": stream["pix_fmt"],
        "color_range": stream["color_range"],
        "color_space": stream["color_space"],
        "color_transfer": stream["color_transfer"],
        "color_primaries": stream["color_primaries"],
        "chroma_location": stream["chroma_location"],
        "has_b_frames": stream["has_b_frames"],
        "side_data_list": stream["side_data_list"],
        "packets": packets.len(),
        "packets_presented_out_of_decode_order": reordered,
        "packets_pts_before_dts": pts_before_dts,
        "video_bytes": video_bytes,
    }))
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let directory = Path::new(arguments.first().ok_or("usage: OUTPUT_DIR [W H N D]")?);
    std::fs::create_dir_all(directory)?;
    let number = |index: usize, default: u32| -> Result<u32, Box<dyn Error>> {
        Ok(arguments
            .get(index)
            .map(|text| text.parse())
            .transpose()?
            .unwrap_or(default))
    };
    let raster = [number(1, 1920)?, number(2, 1080)?];
    let rate = [number(3, 30)?, number(4, 1)?];
    let mut results = Vec::new();
    for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
        let probe = HdrEncoderProbe::new(raster, rate, transfer)?;
        // Planar 10-bit LE input is byte-identical to raw yuv420p10le, so an
        // external decoder can compare decoded planes against this reference.
        let reference = directory.join(format!(
            "{}-{}x{}-input.yuv",
            match transfer {
                HdrTransfer::Pq => "pq",
                HdrTransfer::Hlg => "hlg",
            },
            raster[0],
            raster[1]
        ));
        let mut raw = Vec::new();
        let mut picture = vec![0; usize::try_from(probe.config().picture_bytes)?];
        for ordinal in 0..probe.config().video_frames {
            probe.fill_picture(ordinal, &mut picture)?;
            raw.extend_from_slice(&picture);
        }
        std::fs::write(&reference, raw)?;
        for mode in [EncoderMode::Hardware, EncoderMode::Software] {
            for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
                let name = format!(
                    "{}-{}x{}-{:?}-{:?}.mp4",
                    match transfer {
                        HdrTransfer::Pq => "pq",
                        HdrTransfer::Hlg => "hlg",
                    },
                    raster[0],
                    raster[1],
                    mode,
                    b_frames
                )
                .to_lowercase();
                let path = directory.join(&name);
                let _ = std::fs::remove_file(&path);
                let outcome = match encode(&probe, mode, b_frames, &path) {
                    Ok(value) => json!({"ok": value}),
                    Err(error) => {
                        let kind = error
                            .downcast_ref::<deadpan_encode::EncodeError>()
                            .map(|error| format!("{:?}", error.kind()));
                        json!({"error": error.to_string(), "kind": kind})
                    }
                };
                results.push(json!({
                    "file": name,
                    "transfer": transfer,
                    "mode": mode,
                    "b_frames": b_frames,
                    "raster": raster,
                    "frame_rate": rate,
                    "frames": probe.config().video_frames,
                    "outcome": outcome,
                }));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
