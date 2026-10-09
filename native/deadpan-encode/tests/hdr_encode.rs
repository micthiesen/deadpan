//! Real VideoToolbox HEVC Main10 encodes through the public session, verified
//! independently by walking the emitted MP4 boxes and by the pinned ffprobe.
//! Pixel-accurate decode comparison belongs to the host verifier.

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_encode::probe::{HDR_PROBE_CONTENT_LIGHT, HDR_PROBE_MASTERING, HdrEncoderProbe};
use deadpan_encode::{
    BFramePolicy, ContentLight, EncodeError, EncodeFailureKind, EncodeLimits, EncoderMode,
    EncoderSession, HdrTransfer, NextInput, VideoFormat,
};

struct Encoded {
    bytes: Vec<u8>,
    path: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

fn encode(
    probe: &HdrEncoderProbe,
    mode: EncoderMode,
    b_frames: BFramePolicy,
    light: Option<ContentLight>,
) -> Result<Encoded, EncodeError> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hdr.mp4");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(60);
    let contract = probe.contract(mode, b_frames)?;
    let mut session = EncoderSession::open(
        file,
        contract,
        EncodeLimits::default(),
        &cancelled,
        deadline,
    )?;
    let video = session.video_codec().clone();
    assert_eq!(video.encoder, "hevc_videotoolbox");
    assert_eq!(video.pix_fmt, "p010le");
    assert_eq!(video.codec_tag, "hvc1");
    assert!(!video.mastering_display && !video.content_light);
    assert_eq!(session.info().video_profile, 2);
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
    let mut left = [0.0; 1024];
    let mut right = [0.0; 1024];
    loop {
        match session.next_input()? {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                probe.fill_picture(ordinal, &mut picture).unwrap();
                session.push_picture(ordinal, pts, duration, &picture)?;
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let count = usize::try_from(samples).unwrap();
                probe
                    .fill_audio(first_sample, &mut left[..count], &mut right[..count])
                    .unwrap();
                session.push_audio(first_sample, &left[..count], &right[..count])?;
            }
            NextInput::Finish => break,
        }
    }
    let output = session.finish_with_light(light)?;
    assert_eq!(
        output.video_codec().format,
        match probe.transfer() {
            HdrTransfer::Pq => VideoFormat::HevcMain10Rec2100Pq,
            HdrTransfer::Hlg => VideoFormat::HevcMain10Rec2100Hlg,
        }
    );
    // VideoToolbox's Dolby Vision 8.4 RPUs (HEVC NAL 62) never reach the file.
    eprintln!(
        "{:?} removed unspecified NAL units: {}",
        probe.transfer(),
        output.video_codec().removed_unspecified_nal_units
    );
    let (mut file, report) = output.into_parts();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(u64::try_from(bytes.len()).unwrap(), report.output_bytes);
    assert_eq!(report.video_packets, probe.config().video_frames);
    Ok(Encoded {
        bytes,
        path,
        _directory: directory,
    })
}

fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset + 8 <= data.len() {
        let size = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap());
        let kind: [u8; 4] = data[offset + 4..offset + 8].try_into().unwrap();
        let (header, size) = if size == 1 {
            let large = u64::from_be_bytes(data[offset + 8..offset + 16].try_into().unwrap());
            (16, usize::try_from(large).unwrap())
        } else {
            (8, usize::try_from(size).unwrap())
        };
        assert!(size >= header && offset + size <= data.len(), "box bounds");
        result.push((kind, &data[offset + header..offset + size]));
        offset += size;
    }
    assert_eq!(offset, data.len(), "trailing box bytes");
    result
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .into_iter()
        .find(|(found, _)| found == kind)
        .map(|(_, body)| body)
}

fn be16(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data[at], data[at + 1]])
}

fn be32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().unwrap())
}

struct SampleEntry {
    profile_idc: u8,
    chroma_format: u8,
    luma_bits: u8,
    chroma_bits: u8,
    colr: [u16; 3],
    full_range: bool,
    mdcv: Option<Vec<u8>>,
    clli: Option<[u16; 2]>,
    edit_list: bool,
    ftyp_first_moov_before_mdat: bool,
}

fn inspect(bytes: &[u8]) -> SampleEntry {
    let top = boxes(bytes);
    let kinds: Vec<[u8; 4]> = top.iter().map(|(kind, _)| *kind).collect();
    let position = |kind: &[u8; 4]| kinds.iter().position(|found| found == kind).unwrap();
    let ftyp_first_moov_before_mdat =
        position(b"ftyp") == 0 && position(b"moov") < position(b"mdat");
    let moov = child(bytes, b"moov").unwrap();
    let video = boxes(moov)
        .into_iter()
        .filter(|(kind, _)| kind == b"trak")
        .find(|(_, trak)| {
            let hdlr = child(child(trak, b"mdia").unwrap(), b"hdlr").unwrap();
            &hdlr[8..12] == b"vide"
        })
        .unwrap()
        .1;
    let edit_list = child(video, b"edts").is_some_and(|edts| child(edts, b"elst").is_some());
    let stbl = child(
        child(child(video, b"mdia").unwrap(), b"minf").unwrap(),
        b"stbl",
    )
    .unwrap();
    let stsd = child(stbl, b"stsd").unwrap();
    assert_eq!(be32(stsd, 4), 1);
    let entries = boxes(&stsd[8..]);
    assert_eq!(entries.len(), 1);
    let (kind, entry) = entries[0];
    assert_eq!(&kind, b"hvc1");
    let children = &entry[78..];
    let hvcc = child(children, b"hvcC").unwrap();
    let colr = child(children, b"colr").unwrap();
    assert_eq!(&colr[..4], b"nclx");
    SampleEntry {
        profile_idc: hvcc[1] & 0x1f,
        chroma_format: hvcc[16] & 0x03,
        luma_bits: (hvcc[17] & 0x07) + 8,
        chroma_bits: (hvcc[18] & 0x07) + 8,
        colr: [be16(colr, 4), be16(colr, 6), be16(colr, 8)],
        full_range: colr[10] & 0x80 != 0,
        mdcv: child(children, b"mdcv").map(<[u8]>::to_vec),
        clli: child(children, b"clli").map(|clli| [be16(clli, 0), be16(clli, 2)]),
        edit_list,
        ftyp_first_moov_before_mdat,
    }
}

fn ffprobe(path: &std::path::Path) -> serde_json::Value {
    let prefix = std::env::var("DEADPAN_FFMPEG_PREFIX").expect("pinned FFmpeg prefix");
    let output = Command::new(format!("{prefix}/bin/ffprobe"))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_streams",
            "-show_entries",
            "packet=pts,dts,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn verify(encoded: &Encoded, probe: &HdrEncoderProbe) {
    let entry = inspect(&encoded.bytes);
    assert_eq!(entry.profile_idc, 2, "HEVC Main10");
    assert_eq!(entry.chroma_format, 1);
    assert_eq!((entry.luma_bits, entry.chroma_bits), (10, 10));
    let transfer = match probe.transfer() {
        HdrTransfer::Pq => 16,
        HdrTransfer::Hlg => 18,
    };
    assert_eq!(entry.colr, [9, transfer, 9]);
    assert!(!entry.full_range);
    assert!(entry.edit_list);
    assert!(entry.ftyp_first_moov_before_mdat);
    match probe.transfer() {
        HdrTransfer::Pq => {
            let mdcv = entry.mdcv.expect("PQ mdcv");
            let m = HDR_PROBE_MASTERING;
            // ISO/IEC 23001-8 mdcv order is G, B, R, then white point.
            let expected_xy = [
                m.primaries[1],
                m.primaries[2],
                m.primaries[0],
                m.white_point,
            ];
            for (index, [x, y]) in expected_xy.into_iter().enumerate() {
                assert_eq!(be16(&mdcv, index * 4), x);
                assert_eq!(be16(&mdcv, index * 4 + 2), y);
            }
            assert_eq!(be32(&mdcv, 16), m.max_luminance);
            assert_eq!(be32(&mdcv, 20), m.min_luminance);
            assert_eq!(
                entry.clli,
                Some([
                    HDR_PROBE_CONTENT_LIGHT.max_cll,
                    HDR_PROBE_CONTENT_LIGHT.max_fall
                ])
            );
        }
        HdrTransfer::Hlg => {
            assert!(entry.mdcv.is_none());
            assert!(entry.clli.is_none());
        }
    }
    let probe_json = ffprobe(&encoded.path);
    let stream = &probe_json["streams"][0];
    assert_eq!(stream["codec_name"], "hevc");
    assert_eq!(stream["codec_tag_string"], "hvc1");
    assert_eq!(stream["profile"], "Main 10");
    assert_eq!(stream["pix_fmt"], "yuv420p10le");
    assert_eq!(stream["color_range"], "tv");
    assert_eq!(stream["color_space"], "bt2020nc");
    assert_eq!(stream["color_primaries"], "bt2020");
    // Left (type 0) is explicit or the HEVC 4:2:0 default; OS software PQ
    // was measured declaring topleft, so this is asserted for hardware only.
    assert_eq!(stream["chroma_location"], "left");
    assert_eq!(
        stream["color_transfer"],
        match probe.transfer() {
            HdrTransfer::Pq => "smpte2084",
            HdrTransfer::Hlg => "arib-std-b67",
        }
    );
    let packets = probe_json["packets"].as_array().unwrap();
    assert_eq!(
        u64::try_from(packets.len()).unwrap(),
        probe.config().video_frames
    );
    for packet in packets {
        assert!(packet["pts"].as_i64().unwrap() >= packet["dts"].as_i64().unwrap());
    }
}

#[test]
fn hardware_hevc_main10_pq_and_hlg_emit_tagged_files_with_static_metadata() {
    for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
        let probe = HdrEncoderProbe::new([320, 180], [30, 1], transfer).unwrap();
        for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
            match encode(
                &probe,
                EncoderMode::Hardware,
                b_frames,
                probe.content_light(),
            ) {
                Ok(encoded) => verify(&encoded, &probe),
                // Retained evidence class: the selected path reordered with
                // PTS<DTS and was rejected before muxing, never rewritten.
                Err(error)
                    if b_frames == BFramePolicy::TargetTwo
                        && error.kind() == EncodeFailureKind::VideoTimestampOrder =>
                {
                    eprintln!("{transfer:?} TargetTwo rejected: {error}");
                }
                Err(error) => panic!("{transfer:?} {b_frames:?}: {error}"),
            }
        }
    }
}

#[test]
fn content_light_presence_must_match_the_transfer_and_poisons_on_mismatch() {
    let pq = HdrEncoderProbe::new([64, 64], [30, 1], HdrTransfer::Pq).unwrap();
    let error = encode(&pq, EncoderMode::Hardware, BFramePolicy::None, None)
        .err()
        .unwrap();
    assert_eq!(error.kind(), EncodeFailureKind::Configuration);
    let invalid = ContentLight {
        max_cll: 100,
        max_fall: 101,
    };
    let error = encode(
        &pq,
        EncoderMode::Hardware,
        BFramePolicy::None,
        Some(invalid),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), EncodeFailureKind::Configuration);
    let hlg = HdrEncoderProbe::new([64, 64], [30, 1], HdrTransfer::Hlg).unwrap();
    let error = encode(
        &hlg,
        EncoderMode::Hardware,
        BFramePolicy::None,
        Some(HDR_PROBE_CONTENT_LIGHT),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), EncodeFailureKind::Configuration);
}

#[test]
fn out_of_range_ten_bit_codes_are_rejected_before_native_work() {
    let probe = HdrEncoderProbe::new([64, 64], [30, 1], HdrTransfer::Hlg).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.path().join("reject.mp4"))
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut session = EncoderSession::open(
        file,
        probe
            .contract(EncoderMode::Hardware, BFramePolicy::None)
            .unwrap(),
        EncodeLimits::default(),
        &cancelled,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
    probe.fill_picture(0, &mut picture).unwrap();
    picture[0..2].copy_from_slice(&1023u16.to_le_bytes());
    let error = session.push_picture(0, 0, 1, &picture).unwrap_err();
    assert_eq!(error.kind(), EncodeFailureKind::Input);
    assert!(matches!(
        session.push_picture(0, 0, 1, &picture),
        Err(EncodeError::Poisoned)
    ));
}

#[test]
fn sdr_sessions_keep_h264_declarations_and_reject_content_light() {
    use deadpan_encode::probe::EncoderProbe;
    let probe = EncoderProbe::new([64, 64], [30, 1]).unwrap();
    for light in [None, Some(HDR_PROBE_CONTENT_LIGHT)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sdr.mp4");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut session = EncoderSession::open(
            file,
            // Hardware H.264 TargetTwo is the retained PTS<DTS rejection.
            probe
                .contract(EncoderMode::Hardware, BFramePolicy::None)
                .unwrap(),
            EncodeLimits::default(),
            &cancelled,
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        let video = session.video_codec().clone();
        assert_eq!(video.format, VideoFormat::H264Rec709I420);
        assert_eq!(video.encoder, "h264_videotoolbox");
        assert_eq!(video.pix_fmt, "yuv420p");
        assert_eq!((video.profile, video.codec_tag.as_str()), (100, "avc1"));
        assert_eq!(
            (video.color_primaries, video.color_trc, video.colorspace),
            (1, 1, 1)
        );
        assert_eq!(session.info().abi_version, 2);
        assert_eq!(session.info().video_profile, 100);
        let mut picture = vec![0; usize::try_from(probe.config().picture_bytes).unwrap()];
        let mut left = [0.0; 1024];
        let mut right = [0.0; 1024];
        loop {
            match session.next_input().unwrap() {
                NextInput::Picture {
                    ordinal,
                    pts,
                    duration,
                } => {
                    probe.fill_picture(ordinal, &mut picture).unwrap();
                    session
                        .push_picture(ordinal, pts, duration, &picture)
                        .unwrap();
                }
                NextInput::Audio {
                    first_sample,
                    samples,
                } => {
                    let count = usize::try_from(samples).unwrap();
                    probe
                        .fill_audio(first_sample, &mut left[..count], &mut right[..count])
                        .unwrap();
                    session
                        .push_audio(first_sample, &left[..count], &right[..count])
                        .unwrap();
                }
                NextInput::Finish => break,
            }
        }
        match light {
            Some(light) => assert_eq!(
                session.finish_with_light(Some(light)).err().unwrap().kind(),
                EncodeFailureKind::Configuration
            ),
            None => {
                let output = session.finish().unwrap();
                assert!(!output.video_codec().content_light);
                let stream = &ffprobe(&path)["streams"][0];
                assert_eq!(stream["codec_name"], "h264");
                assert_eq!(stream["codec_tag_string"], "avc1");
                assert_eq!(stream["profile"], "High");
                assert_eq!(stream["color_transfer"], "bt709");
            }
        }
    }
}
