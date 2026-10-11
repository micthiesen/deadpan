//! Joined download descriptors must retain each container's own address space.
use deadpan_source::{
    DecodeControl, DecodeLimits,
    audio::{AudioDecodeLimits, inspect_matroska_opus_region},
    inspect_matroska_video_region,
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

#[test]
fn joined_regions_match_standalone_admission_and_refuse_boundary_escapes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let video = std::fs::read(root.join("fixtures/vp9-pq-static.webm")).unwrap();
    let audio = std::fs::read(root.join("audio-fixtures/opus-mono-preskip.webm")).unwrap();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&video).unwrap();
    file.write_all(&audio).unwrap();
    let cancelled = AtomicBool::new(false);
    let control = || DecodeControl {
        cancelled: &cancelled,
        timeout: Duration::from_secs(10),
    };
    let inspect_audio = |file: &File, offset, length| {
        inspect_matroska_opus_region(
            file,
            offset,
            length,
            AudioDecodeLimits::default(),
            control(),
        )
    };
    let inspect_video = |file: &File, offset, length| {
        inspect_matroska_video_region(file, offset, length, DecodeLimits::default(), control())
    };
    let standalone = File::open(root.join("audio-fixtures/opus-mono-preskip.webm")).unwrap();
    let expected = inspect_audio(&standalone, 0, audio.len() as u64).unwrap();
    assert_eq!(
        inspect_audio(&file, video.len() as u64, audio.len() as u64).unwrap(),
        expected
    );
    let standalone = File::open(root.join("fixtures/vp9-pq-static.webm")).unwrap();
    assert_eq!(
        inspect_video(&file, 0, video.len() as u64).unwrap(),
        inspect_video(&standalone, 0, video.len() as u64).unwrap()
    );
    for (offset, length) in [
        (video.len() as u64, audio.len() as u64 - 1),
        (video.len() as u64 - 1, audio.len() as u64),
        (video.len() as u64, audio.len() as u64 + 1),
        (u64::MAX, 1),
        (video.len() as u64, u64::MAX),
        (video.len() as u64, 0),
    ] {
        assert!(
            inspect_audio(&file, offset, length).is_err(),
            "{offset}/{length}"
        );
    }
    // The following valid container cannot repair the first one's truncated end.
    assert!(inspect_video(&file, 0, video.len() as u64 - 1).is_err());
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(inspect_audio(&file, video.len() as u64, audio.len() as u64).is_err());
    assert!(inspect_video(&file, 0, video.len() as u64).is_err());
}
