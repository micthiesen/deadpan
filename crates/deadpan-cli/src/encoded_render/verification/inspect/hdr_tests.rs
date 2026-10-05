//! Real HEVC Main10 PQ/HLG files encoded through the native session, then
//! inspected by the same complete verifier used for finished candidates.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId,
    RevisionId,
};
use deadpan_encode::{
    BFramePolicy, ContentLight, EncodeLimits, EncoderMode, EncoderSession, HdrTransfer, NextInput,
    probe::HdrEncoderProbe,
};
use deadpan_jobs::{Sha256, WorkspaceArtifact, WorkspaceRef};
use deadpan_source::{DecodeControl, Mp4PacketReader};
use sha2::Digest;

use super::{
    Context, container,
    light::{self, LightMeter},
};
use crate::{
    encoded_render::{
        protocol::{EncodedManifest, EncodedRenderContract, EncoderChoice, MOVIE_REF},
        verification::{ContentLightEvidence, VerificationLimits, VerificationReport},
    },
    render_worker::protocol::{RenderContract, RenderTimeBase},
};

static NOT_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Synthetic picture content for one encode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Content {
    /// Exact `HdrEncoderProbe` pictures; host light is metered on those codes.
    Probe,
    /// Linear working pictures converted by the shared renderer boundary,
    /// whose own CTA-861.3 statistics are the host declaration. Contains fine
    /// detail, isolated 4000 cd/m² 2x2 highlights (one moving), saturated
    /// colors and noise; MaxCLL is set by the small highlights.
    Stress,
    /// As `Stress` without the highlights: MaxCLL (1000 cd/m²) is shared by
    /// saturated primaries with hard edges, a ramp and line detail, so chroma
    /// reconstruction at saturated edges decides the decoded maximum.
    Edges,
}

pub(super) struct Encoded {
    _directory: tempfile::TempDir,
    pub path: PathBuf,
    pub manifest: EncodedManifest,
    /// What the host would declare: whole cd/m², ceil, FALL <= CLL.
    pub host_light: Option<ContentLight>,
    /// Unrounded host statistics in cd/m².
    pub host_nits: [f64; 2],
}

pub(super) fn contract(probe: &HdrEncoderProbe, choice: EncoderChoice) -> EncodedRenderContract {
    let config = probe.config();
    let frames = i64::try_from(config.video_frames).unwrap();
    let contract = EncodedRenderContract {
        picture: RenderContract {
            project_id: ProjectId::new("hdr-verification-test").unwrap(),
            revision_id: RevisionId::new("hdr-verification-revision").unwrap(),
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(frames)).unwrap(),
            canvas: config.raster,
            raster: config.raster,
            frame_rate: FrameRate::new(config.frame_rate[0], config.frame_rate[1]).unwrap(),
            color_policy: match probe.transfer() {
                HdrTransfer::Pq => ColorPolicy::HdrRec2020Pq,
                HdrTransfer::Hlg => ColorPolicy::HdrRec2020Hlg,
            },
            time_base: RenderTimeBase {
                numerator: 1,
                denominator: config.frame_rate[0],
            },
            frame_count: config.video_frames,
            terminal_pts: frames * i64::from(config.frame_rate[1]),
            project_audio_start: AudioSample(0),
            project_audio_end: AudioSample(i64::try_from(config.audio_samples).unwrap()),
            relative_aspect_error: ExactRatio::ZERO,
            mastering_display: probe.signal().mastering.map(|volume| {
                deadpan_core::MasteringDisplay {
                    primaries: volume.primaries,
                    white_point: volume.white_point,
                    max_luminance: volume.max_luminance,
                    min_luminance: volume.min_luminance,
                }
            }),
        },
        choice,
    };
    assert_eq!(
        contract.native_contract().unwrap(),
        probe.contract(choice.mode, choice.b_frames).unwrap()
    );
    contract
}

fn half(value: f32) -> [u8; 2] {
    // Round-to-nearest binary16 for finite nonnegative working values.
    let bits = value.to_bits();
    let exponent = i32::try_from((bits >> 23) & 0xff).unwrap() - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    let half = if value == 0.0 || exponent <= 0 {
        0_u16
    } else {
        let rounded = (u32::try_from(exponent).unwrap() << 10) + ((mantissa + 0x1000) >> 13);
        u16::try_from(rounded).unwrap()
    };
    half.to_le_bytes()
}

/// Deterministic stress working light (cd/m² / 203) at one pixel.
fn stress_light(ordinal: u64, x: u32, y: u32, size: [u32; 2], spots: bool) -> [f32; 3] {
    let [width, height] = size;
    let nits = |value: f64| (value / 203.0) as f32;
    let hash = {
        let mut value =
            (u64::from(x) * 0x9e37_79b9) ^ (u64::from(y) * 0x85eb_ca6b) ^ (ordinal * 0xc2b2_ae35);
        value ^= value >> 15;
        value = value.wrapping_mul(0x2c1b_3c6d);
        value ^= value >> 12;
        value
    };
    let frame = u32::try_from(ordinal).unwrap();
    match y * 6 / height {
        // One-pixel vertical lines alternating 0 and 1000 cd/m².
        0 => {
            [nits(if (x + frame).is_multiple_of(2) {
                1_000.0
            } else {
                0.0
            }); 3]
        }
        // Isolated 2x2 highlights on a dark (1 cd/m²) field; one moves.
        1 => {
            let moving = frame * 7 % width;
            let spot = (x % 64 < 2 && y % 16 < 2) || (x.abs_diff(moving) < 2 && y % 16 < 2);
            // Off-grid single saturated red pixels: chroma subsampling hides
            // most of their light from any decoded measurement.
            if spots && x % 64 == 33 && y % 16 == 5 {
                return [nits(4_000.0), 0.0, 0.0];
            }
            [nits(if spot && spots { 4_000.0 } else { 1.0 }); 3]
        }
        // Saturated primaries and secondaries at 1000 cd/m² with hard edges.
        2 => {
            let full = nits(1_000.0);
            match (x / 24 + frame) % 6 {
                0 => [full, 0.0, 0.0],
                1 => [0.0, 0.0, full],
                2 => [0.0, full, 0.0],
                3 => [full, 0.0, full],
                4 => [0.0, full, full],
                _ => [full, full, 0.0],
            }
        }
        // Per-pixel colored noise up to 1000 cd/m².
        3 => [
            nits((hash % 1_000) as f64),
            nits(((hash >> 16) % 1_000) as f64),
            nits(((hash >> 32) % 1_000) as f64),
        ],
        // Horizontal luminance ramp to 1000 cd/m².
        4 => [nits(1_000.0 * f64::from(x) / f64::from(width - 1)); 3],
        // 203 cd/m² reference white with a soft 600 cd/m² gradient.
        _ => [nits(203.0 + 400.0 * f64::from(y % 64) / 63.0); 3],
    }
}

fn stress_picture(
    ordinal: u64,
    size: [u32; 2],
    spots: bool,
) -> (Vec<u8>, deadpan_render::FrameLight) {
    let [width, height] = size;
    let stride = width * 8;
    let mut bytes = Vec::with_capacity(usize::try_from(stride * height).unwrap());
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = stress_light(ordinal, x, y, size, spots);
            for channel in [r, g, b, 1.0] {
                bytes.extend_from_slice(&half(channel));
            }
        }
    }
    let working = deadpan_render::WorkingRgba16Frame::new(width, height, stride, bytes).unwrap();
    let (frame, light) = deadpan_render::Rec2100Yuv420P10Frame::from_working(
        &working,
        deadpan_render::HdrTransfer::Pq,
    )
    .unwrap();
    (frame.bytes().to_vec(), light)
}

pub(super) fn encode(
    transfer: HdrTransfer,
    raster: [u32; 2],
    choice: EncoderChoice,
    content: Content,
    declared: Option<Option<ContentLight>>,
) -> Encoded {
    let probe = HdrEncoderProbe::new(raster, [30, 1], transfer).unwrap();
    let contract = contract(&probe, choice);
    let native = contract.native_contract().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("movie.mp4");
    let output = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut session = EncoderSession::open(
        output,
        native.clone(),
        EncodeLimits::default(),
        &NOT_CANCELLED,
        deadline,
    )
    .unwrap();
    let mut picture = vec![0_u8; usize::try_from(native.picture_bytes()).unwrap()];
    let mut meter = PixelLight::new(raster[0], false);
    let mut host = [0.0_f64; 2];
    let (mut left, mut right) = (vec![0.0_f32; 1024], vec![0.0_f32; 1024]);
    loop {
        match session.next_input().unwrap() {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                match content {
                    Content::Probe => {
                        probe.fill_picture(ordinal, &mut picture).unwrap();
                        let samples: Vec<u16> = picture
                            .chunks_exact(2)
                            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                            .collect();
                        meter.add(raster[0], raster[1], &samples);
                        host = meter.light;
                    }
                    Content::Stress | Content::Edges => {
                        let (bytes, light) =
                            stress_picture(ordinal, raster, content == Content::Stress);
                        picture.copy_from_slice(&bytes);
                        host = [host[0].max(light.max_nits), host[1].max(light.mean_nits)];
                    }
                }
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
    // Same rounding as the encoded worker's ContentLightAccumulator.
    let whole = |value: f64| value.ceil().clamp(0.0, 10_000.0) as u16;
    let host_light = (transfer == HdrTransfer::Pq).then(|| ContentLight {
        max_cll: whole(host[0]),
        max_fall: whole(host[1]).min(whole(host[0])),
    });
    let light = declared.unwrap_or(host_light);
    let (mut file, report) = session.finish_with_light(light).unwrap().into_parts();
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    let manifest = EncodedManifest {
        contract,
        document_sha256: digest(b"hdr-verification-document"),
        movie: WorkspaceArtifact::new(
            WorkspaceRef::new(MOVIE_REF).unwrap(),
            digest(&bytes),
            u64::try_from(bytes.len()).unwrap(),
        )
        .unwrap(),
        report,
    };
    manifest.validate().unwrap();
    if let Some(keep) = std::env::var_os("DEADPAN_HDR_VERIFY_KEEP") {
        let name = format!(
            "{transfer:?}-{}x{}-{:?}-{:?}-{content:?}.mp4",
            raster[0], raster[1], choice.mode, choice.b_frames
        )
        .to_lowercase();
        std::fs::write(PathBuf::from(keep).join(name), &bytes).unwrap();
    }
    Encoded {
        _directory: directory,
        path,
        manifest,
        host_light,
        host_nits: host,
    }
}

fn digest(bytes: &[u8]) -> Sha256 {
    Sha256::new(
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

pub(super) fn inspect(encoded: &Encoded) -> Result<VerificationReport, String> {
    let file = File::open(&encoded.path).unwrap();
    super::inspect(
        &file,
        &encoded.manifest,
        VerificationLimits::default(),
        &NOT_CANCELLED,
        Instant::now() + Duration::from_secs(600),
        |_| Ok(()),
    )
}

/// Rewrite movie bytes and rebind the manifest hash, as a different encoder
/// that emitted otherwise identical files would.
fn patched(encoded: &Encoded, patch: impl FnOnce(&mut Vec<u8>)) -> Encoded {
    let mut bytes = std::fs::read(&encoded.path).unwrap();
    patch(&mut bytes);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("movie.mp4");
    File::create(&path).unwrap().write_all(&bytes).unwrap();
    let mut manifest = encoded.manifest.clone();
    manifest.movie = WorkspaceArtifact::new(
        WorkspaceRef::new(MOVIE_REF).unwrap(),
        digest(&bytes),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap();
    Encoded {
        _directory: directory,
        path,
        manifest,
        host_light: encoded.host_light,
        host_nits: encoded.host_nits,
    }
}

/// Rewrite video NAL units in place. `rewrite` receives the decode-order
/// packet index and each NAL unit's bytes (header first).
fn rewrite_video_nals(encoded: &Encoded, mut rewrite: impl FnMut(usize, &mut [u8])) -> Encoded {
    let file = File::open(&encoded.path).unwrap();
    let mut reader = Mp4PacketReader::open(
        file,
        deadpan_source::DecodeLimits::default(),
        DecodeControl {
            cancelled: &NOT_CANCELLED,
            timeout: Duration::from_secs(60),
        },
    )
    .unwrap();
    let video = reader
        .inspection()
        .tracks
        .iter()
        .find(|track| track.kind == deadpan_source::Mp4TrackKind::Video)
        .unwrap()
        .clone();
    let length_bytes = usize::from(video.hevc.unwrap().nal_length_bytes);
    let mut spans = Vec::new();
    while let Some(packet) = reader
        .next_packet(DecodeControl {
            cancelled: &NOT_CANCELLED,
            timeout: Duration::from_secs(60),
        })
        .unwrap()
    {
        if packet.track_index == video.index {
            spans.push((
                usize::try_from(packet.offset).unwrap(),
                usize::try_from(packet.length).unwrap(),
            ));
        }
    }
    patched(encoded, |bytes| {
        for (index, (offset, length)) in spans.into_iter().enumerate() {
            let mut position = offset;
            while position < offset + length {
                let amount = bytes[position..position + length_bytes]
                    .iter()
                    .fold(0_usize, |value, byte| (value << 8) | usize::from(*byte));
                let nal = position + length_bytes;
                rewrite(index, &mut bytes[nal..nal + amount]);
                position = nal + amount;
            }
        }
    })
}

/// VideoToolbox inserts a Dolby Vision RPU (HEVC NAL type 62) into every HLG
/// access unit (measured 2026-10-05, hardware and software); the encoder now
/// strips it. Source admission and this verifier refuse it. The returned file
/// overwrites each RPU in place with an equally long filler-data NAL (type
/// 38) so the actual HLG pictures, boxes and tables can be verified. Returns
/// the RPU count.
fn without_dolby_vision(encoded: &Encoded) -> (Encoded, usize) {
    let mut count = 0;
    let stripped = rewrite_video_nals(encoded, |_, nal| {
        if (nal[0] >> 1) & 63 == 62 {
            let amount = nal.len();
            assert!(amount >= 3);
            nal[0] = 38 << 1;
            nal[1] = 1;
            nal[2..amount - 1].fill(0xff);
            nal[amount - 1] = 0x80;
            count += 1;
        }
    });
    (stripped, count)
}

/// Inspect an HLG file, first proving that an emitted Dolby Vision RPU is
/// refused, then verifying the same pictures without it.
fn inspect_hlg(encoded: &Encoded) -> Result<VerificationReport, String> {
    let (stripped, count) = without_dolby_vision(encoded);
    if count > 0 {
        let error = inspect(encoded).unwrap_err();
        assert!(error.contains("Dolby Vision"), "{error}");
    }
    inspect(&stripped)
}

fn find(bytes: &[u8], kind: &[u8; 4]) -> usize {
    let positions: Vec<usize> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, window)| window == kind)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(positions.len(), 1, "{}", String::from_utf8_lossy(kind));
    positions[0] + 4
}

fn hardware(b_frames: BFramePolicy) -> EncoderChoice {
    EncoderChoice {
        mode: EncoderMode::Hardware,
        b_frames,
    }
}

const RASTER: [u32; 2] = [640, 360];

#[test]
fn hdr_pq_and_hlg_main10_files_pass_complete_inspection() {
    for transfer in [HdrTransfer::Pq, HdrTransfer::Hlg] {
        for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
            let encoded = encode(transfer, RASTER, hardware(b_frames), Content::Probe, None);
            let report = match transfer {
                HdrTransfer::Pq => inspect(&encoded),
                HdrTransfer::Hlg => inspect_hlg(&encoded),
            }
            .unwrap_or_else(|error| panic!("{transfer:?} {b_frames:?}: {error}"));
            report.validate(VerificationLimits::default()).unwrap();
            assert_eq!(report.fresh_gop_frames, report.video_frames);
            assert_eq!(
                report.maximum_b_run > 0,
                b_frames == BFramePolicy::TargetTwo,
                "{transfer:?}"
            );
            match transfer {
                HdrTransfer::Pq => {
                    let light = report.content_light.expect("PQ content light evidence");
                    let declared = encoded.host_light.unwrap();
                    assert_eq!(
                        [light.declared_max_cll, light.declared_max_fall],
                        [declared.max_cll, declared.max_fall]
                    );
                }
                HdrTransfer::Hlg => assert!(report.content_light.is_none()),
            }
            // HDR evidence does not leak into another policy's report shape.
            let json = serde_json::to_value(&report).unwrap();
            assert_eq!(
                json.get("content_light").is_some(),
                transfer == HdrTransfer::Pq
            );
        }
    }
}

#[test]
fn hdr_container_rejects_wrong_codec_color_and_static_metadata() {
    let encoded = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    let file = File::open(&encoded.path).unwrap();
    let context = Context {
        file: &file,
        manifest: &encoded.manifest,
        limits: deadpan_source::DecodeLimits::default(),
        cancelled: &NOT_CANCELLED,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let reader = Mp4PacketReader::open(
        context.file().unwrap(),
        context.limits,
        DecodeControl {
            cancelled: &NOT_CANCELLED,
            timeout: Duration::from_secs(60),
        },
    )
    .unwrap();
    let movie = reader.inspection().clone();
    container(&movie, &encoded.manifest).unwrap();
    let video = movie
        .tracks
        .iter()
        .position(|track| track.kind == deadpan_source::Mp4TrackKind::Video)
        .unwrap();
    type Mutation = (&'static str, fn(&mut deadpan_source::Mp4TrackInspection));
    let mutations: [Mutation; 9] = [
        ("missing clli", |t| t.content_light = None),
        ("missing mdcv", |t| t.mastering = None),
        ("mdcv luminance", |t| {
            t.mastering.as_mut().unwrap().max_luminance -= 1;
        }),
        ("colr transfer", |t| t.color.as_mut().unwrap().transfer = 18),
        ("colr primaries", |t| {
            t.color.as_mut().unwrap().primaries = 1
        }),
        ("colr full range", |t| {
            let color = t.color.as_mut().unwrap();
            color.full_range = true;
            color.range_byte = 0x80;
        }),
        ("main profile", |t| t.hevc.as_mut().unwrap().profile_idc = 1),
        ("8-bit chroma", |t| {
            t.hevc.as_mut().unwrap().bit_depth_chroma = 8
        }),
        ("FALL above CLL", |t| {
            let light = t.content_light.as_mut().unwrap();
            light.max_fall = light.max_cll + 1;
        }),
    ];
    for (name, mutate) in mutations {
        let mut changed = movie.clone();
        mutate(&mut changed.tracks[video]);
        assert!(
            container(&changed, &encoded.manifest).is_err(),
            "{name} was admitted"
        );
    }
    // The same file under an HLG contract: transfer and clli both contradict.
    let hlg = encode(
        HdrTransfer::Hlg,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    assert!(container(&movie, &hlg.manifest).is_err());
}

#[test]
fn hdr_files_with_wrong_boxes_fail_full_verification() {
    let encoded = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    inspect(&encoded).unwrap();
    // colr nclx transfer 16 (PQ) -> 18 (HLG); the bitstream VUI still says PQ.
    let colr = patched(&encoded, |bytes| {
        let at = find(bytes, b"colr") + 4;
        assert_eq!(&bytes[at..at + 6], &[0, 9, 0, 16, 0, 9]);
        bytes[at + 3] = 18;
    });
    let error = inspect(&colr).unwrap_err();
    assert!(error.contains("sample description"), "{error}");
    // mdcv maximum luminance differs from the contract volume.
    let mdcv = patched(&encoded, |bytes| {
        let at = find(bytes, b"mdcv") + 16;
        bytes[at + 3] ^= 1;
    });
    assert!(inspect(&mdcv).unwrap_err().contains("sample description"));
    // clli declares 100 cd/m² for pictures that reach 1000.
    let clli = patched(&encoded, |bytes| {
        let at = find(bytes, b"clli");
        bytes[at..at + 4].copy_from_slice(&[0, 100, 0, 50]);
    });
    let error = inspect(&clli).unwrap_err();
    assert!(error.contains("declared clli"), "{error}");
    // A missing clli box: rename it to an ignored free-space box.
    let free = patched(&encoded, |bytes| {
        let at = find(bytes, b"clli");
        bytes[at - 4..at].copy_from_slice(b"free");
    });
    assert!(inspect(&free).is_err());
}

#[test]
fn pq_content_light_below_decoded_pictures_is_rejected() {
    let truthful = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    let host = truthful.host_light.unwrap();
    assert!(host.max_cll >= 1_000 && host.max_fall >= 100, "{host:?}");
    // Overstatement cannot be disproved from 4:2:0 pictures: the fixed probe
    // declaration and a 10000 cd/m² claim both remain admissible.
    for declared in [
        deadpan_encode::probe::HDR_PROBE_CONTENT_LIGHT,
        ContentLight {
            max_cll: 10_000,
            max_fall: 10_000,
        },
    ] {
        let encoded = encode(
            HdrTransfer::Pq,
            RASTER,
            hardware(BFramePolicy::None),
            Content::Probe,
            Some(Some(declared)),
        );
        inspect(&encoded).unwrap_or_else(|error| panic!("{declared:?}: {error}"));
    }
    for declared in [
        ContentLight {
            max_cll: host.max_cll / 2,
            max_fall: host.max_fall,
        },
        ContentLight {
            max_cll: host.max_cll,
            max_fall: host.max_fall / 2,
        },
        // CTA-861.3 "unknown" is not admitted for a measured output.
        ContentLight {
            max_cll: 0,
            max_fall: 0,
        },
    ] {
        let encoded = encode(
            HdrTransfer::Pq,
            RASTER,
            hardware(BFramePolicy::None),
            Content::Probe,
            Some(Some(declared)),
        );
        let error = inspect(&encoded).unwrap_err();
        assert!(error.contains("declared clli"), "{declared:?}: {error}");
    }
}

#[test]
fn os_software_pq_topleft_chroma_siting_is_rejected() {
    // Measured 2026-10-05: the OS software HEVC encoder declares top-left
    // chroma siting for PQ (not HLG), contradicting the left-sited input.
    let software = EncoderChoice {
        mode: EncoderMode::Software,
        b_frames: BFramePolicy::None,
    };
    let pq = encode(HdrTransfer::Pq, RASTER, software, Content::Probe, None);
    let error = inspect(&pq).unwrap_err();
    assert!(error.contains("chroma_location: TopLeft"), "{error}");
    let hlg = encode(HdrTransfer::Hlg, RASTER, software, Content::Probe, None);
    inspect_hlg(&hlg).unwrap();
}

#[test]
fn report_content_light_is_required_for_pq_only_and_rechecked() {
    let encoded = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    let report = inspect(&encoded).unwrap();
    let limits = VerificationLimits::default();
    let mut missing = report.clone();
    missing.content_light = None;
    assert!(missing.validate(limits).is_err());
    let mut far = report.clone();
    let light = far.content_light.as_mut().unwrap();
    light.declared_max_cll = 10;
    light.declared_max_fall = 10;
    assert!(far.validate(limits).is_err());
    let hlg = inspect_hlg(&encode(
        HdrTransfer::Hlg,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    ))
    .unwrap();
    let mut extra = hlg.clone();
    extra.content_light = report.content_light;
    assert!(extra.validate(limits).is_err());
    let json = serde_json::to_string(&report).unwrap();
    assert_eq!(
        serde_json::from_str::<VerificationReport>(&json).unwrap(),
        report
    );
    assert!(ContentLightEvidence::pq_code_shortfall(1_000.0, 1_000.0).abs() < 1e-9);
}

#[test]
fn hevc_nal_types_62_and_63_are_rejected_in_every_packet() {
    let encoded = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::TargetTwo),
        Content::Probe,
        None,
    );
    inspect(&encoded).unwrap();
    // The emitted file carries no 62/63 NAL, so nothing was stripped.
    assert_eq!(without_dolby_vision(&encoded).1, 0);
    let frames = usize::try_from(encoded.manifest.contract.picture.frame_count).unwrap();
    // The opening IDR, an inter picture and the final packet, as an RPU-like
    // type 62 and as type 63; the temporal ID byte is left intact.
    for (target, kind) in [(0, 62_u8), (1, 63), (frames - 1, 62), (frames / 2, 63)] {
        let mut rewritten = 0;
        let changed = rewrite_video_nals(&encoded, |packet, nal| {
            if packet == target && rewritten == 0 {
                nal[0] = kind << 1;
                rewritten += 1;
            }
        });
        assert_eq!(rewritten, 1);
        let error = inspect(&changed).unwrap_err();
        assert!(
            error.contains("NAL type 62/63"),
            "packet {target} type {kind}: {error}"
        );
    }
}

#[test]
fn hlg_container_rejects_static_pq_metadata_boxes() {
    let encoded = encode(
        HdrTransfer::Hlg,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    let file = File::open(&encoded.path).unwrap();
    let reader = Mp4PacketReader::open(
        file,
        deadpan_source::DecodeLimits::default(),
        DecodeControl {
            cancelled: &NOT_CANCELLED,
            timeout: Duration::from_secs(60),
        },
    )
    .unwrap();
    let movie = reader.inspection().clone();
    let video = movie
        .tracks
        .iter()
        .position(|track| track.kind == deadpan_source::Mp4TrackKind::Video)
        .unwrap();
    assert!(movie.tracks[video].mastering.is_none());
    assert!(movie.tracks[video].content_light.is_none());
    container(&movie, &encoded.manifest).unwrap();
    let pq = encode(
        HdrTransfer::Pq,
        RASTER,
        hardware(BFramePolicy::None),
        Content::Probe,
        None,
    );
    let pq_track = File::open(&pq.path)
        .map(|file| {
            Mp4PacketReader::open(
                file,
                deadpan_source::DecodeLimits::default(),
                DecodeControl {
                    cancelled: &NOT_CANCELLED,
                    timeout: Duration::from_secs(60),
                },
            )
            .unwrap()
            .inspection()
            .tracks
            .iter()
            .find(|track| track.kind == deadpan_source::Mp4TrackKind::Video)
            .unwrap()
            .clone()
        })
        .unwrap();
    let (mastering, light) = (pq_track.mastering.unwrap(), pq_track.content_light.unwrap());
    let mut with_clli = movie.clone();
    with_clli.tracks[video].content_light = Some(light);
    assert!(
        container(&with_clli, &encoded.manifest).is_err(),
        "HLG clli"
    );
    let mut with_mdcv = movie.clone();
    with_mdcv.tracks[video].mastering = Some(mastering);
    assert!(
        container(&with_mdcv, &encoded.manifest).is_err(),
        "HLG mdcv"
    );
}

/// Measurement used for the documented tolerances. Run with
/// `cargo test -p deadpan-cli --locked --release hdr_light_measurement -- --ignored --nocapture`.
#[test]
#[ignore = "qualification measurement; prints host and decoded content light"]
fn hdr_light_measurement() {
    for raster in [[1920, 1080], [3840, 2160]] {
        for content in [Content::Probe, Content::Stress, Content::Edges] {
            for mode in [EncoderMode::Hardware, EncoderMode::Software] {
                for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
                    let encoded = encode(
                        HdrTransfer::Pq,
                        raster,
                        EncoderChoice { mode, b_frames },
                        content,
                        None,
                    );
                    let decoded = measure(&encoded);
                    let declared = encoded.host_light.unwrap();
                    // Positive: the decoded site bound lies above the declaration.
                    let codes_of = |value: f64, declared: u16| {
                        ContentLightEvidence::pq_code_shortfall(f64::from(declared), value)
                    };
                    let codes = |index: usize, declared: u16| {
                        deadpan_render::pq_inverse_eotf(decoded[index]) * 876.0
                            - deadpan_render::pq_inverse_eotf(f64::from(declared)) * 876.0
                    };
                    println!(
                        "{raster:?} {content:?} {mode:?} {b_frames:?} bytes={} host={:.3}/{:.3} declared={}/{} sites={:.3}/{:.3} ({:+.3}/{:+.3} codes) nearest={:.3}/{:.3} bilinear={:.3}/{:.3} quantile_codes(q,0.99,0.999,0.9999,1)={} verify={}",
                        encoded.manifest.movie.byte_length(),
                        encoded.host_nits[0],
                        encoded.host_nits[1],
                        declared.max_cll,
                        declared.max_fall,
                        decoded[0],
                        decoded[1],
                        codes(0, declared.max_cll),
                        codes(1, declared.max_fall),
                        decoded[2],
                        decoded[3],
                        decoded[4],
                        decoded[5],
                        decoded[6..]
                            .iter()
                            .map(|value| format!("{:+.1}", codes_of(*value, declared.max_cll)))
                            .collect::<Vec<_>>()
                            .join(","),
                        inspect(&encoded).map_or_else(|error| error, |_| "pass".into()),
                    );
                }
            }
        }
    }
}

/// Per-pixel CTA-861.3 light of a 4:2:0 picture under one chroma
/// reconstruction: each chroma sample replicated over its 2x2 block, or
/// bilinear at the left siting. Measurement only; see `light` for why the
/// verifier does not use a per-pixel maximum of a reconstruction.
struct PixelLight {
    meter: LightMeter,
    bilinear: bool,
    light: [f64; 2],
}

impl PixelLight {
    fn new(width: u32, bilinear: bool) -> Self {
        Self {
            meter: LightMeter::new(width).unwrap(),
            bilinear,
            light: [0.0; 2],
        }
    }

    fn add(&mut self, width: u32, height: u32, samples: &[u16]) {
        let (width, height) = (width as usize, height as usize);
        let (columns, rows) = (width / 2, height / 2);
        let (y_plane, chroma) = samples.split_at(width * height);
        let (cb_plane, cr_plane) = chroma.split_at(columns * rows);
        let at =
            |plane: &[u16], row: usize, column: usize| light::chroma(plane[row * columns + column]);
        let (mut maximum, mut total) = (0.0_f64, 0.0_f64);
        for y in 0..height {
            let near = y / 2;
            let far = if !self.bilinear {
                near
            } else if y % 2 == 0 {
                near.saturating_sub(1)
            } else {
                (near + 1).min(rows - 1)
            };
            for x in 0..width {
                let vertical = |plane: &[u16], column: usize| {
                    0.75 * at(plane, near, column) + 0.25 * at(plane, far, column)
                };
                let sample = |plane: &[u16]| {
                    if x % 2 == 0 || !self.bilinear {
                        vertical(plane, x / 2)
                    } else {
                        (vertical(plane, x / 2) + vertical(plane, (x / 2 + 1).min(columns - 1)))
                            / 2.0
                    }
                };
                let nits = self.meter.nits(light::brightest(
                    light::luma(y_plane[y * width + x]),
                    sample(cb_plane),
                    sample(cr_plane),
                ));
                maximum = maximum.max(nits);
                total += nits;
            }
        }
        self.light = [
            self.light[0].max(maximum),
            self.light[1].max(total / (width * height) as f64),
        ];
    }
}

/// Decoded light independent of the verifier's checks (topleft files
/// included): chroma-site bound, nearest and bilinear per-pixel MaxCLL/MaxFALL.
fn measure(encoded: &Encoded) -> Vec<f64> {
    let file = File::open(&encoded.path).unwrap();
    let control = DecodeControl {
        cancelled: &NOT_CANCELLED,
        timeout: Duration::from_secs(60),
    };
    let mut decoder = deadpan_source::SourceDecoder::open(
        file,
        deadpan_source::DecodeLimits {
            max_pixels: 3840 * 2176,
            ..deadpan_source::DecodeLimits::default()
        },
        control,
    )
    .unwrap();
    let width = decoder.info().width;
    let mut sites = LightMeter::with_quantiles(width, &QUANTILES).unwrap();
    let mut nearest = PixelLight::new(width, false);
    let mut bilinear = PixelLight::new(width, true);
    while let Some(frame) = decoder.next_yuv420p10(control).unwrap() {
        sites
            .add(frame.width, frame.height, &frame.samples)
            .unwrap();
        nearest.add(frame.width, frame.height, &frame.samples);
        bilinear.add(frame.width, frame.height, &frame.samples);
    }
    let mut values = vec![
        sites.light().max_cll,
        sites.light().max_fall,
        nearest.light[0],
        nearest.light[1],
        bilinear.light[0],
        bilinear.light[1],
    ];
    values.extend_from_slice(sites.quantile_light());
    values
}

/// Production quantile first, then the measured alternatives.
const QUANTILES: [f64; 5] = [light::SITE_QUANTILE, 0.99, 0.999, 0.9999, 1.0];
