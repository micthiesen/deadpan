//! Private encoded candidate producer. Publication and independent emitted-file
//! verification belong to later host boundaries, after this process is reaped.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use deadpan_core::{AudioSample, ColorPolicy};
use deadpan_encode::{AUDIO_FRAME_SAMPLES, EncodeError, EncodeLimits, EncoderSession, NextInput};
use deadpan_jobs::{CancellationToken, Sha256, WorkspaceArtifact, WorkspaceRef};
use deadpan_render::Yuv420Policy;
use sha2::{Digest, Sha256 as Sha256Hasher};

use crate::audio::OfflineAudioSession;
use crate::export_picture::{ExportPictureContract, ExportPictureSession, OutputFrameOrdinal};
use crate::picture::ProjectPictureSession;
use crate::render_worker::protocol::RenderIdentity;
use crate::render_worker::worker::control::{ControlEnd, ControlPump, ControlReader};
use crate::render_worker::worker::{
    WorkerOutput, check_control as check_worker_control, diagnostic, metal_renderer, open_output,
    progress_due,
};

use super::protocol::{
    EncodedFailure, EncodedFailureKind, EncodedHostMessage, EncodedManifest, EncodedRenderContract,
    EncodedWorkerMessage, MOVIE_REF, PROTOCOL_VERSION, read_host_message, write_worker_message,
};
use super::runtime::{EncodingBinding, RuntimeCapture};

type Result<T> = std::result::Result<T, String>;
type PreparationResult<T> = std::result::Result<T, EncodedFailure>;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const HASH_CHUNK_BYTES: usize = 64 * 1024;
const AUDIO_BLOCK: usize = 1024;

struct Request {
    identity: RenderIdentity,
    contract: EncodedRenderContract,
    binding: Option<EncodingBinding>,
    document_sha256: Sha256,
    limits: EncodeLimits,
}

pub(crate) fn entry(package: &Path) -> ExitCode {
    match run_entry(package) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("render encode worker: {}", diagnostic(&error).as_str());
            ExitCode::FAILURE
        }
    }
}

fn run_entry(package: &Path) -> Result<bool> {
    if !package.is_absolute() {
        return Err("encoded render worker requires an absolute project package path".to_owned());
    }
    let handshake_deadline = Instant::now()
        .checked_add(HANDSHAKE_TIMEOUT)
        .ok_or("encoded render handshake deadline overflow")?;
    let stdin = File::from(rustix::io::dup(io::stdin()).map_err(|error| error.to_string())?);
    let mut reader =
        ControlReader::new(stdin, Arc::new(AtomicBool::new(false)), handshake_deadline)
            .map_err(|error| error.to_string())?;
    let Some(EncodedHostMessage::Prepare {
        identity,
        cancellation_token,
        contract,
        binding,
        document_sha256,
        limits,
        timeout_millis,
        ..
    }) = read_host_message(&mut reader)?
    else {
        return Err("encoded render worker expected one Prepare message".to_owned());
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_millis))
        .ok_or("encoded render request deadline overflow")?;
    let request = Request {
        identity,
        contract: *contract,
        binding: binding.map(|value| *value),
        document_sha256,
        limits,
    };
    let control_identity = request.identity.clone();
    let mut control = ControlPump::start_with(reader, deadline, move |reader| {
        receive_control(reader, &control_identity, &cancellation_token)
    })
    .map_err(|error| error.to_string())?;
    let mut stdout = io::stdout().lock();
    let prepared = prepare(
        package,
        &request,
        control.cancelled(),
        deadline,
        &mut stdout,
    );
    // The nonblocking reader drains available bytes and joins before any
    // terminal claim. A queued cancellation overrides a just-finished movie.
    let terminal = match control.finish() {
        Ok(ControlEnd::Cancelled) => EncodedWorkerMessage::Cancelled {
            protocol: PROTOCOL_VERSION,
            identity: request.identity,
        },
        Err(error) => EncodedWorkerMessage::Failed {
            protocol: PROTOCOL_VERSION,
            identity: request.identity,
            failure: failure(EncodedFailureKind::Control, error),
        },
        Ok(ControlEnd::Stopped) => match prepared {
            Ok(manifest) => EncodedWorkerMessage::Completed {
                protocol: PROTOCOL_VERSION,
                identity: request.identity,
                manifest: Box::new(manifest),
                binding: request.binding.map(Box::new),
            },
            Err(error) => EncodedWorkerMessage::Failed {
                protocol: PROTOCOL_VERSION,
                identity: request.identity,
                failure: error,
            },
        },
    };
    let success = !matches!(terminal, EncodedWorkerMessage::Failed { .. });
    write_worker_message(&mut stdout, &terminal)?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(success)
}

fn receive_control(
    reader: &mut impl Read,
    identity: &RenderIdentity,
    token: &CancellationToken,
) -> Result<ControlEnd> {
    match read_host_message(reader) {
        Ok(Some(EncodedHostMessage::Cancel {
            identity: received,
            cancellation_token,
            ..
        })) if &received == identity && &cancellation_token == token => Ok(ControlEnd::Cancelled),
        Ok(Some(EncodedHostMessage::Cancel { .. })) => {
            Err("encoded render cancellation identity or token does not match".to_owned())
        }
        Ok(Some(EncodedHostMessage::Prepare { .. })) => {
            Err("encoded render worker received a second Prepare message".to_owned())
        }
        Ok(None) => Err("encoded render host closed its control stream".to_owned()),
        Err(error) => Err(format!("invalid encoded render control: {error}")),
    }
}

fn prepare(
    package: &Path,
    request: &Request,
    cancelled: &AtomicBool,
    deadline: Instant,
    stdout: &mut impl Write,
) -> PreparationResult<EncodedManifest> {
    check_control(cancelled, deadline)?;
    request
        .contract
        .validate()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let native_contract = request
        .contract
        .native_contract()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    request
        .limits
        .validate_for(&native_contract)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let mut runtime_capture = check_binding(request, cancelled, deadline)?;
    let picture = &request.contract.picture;
    let pictures = ProjectPictureSession::open_revision(
        package,
        &picture.revision_id,
        Some(picture.range),
        cancelled,
    )
    .map_err(|error| failure(EncodedFailureKind::Source, error))?;
    check_control(cancelled, deadline)?;
    let captured = ExportPictureContract::capture(&pictures)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    if !picture.matches(&captured) {
        return Err(failure(
            EncodedFailureKind::Contract,
            "committed pictures differ from the encoded render contract",
        ));
    }
    let mut audio = OfflineAudioSession::open_revision(
        package,
        &picture.revision_id,
        picture.range,
        cancelled,
        deadline,
    )
    .map_err(|error| failure(EncodedFailureKind::Audio, error))?;
    if audio.project_id() != &picture.project_id
        || audio.revision() != &picture.revision_id
        || audio.range() != picture.range
        || audio.frame_rate() != picture.frame_rate
        || audio.sample_range() != (picture.project_audio_start..picture.project_audio_end)
        || audio.sample_count() != native_contract.audio_samples()
    {
        return Err(failure(
            EncodedFailureKind::Contract,
            "committed audio differs from the encoded render interval",
        ));
    }
    for document in [pictures.document(), audio.document()] {
        let actual = crate::render_worker::document_hash(document, cancelled, deadline)
            .map_err(document_failure)?;
        if actual != request.document_sha256 {
            return Err(failure(
                EncodedFailureKind::Contract,
                "committed document differs from the requested SHA-256",
            ));
        }
    }
    check_control(cancelled, deadline)?;
    // No GPU or output allocation precedes both immutable snapshot bindings.
    let renderer = metal_renderer(cancelled, deadline)
        .map_err(|error| failure(EncodedFailureKind::Picture, error))?;
    let mut pictures = ExportPictureSession::new(pictures, renderer, cancelled, deadline)
        .map_err(|error| failure(EncodedFailureKind::Picture, error))?;
    check_control(cancelled, deadline)?;
    let output = open_output(Path::new("."), WorkerOutput::Movie)
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let mut encoder =
        EncoderSession::open(output, native_contract, request.limits, cancelled, deadline)
            .map_err(encoder_failure)?;
    let audio_total = encoder.contract().audio_samples();
    let total_inputs = captured
        .frame_count()
        .checked_add(audio_total.div_ceil(u64::from(AUDIO_FRAME_SAMPLES)))
        .ok_or_else(|| {
            failure(
                EncodedFailureKind::Contract,
                "encoded render input count overflow",
            )
        })?;
    let mut completed_inputs = 0_u64;
    let mut light = ContentLightAccumulator::default();
    loop {
        check_control(cancelled, deadline)?;
        match encoder.next_input().map_err(encoder_failure)? {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                let output_ordinal = OutputFrameOrdinal(ordinal);
                let frame = pictures
                    .prepare(output_ordinal, cancelled, deadline)
                    .map_err(|error| failure(EncodedFailureKind::Picture, error))?;
                let timing = captured
                    .timing(output_ordinal)
                    .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
                let pixels = frame.pixels();
                if frame.contract() != &captured
                    || frame.timing() != timing
                    || timing.pts() != pts
                    || timing.duration() != duration
                    || [pixels.width(), pixels.height()] != captured.raster()
                    || !pixels_match_policy(pixels, captured.color_policy())
                    || u64::try_from(pixels.bytes().len()).ok()
                        != Some(
                            picture
                                .frame_bytes()
                                .map_err(|error| failure(EncodedFailureKind::Contract, error))?,
                        )
                {
                    return Err(failure(
                        EncodedFailureKind::Contract,
                        "prepared encoder picture changed its captured contract",
                    ));
                }
                if let Some((_, frame_light)) = pixels.hdr() {
                    light.add(frame_light);
                }
                encoder
                    .push_picture(ordinal, pts, duration, pixels.bytes())
                    .map_err(encoder_failure)?;
                drop(frame);
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let start = audio_start(
                    picture.project_audio_start,
                    picture.project_audio_end,
                    first_sample,
                    samples,
                )
                .map_err(|error| failure(EncodedFailureKind::Audio, error))?;
                let block = audio
                    .read(start, samples, cancelled)
                    .map_err(|error| failure(EncodedFailureKind::Audio, error))?;
                if block.project_id != picture.project_id
                    || block.revision_id != picture.revision_id
                    || block.start != start
                {
                    return Err(failure(
                        EncodedFailureKind::Contract,
                        "prepared audio changed its captured project identity",
                    ));
                }
                let planar = PlanarInput::new(&block.samples, samples)
                    .map_err(|error| failure(EncodedFailureKind::Audio, error))?;
                encoder
                    .push_audio(
                        first_sample,
                        &planar.left[..planar.count],
                        &planar.right[..planar.count],
                    )
                    .map_err(encoder_failure)?;
            }
            NextInput::Finish => break,
        }
        completed_inputs = completed_inputs
            .checked_add(1)
            .filter(|count| *count <= total_inputs)
            .ok_or_else(|| {
                failure(
                    EncodedFailureKind::Contract,
                    "encoded render input count differs",
                )
            })?;
        if progress_due(completed_inputs, total_inputs) {
            write_worker_message(
                stdout,
                &EncodedWorkerMessage::Progress {
                    protocol: PROTOCOL_VERSION,
                    identity: request.identity.clone(),
                    completed_frames: encoder.accepted_pictures(),
                    total_frames: captured.frame_count(),
                    completed_audio_samples: encoder.accepted_audio_samples(),
                    total_audio_samples: audio_total,
                },
            )
            .map_err(|error| failure(EncodedFailureKind::Control, error))?;
            stdout
                .flush()
                .map_err(|error| failure(EncodedFailureKind::Control, error))?;
        }
    }
    if completed_inputs != total_inputs {
        return Err(failure(
            EncodedFailureKind::Contract,
            "encoded render ended before every captured input",
        ));
    }
    // PQ outputs carry MaxCLL/MaxFALL measured from these exact pictures,
    // never copied from the source; SDR and HLG outputs carry none.
    let content_light =
        (captured.color_policy() == ColorPolicy::HdrRec2020Pq).then(|| light.finish());
    let (mut output, report) = encoder
        .finish_with_light(content_light)
        .map_err(encoder_failure)?
        .into_parts();
    check_control(cancelled, deadline)?;
    let hash = hash_movie(
        &mut output,
        report.output_bytes,
        request.limits.maximum_output_bytes,
        cancelled,
        deadline,
    )?;
    let movie = WorkspaceArtifact::new(
        WorkspaceRef::new(MOVIE_REF).map_err(|error| failure(EncodedFailureKind::Output, error))?,
        hash,
        report.output_bytes,
    )
    .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let manifest = EncodedManifest {
        contract: request.contract.clone(),
        document_sha256: request.document_sha256.clone(),
        movie,
        report,
    };
    manifest
        .validate_for(request.limits)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    if let Some(capture) = &mut runtime_capture {
        capture
            .revalidate(cancelled, deadline)
            .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    }
    check_control(cancelled, deadline)?;
    Ok(manifest)
}

fn check_binding(
    request: &Request,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> PreparationResult<Option<RuntimeCapture>> {
    if let Some(binding) = &request.binding {
        binding
            .validate_for(&request.contract)
            .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
        let observed = binding
            .check_current(cancelled, deadline)
            .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
        if observed.fingerprint() != &binding.runtime {
            return Err(failure(
                EncodedFailureKind::Contract,
                "encoding runtime differs from the qualified binding",
            ));
        }
        return Ok(Some(observed));
    }
    Ok(None)
}

fn audio_start(
    start: AudioSample,
    end: AudioSample,
    first: u64,
    count: u32,
) -> Result<AudioSample> {
    if count == 0 || count > AUDIO_FRAME_SAMPLES {
        return Err("encoded audio block exceeds the native input bound".to_owned());
    }
    let first = i64::try_from(first).map_err(|_| "encoded audio coordinate overflow")?;
    let absolute = start
        .0
        .checked_add(first)
        .ok_or("encoded audio coordinate overflow")?;
    if start.0 < 0
        || absolute < start.0
        || absolute
            .checked_add(i64::from(count))
            .is_none_or(|value| value > end.0)
    {
        return Err("encoded audio block leaves the captured sample interval".to_owned());
    }
    Ok(AudioSample(absolute))
}

struct PlanarInput {
    left: [f32; AUDIO_BLOCK],
    right: [f32; AUDIO_BLOCK],
    count: usize,
}

impl PlanarInput {
    fn new(samples: &[[f32; 2]], count: u32) -> Result<Self> {
        let count = usize::try_from(count).map_err(|_| "audio block exceeds address space")?;
        if count == 0 || count > AUDIO_BLOCK || samples.len() != count {
            return Err("prepared audio length differs from the next encoder block".to_owned());
        }
        let mut planar = Self {
            left: [0.; AUDIO_BLOCK],
            right: [0.; AUDIO_BLOCK],
            count,
        };
        for (index, [left, right]) in samples.iter().copied().enumerate() {
            if !left.is_finite() || !right.is_finite() {
                return Err("prepared audio contains a nonfinite sample".to_owned());
            }
            planar.left[index] = left;
            planar.right[index] = right;
        }
        Ok(planar)
    }
}

pub(crate) fn hash_movie(
    file: &mut File,
    expected: u64,
    maximum: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> PreparationResult<Sha256> {
    check_control(cancelled, deadline)?;
    let metadata = file
        .metadata()
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    if !metadata.is_file() || metadata.len() != expected || expected == 0 || expected > maximum {
        return Err(failure(
            EncodedFailureKind::Output,
            "encoded movie descriptor length differs from its bounded report",
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let hash = hash_exact(file, expected, cancelled, deadline)?;
    if file
        .metadata()
        .map_err(|error| failure(EncodedFailureKind::Output, error))?
        .len()
        != expected
    {
        return Err(failure(
            EncodedFailureKind::Output,
            "encoded movie changed length while hashing",
        ));
    }
    check_control(cancelled, deadline)?;
    Ok(hash)
}

fn hash_exact(
    reader: &mut impl Read,
    expected: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> PreparationResult<Sha256> {
    let mut buffer = [0_u8; HASH_CHUNK_BYTES];
    let mut length = 0_u64;
    let mut hasher = Sha256Hasher::new();
    loop {
        check_control(cancelled, deadline)?;
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(failure(EncodedFailureKind::Output, error)),
        };
        check_control(cancelled, deadline)?;
        if count == 0 {
            break;
        }
        length = length
            .checked_add(
                u64::try_from(count).map_err(|error| failure(EncodedFailureKind::Output, error))?,
            )
            .filter(|length| *length <= expected)
            .ok_or_else(|| {
                failure(
                    EncodedFailureKind::Output,
                    "encoded movie exceeds its declared length",
                )
            })?;
        hasher.update(&buffer[..count]);
    }
    if length != expected {
        return Err(failure(
            EncodedFailureKind::Output,
            "encoded movie ended before its declared length",
        ));
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Sha256::new(hex).map_err(|error| failure(EncodedFailureKind::Output, error))
}

fn failure(kind: EncodedFailureKind, error: impl std::fmt::Display) -> EncodedFailure {
    EncodedFailure {
        kind,
        diagnostic: diagnostic(&error.to_string()),
    }
}

fn encoder_failure(error: EncodeError) -> EncodedFailure {
    failure(EncodedFailureKind::Encoder(error.kind()), error)
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> PreparationResult<()> {
    check_worker_control(cancelled, deadline)
        .map_err(|error| failure(EncodedFailureKind::Control, error))
}

fn document_failure(error: crate::render_worker::RenderWorkerError) -> EncodedFailure {
    let kind = match &error {
        crate::render_worker::RenderWorkerError::Cancelled
        | crate::render_worker::RenderWorkerError::Deadline => EncodedFailureKind::Control,
        _ => EncodedFailureKind::Contract,
    };
    failure(kind, error)
}

#[cfg(test)]
mod tests;

fn pixels_match_policy(pixels: &crate::export_picture::ExportPixels, policy: ColorPolicy) -> bool {
    use deadpan_render::Yuv420P10Policy;
    match (policy, pixels) {
        (ColorPolicy::SdrRec709, crate::export_picture::ExportPixels::Sdr(pixels)) => {
            pixels.policy() == Yuv420Policy::Rec709LimitedLeft
        }
        (ColorPolicy::HdrRec2020Pq, crate::export_picture::ExportPixels::Hdr { pixels, .. }) => {
            pixels.policy() == Yuv420P10Policy::Rec2100PqLimitedLeft
        }
        (ColorPolicy::HdrRec2020Hlg, crate::export_picture::ExportPixels::Hdr { pixels, .. }) => {
            pixels.policy() == Yuv420P10Policy::Rec2100HlgLimitedLeft
        }
        _ => false,
    }
}

/// CTA-861.3 aggregation: MaxCLL is the brightest pixel's max(R,G,B) and
/// MaxFALL the brightest frame-average of max(R,G,B), both in cd/m², over
/// the clipped linear light actually coded. Values round up to whole cd/m².
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ContentLightAccumulator {
    max_cll: f64,
    max_fall: f64,
}

impl ContentLightAccumulator {
    pub(crate) fn add(&mut self, light: deadpan_render::FrameLight) {
        if light.max_nits.is_finite() {
            self.max_cll = self.max_cll.max(light.max_nits);
        }
        if light.mean_nits.is_finite() {
            self.max_fall = self.max_fall.max(light.mean_nits);
        }
    }

    pub(crate) fn finish(self) -> deadpan_encode::ContentLight {
        let whole = |value: f64| value.ceil().clamp(0.0, 10_000.0) as u16;
        let max_cll = whole(self.max_cll);
        deadpan_encode::ContentLight {
            max_cll,
            max_fall: whole(self.max_fall).min(max_cll),
        }
    }
}
