//! Stream-copy assembly through the real isolated worker and pinned FFmpeg.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_media::{ConversionError, RemuxLimits, remux_av};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/remux")
        .join(name)
}

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker"))
}

fn limits() -> RemuxLimits {
    RemuxLimits {
        max_output_bytes: 1024 * 1024,
        timeout: Duration::from_secs(60),
    }
}

#[test]
fn separate_picture_and_sound_become_one_progressive_mp4() -> Result<(), Box<dyn std::error::Error>>
{
    let scratch = tempfile::tempdir()?;
    let output = rw(scratch.path().join("assembled.mp4"))?;
    let report = remux_av(
        worker(),
        &mut File::open(fixture("video-fragmented.mp4"))?,
        &mut File::open(fixture("audio-fragmented.m4a"))?,
        &output,
        limits(),
        &AtomicBool::new(false),
    )?;
    assert_eq!((report.video_packets, report.audio_packets), (120, 189));
    assert_eq!((report.width, report.height), (320, 180));
    assert_eq!((report.sample_rate, report.channels), (48_000, 2));
    assert_eq!(output.metadata()?.len(), report.output_bytes);
    let bytes = std::fs::read(scratch.path().join("assembled.mp4"))?;
    // A progressive file has a top-level moov and no movie fragments.
    let boxes = top_level_boxes(&bytes);
    assert!(
        boxes.contains(b"moov") && boxes.contains(b"mdat"),
        "{boxes:?}"
    );
    assert!(!boxes.contains(b"moof"));
    Ok(())
}

#[test]
fn swapped_or_unexpected_streams_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    for (index, (video, audio)) in [
        ("audio-fragmented.m4a", "video-fragmented.mp4"),
        ("video-fragmented.mp4", "video-fragmented.mp4"),
    ]
    .into_iter()
    .enumerate()
    {
        let output = rw(scratch.path().join(format!("{index}.mp4")))?;
        let error = remux_av(
            worker(),
            &mut File::open(fixture(video))?,
            &mut File::open(fixture(audio))?,
            &output,
            limits(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(
            matches!(&error, ConversionError::Worker { code, .. } if code == "invalid_media"),
            "{error}"
        );
    }
    let truncated = scratch.path().join("truncated.mp4");
    std::fs::write(
        &truncated,
        &std::fs::read(fixture("video-fragmented.mp4"))?[..600],
    )?;
    let output = rw(scratch.path().join("truncated-out.mp4"))?;
    assert!(
        remux_av(
            worker(),
            &mut File::open(&truncated)?,
            &mut File::open(fixture("audio-fragmented.m4a"))?,
            &output,
            limits(),
            &AtomicBool::new(false),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn output_budget_and_cancellation_are_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let output = rw(scratch.path().join("small.mp4"))?;
    let error = remux_av(
        worker(),
        &mut File::open(fixture("video-fragmented.mp4"))?,
        &mut File::open(fixture("audio-fragmented.m4a"))?,
        &output,
        RemuxLimits {
            max_output_bytes: 4096,
            ..limits()
        },
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(&error, ConversionError::Worker { code, .. } if code == "output_too_large")
            || matches!(&error, ConversionError::Protocol(_)),
        "{error}"
    );
    let output = rw(scratch.path().join("cancelled.mp4"))?;
    assert!(matches!(
        remux_av(
            worker(),
            &mut File::open(fixture("video-fragmented.mp4"))?,
            &mut File::open(fixture("audio-fragmented.m4a"))?,
            &output,
            limits(),
            &AtomicBool::new(true),
        ),
        Err(ConversionError::Cancelled)
    ));
    Ok(())
}

fn top_level_boxes(bytes: &[u8]) -> Vec<[u8; 4]> {
    let mut boxes = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        boxes.push(bytes[offset + 4..offset + 8].try_into().unwrap());
        if size < 8 {
            break;
        }
        offset += size;
    }
    boxes
}

/// Per-stream timing and payload as the pinned FFmpeg demuxer sees them:
/// clock, start, duration, priming/padding, and every packet's timestamps,
/// duration, flags, size, CRC and skip-samples side data.
fn demuxed(path: &Path, selector: &str) -> String {
    let ffprobe = Path::new(env!("DEADPAN_FFMPEG_PREFIX")).join("bin/ffprobe");
    let output = std::process::Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", selector, "-show_streams", "-show_packets"])
        .args(["-show_data_hash", "CRC32", "-of", "compact", "-show_entries"])
        .arg(
            "stream=codec_name,time_base,start_pts,duration_ts,initial_padding,trailing_padding,extradata_size\
             :stream_tags=language:packet=pts,dts,duration,flags,size,data_hash:packet_side_data",
        )
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn assembly_preserves_every_packet_timestamp_trim_and_byte()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let assembled = scratch.path().join("assembled.mp4");
    remux_av(
        worker(),
        &mut File::open(fixture("video-fragmented.mp4"))?,
        &mut File::open(fixture("audio-fragmented.m4a"))?,
        &rw(&assembled)?,
        limits(),
        &AtomicBool::new(false),
    )?;
    let video = demuxed(&fixture("video-fragmented.mp4"), "v");
    // The picture input starts at a 1001/30000 s composition delay; a
    // millisecond movie clock would have moved it to 990/30000.
    assert!(video.contains("start_pts=1001"), "{video}");
    assert_eq!(demuxed(&assembled, "v"), video);
    assert_eq!(
        demuxed(&assembled, "a"),
        demuxed(&fixture("audio-fragmented.m4a"), "a")
    );
    Ok(())
}

/// The worker re-reads its output to verify it, so outputs are read-write.
fn rw(path: impl AsRef<Path>) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}
