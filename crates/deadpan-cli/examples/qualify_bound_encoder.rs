//! Qualify a fresh encoder, consume it for one committed revision and verify it.
//! Usage: qualify_bound_encoder PACKAGE REVISION WORKER REPORT
//! Outputs are new private /tmp files. This never writes the project or publishes.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedRenderError, EncodedWorkerLimits,
        admission::{AdmissionLimits, AdmissionRequest, qualify},
        protocol::EncodedRenderContract,
        runtime::{EncodingBinding, ResolvedSdrSettings},
        verification::{VerificationLimits, VerificationRequest, verify},
    },
    export_picture::ExportPictureContract,
    picture::ProjectPictureSession,
    render_worker::{RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity},
};
use deadpan_core::RevisionId;
use deadpan_encode::{EncodeLimits, SDR_POLICY_VERSION_V1};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[path = "qualify_render_workflow/reference.rs"]
mod reference;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAXIMUM_BYTES: u64 = 512 * 1024 * 1024;
const MAXIMUM_PACKETS: u64 = 131_072;

#[derive(Serialize)]
struct RetainedMovie {
    path: PathBuf,
    byte_length: u64,
    sha256: String,
}

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [package, revision, worker, report] = arguments.as_slice() else {
        return Err("expected PACKAGE REVISION WORKER REPORT".into());
    };
    let package = fs::canonicalize(package)?;
    let revision = RevisionId::new(revision.to_str().ok_or("revision must be UTF-8")?)?;
    let runtime = RenderWorkerRuntime {
        executable: fs::canonicalize(worker)?,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let report = private_report_path(Path::new(report))?;
    if report.starts_with(&package) {
        return Err("qualification outputs must be outside the project package".into());
    }
    let mut output = private_file(&report)?;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(240);
    let cancelled = AtomicBool::new(false);
    let identity = RenderIdentity {
        request_id: RequestId::new(uuid::Uuid::new_v4().to_string())?,
        attempt_id: AttemptId::new(uuid::Uuid::new_v4().to_string())?,
    };
    let cancellation_token = CancellationToken::new(uuid::Uuid::new_v4().to_string())?;
    let mut evidence = json!({
        "schema_version": 1,
        "status": "running",
        "stage": "capture",
        "scope": "fresh automatic admission, bound committed encoding and independent verification; no durable job, project write, destination publication or public Render UI",
        "package": package,
        "revision": revision,
        "worker": runtime.executable,
        "identity": identity,
        "cleanup_confirmed": true,
        "maximum_seconds": 240,
        "maximum_movie_bytes": MAXIMUM_BYTES,
        "maximum_movie_packets": MAXIMUM_PACKETS,
    });
    let result = (|| -> Result<()> {
        check_deadline(deadline)?;
        let pictures = ProjectPictureSession::open_revision(&package, &revision, None, &cancelled)?;
        let captured = ExportPictureContract::capture(&pictures)?;
        drop(pictures);
        check_deadline(deadline)?;
        evidence["captured_contract"] = serde_json::to_value(&captured)?;
        let (picture_reference_bytes, audio_reference_bytes) = reference_extents(&captured)?;
        evidence["reference_extents"] = json!({
            "picture_bytes": picture_reference_bytes,
            "audio_bytes": audio_reference_bytes,
            "maximum_frames": 540,
            "maximum_bytes_per_reference": MAXIMUM_BYTES,
        });
        evidence["stage"] = json!("qualify");
        let rate = captured.frame_rate();
        let mut qualified = qualify(
            &runtime,
            AdmissionRequest {
                identity: identity.clone(),
                cancellation_token: cancellation_token.clone(),
                raster: captured.raster(),
                frame_rate: [rate.numerator(), rate.denominator()],
                color_policy: captured.color_policy(),
            },
            AdmissionLimits::default(),
            &cancelled,
            deadline,
            |_, _, _| {},
        )
        .map_err(|error| -> Box<dyn std::error::Error> {
            evidence["cleanup_confirmed"] = json!(error.error.cleanup_confirmed());
            evidence["rejected"] = json!(error.rejected);
            error.into()
        })?;
        evidence["decision"] = serde_json::to_value(qualified.decision())?;
        let selected = &qualified.decision().selected;
        let probe_hash = selected.manifest.movie.sha256().as_str().to_owned();
        let probe_bytes = selected.manifest.movie.byte_length();
        let encoding = EncodedRenderContract::from_contract(&captured, qualified.choice());
        let native = encoding.native_contract()?;
        let expected_binding = EncodingBinding {
            schema_version: 1,
            policy_version: SDR_POLICY_VERSION_V1,
            raster: captured.raster(),
            frame_rate: [rate.numerator(), rate.denominator()],
            choice: qualified.choice(),
            settings: ResolvedSdrSettings::from(native.policy()),
            runtime: selected.runtime.clone(),
        };
        expected_binding.validate_for(&encoding)?;
        evidence["expected_binding"] = serde_json::to_value(&expected_binding)?;
        evidence["stage"] = json!("retain_probe");
        let retained_probe = retain_movie(
            &report.with_extension("probe.mp4"),
            probe_bytes,
            &probe_hash,
            deadline,
            |file| qualified.copy_probe_to(file, &cancelled, deadline),
        )?;
        evidence["retained_probe"] = serde_json::to_value(retained_probe)?;
        evidence["stage"] = json!("encode");
        let automatic = qualified
            .encode(
                &runtime,
                RenderPictureRequest {
                    package: package.clone(),
                    revision: revision.clone(),
                    range: Some(captured.range()),
                    identity: identity.clone(),
                    cancellation_token: cancellation_token.clone(),
                },
                EncodedWorkerLimits {
                    encode: EncodeLimits {
                        maximum_output_bytes: MAXIMUM_BYTES,
                        maximum_packets: MAXIMUM_PACKETS,
                        maximum_packet_bytes: 8 * 1024 * 1024,
                    },
                    ..EncodedWorkerLimits::default()
                },
                &cancelled,
                deadline,
                |_| {},
            )
            .map_err(|error| -> Box<dyn std::error::Error> {
                evidence["cleanup_confirmed"] = json!(error.cleanup_confirmed());
                error.into()
            })?;
        if serde_json::to_value(automatic.decision())? != evidence["decision"] {
            return Err("consumed encoder changed its admission decision".into());
        }
        let (mut candidate, _) = automatic.into_parts();
        evidence["manifest"] = serde_json::to_value(candidate.manifest())?;
        evidence["binding"] = serde_json::to_value(candidate.encoding_binding())?;
        if candidate.contract() != &captured
            || candidate.manifest().contract != encoding
            || candidate.encoding_binding() != Some(&expected_binding)
        {
            return Err(
                "bound candidate differs from the captured contract or exact runtime".into(),
            );
        }
        let movie_hash = candidate.manifest().movie.sha256().as_str().to_owned();
        let movie_bytes = candidate.byte_length();
        evidence["stage"] = json!("retain_candidate");
        let retained_candidate = retain_movie(
            &report.with_extension("candidate.mp4"),
            movie_bytes,
            &movie_hash,
            deadline,
            |file| candidate.copy_to(file, &cancelled, deadline),
        )?;
        evidence["retained_candidate"] = serde_json::to_value(retained_candidate)?;
        evidence["stage"] = json!("capture_reference");
        let reference_name = format!(
            "{}-{}",
            report
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or("reference filename must be UTF-8")?,
            uuid::Uuid::new_v4(),
        );
        evidence["direct_inputs"] = reference::capture(
            &package,
            &revision,
            captured.range(),
            &reference_name,
            report.parent().ok_or("reference directory missing")?,
            deadline,
        )?;
        if evidence["direct_inputs"]["contract"] != evidence["captured_contract"] {
            return Err("fresh reference contract differs from the captured revision".into());
        }
        for (field, expected) in [
            ("picture_reference", picture_reference_bytes),
            ("audio_reference", audio_reference_bytes),
        ] {
            let path = evidence["direct_inputs"][field]
                .as_str()
                .ok_or("reference path missing")?;
            if fs::metadata(path)?.len() != expected {
                return Err(
                    "fresh reference length differs from the exact captured interval".into(),
                );
            }
        }
        check_deadline(deadline)?;
        evidence["stage"] = json!("verify");
        let verification_identity = RenderIdentity {
            request_id: identity.request_id.clone(),
            attempt_id: AttemptId::new(uuid::Uuid::new_v4().to_string())?,
        };
        evidence["verification_identity"] = serde_json::to_value(&verification_identity)?;
        let mut verified = verify(
            &runtime,
            candidate,
            VerificationRequest {
                identity: verification_identity,
                cancellation_token: CancellationToken::new(uuid::Uuid::new_v4().to_string())?,
                limits: VerificationLimits {
                    maximum_bytes: MAXIMUM_BYTES,
                    maximum_packets: MAXIMUM_PACKETS,
                },
            },
            &cancelled,
            deadline,
            |_| {},
        )
        .map_err(|error| -> Box<dyn std::error::Error> {
            evidence["cleanup_confirmed"] = json!(error.error.cleanup_confirmed());
            error
        })?;
        evidence["verification"] = serde_json::to_value(verified.report())?;
        if verified.candidate().encoding_binding() != Some(&expected_binding) {
            return Err("verified candidate lost its exact encoding binding".into());
        }
        evidence["stage"] = json!("retain_verified");
        let retained_verified = retain_movie(
            &report.with_extension("verified.mp4"),
            movie_bytes,
            &movie_hash,
            deadline,
            |file| verified.copy_to(file, &cancelled, deadline),
        )?;
        evidence["retained_verified"] = serde_json::to_value(retained_verified)?;
        check_deadline(deadline)?;
        evidence["stage"] = json!("complete");
        Ok(())
    })();
    evidence["seconds"] = json!(started.elapsed().as_secs_f64());
    match &result {
        Ok(()) => evidence["status"] = json!("passed"),
        Err(error) => {
            evidence["status"] = json!("failed");
            evidence["error"] = json!(error.to_string());
        }
    }
    serde_json::to_writer_pretty(&mut output, &evidence)?;
    writeln!(output)?;
    output.sync_all()?;
    result
}

fn private_report_path(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let parent = absolute
        .parent()
        .ok_or("report parent missing")?
        .canonicalize()?;
    if !parent.starts_with(fs::canonicalize("/tmp")?) {
        return Err("qualification output must be in /tmp".into());
    }
    Ok(parent.join(absolute.file_name().ok_or("report filename missing")?))
}

fn reference_extents(contract: &ExportPictureContract) -> Result<(u64, u64)> {
    if contract.frame_count() == 0 || contract.frame_count() > 540 {
        return Err("reference qualification requires between 1 and 540 frames".into());
    }
    let [width, height] = contract.raster();
    let picture_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(3))
        .map(|bytes| bytes / 2)
        .and_then(|bytes| bytes.checked_mul(contract.frame_count()))
        .ok_or("reference picture extent overflow")?;
    let samples = contract
        .project_audio_end()
        .0
        .checked_sub(contract.project_audio_start().0)
        .and_then(|samples| u64::try_from(samples).ok())
        .ok_or("reference audio interval overflow")?;
    let audio_bytes = samples
        .checked_mul(8)
        .ok_or("reference PCM extent overflow")?;
    if !(1..=MAXIMUM_BYTES).contains(&picture_bytes) || !(1..=MAXIMUM_BYTES).contains(&audio_bytes)
    {
        return Err("reference qualification exceeds 512 MiB per picture or PCM file".into());
    }
    Ok((picture_bytes, audio_bytes))
}

fn private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

fn retain_movie(
    path: &Path,
    expected_bytes: u64,
    expected_hash: &str,
    deadline: Instant,
    copy: impl FnOnce(&mut File) -> std::result::Result<u64, EncodedRenderError>,
) -> Result<RetainedMovie> {
    check_deadline(deadline)?;
    if expected_bytes == 0 || expected_bytes > MAXIMUM_BYTES {
        return Err("retained movie exceeds the qualification byte bound".into());
    }
    let mut file = private_file(path)?;
    if copy(&mut file)? != expected_bytes {
        return Err("retained movie copy count differs".into());
    }
    file.sync_all()?;
    check_deadline(deadline)?;
    if file.metadata()?.len() != expected_bytes {
        return Err("retained movie length differs".into());
    }
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        check_deadline(deadline)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(u64::try_from(read)?)
            .ok_or("hash count overflow")?;
        if count > expected_bytes {
            return Err("retained movie grew while hashing".into());
        }
        hash.update(&buffer[..read]);
    }
    let sha256: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if count != expected_bytes
        || file.metadata()?.len() != expected_bytes
        || sha256 != expected_hash
    {
        return Err("retained movie bytes differ from admitted identity".into());
    }
    check_deadline(deadline)?;
    Ok(RetainedMovie {
        path: path.to_path_buf(),
        byte_length: count,
        sha256,
    })
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err("bound encoder qualification exceeded its shared deadline".into())
    } else {
        Ok(())
    }
}
