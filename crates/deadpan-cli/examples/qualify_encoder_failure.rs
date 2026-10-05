//! Real engineering-path qualification of typed encoder failure transport.
//! Usage: qualify_encoder_failure SOURCE_PACKAGE WORKER REPORT
//! It expects the measured hardware/B-frame rejection, then separately encodes
//! and verifies hardware/no-B output. It does not implement automatic fallback.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        self, EncodedCandidate, EncodedRenderError, EncodedWorkerLimits,
        protocol::{EncodedFailureKind, EncoderChoice},
        verification::{self, VerificationLimits, VerificationRequest},
    },
    render_worker::{RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity},
};
use deadpan_core::{FrameRange, ProjectFrame, RevisionId};
use deadpan_encode::{BFramePolicy, EncodeFailureKind, EncoderMode};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, render::document_sha256};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_EVIDENCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MOVIE_BYTES: u64 = 512 * 1024 * 1024;

fn main() -> Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("expected SOURCE_PACKAGE WORKER REPORT".into());
    }
    let package = fs::canonicalize(&arguments[0])?;
    let worker = fs::canonicalize(&arguments[1])?;
    let selected_report = std::path::absolute(&arguments[2])?;
    let directory = selected_report
        .parent()
        .ok_or("report has no parent")?
        .canonicalize()?;
    let report_path = directory.join(
        selected_report
            .file_name()
            .ok_or("report has no filename")?,
    );
    let scratch = fs::canonicalize("/tmp")?;
    if !package.starts_with(&scratch)
        || !directory.starts_with(&scratch)
        || directory.starts_with(&package)
    {
        return Err(
            "qualification requires a /tmp package and a separate /tmp report directory".into(),
        );
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&report_path)?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let cancelled = AtomicBool::new(false);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let snapshot = store.snapshot()?;
    let original = authored(&package)?;
    let frames = snapshot.duration()?.frames().min(90);
    let range = FrameRange::new(ProjectFrame(0), ProjectFrame(frames))?;
    // Round the half-second target upward, conservatively requiring at least
    // two actual GOP lengths. The production host captures the exact contract.
    let rate = snapshot.presentation_basis().frame_rate;
    let minimum_frames = u64::from(rate.numerator())
        .div_ceil(2 * u64::from(rate.denominator()))
        .max(1)
        * 2;
    if u64::try_from(frames)? < minimum_frames {
        return Err(
            "qualification requires at least two GOPs inside the first 90 project frames".into(),
        );
    }
    let runtime = RenderWorkerRuntime {
        executable: worker.clone(),
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let mut report = json!({
        "schema_version": 1, "status": "running", "package": package, "worker": file_identity(&worker, deadline)?,
        "project_id": snapshot.project_id(), "revision": snapshot.revision_id(),
        "document_sha256": document_sha256(&snapshot, &cancelled, deadline)?,
        "range": range, "basis": snapshot.presentation_basis(), "minimum_probe_frames": minimum_frames,
        "authoring_before": original,
        "scope": "real typed hardware/B rejection followed by a separately requested hardware/no-B encode and independent verification",
        "limitations": ["explicit engineering attempts only; no automatic selection or fallback", "expects the previously measured hardware-B failure on this host", "independent content/AVFoundation qualification is a separate parent check", "no destination publication or authored changes"],
    });
    save(&mut output, &report)?;
    let run = Run {
        runtime: &runtime,
        package: &package,
        revision: snapshot.revision_id(),
        range,
        cancelled: &cancelled,
        deadline,
    };
    let result = qualify(&run, &report_path, &mut report);
    let unchanged = (|| -> Result {
        let after = store.snapshot()?;
        let after_rows = authored(&package)?;
        report["authoring_after"] = serde_json::to_value(&after_rows)?;
        report["authoring_unchanged"] = json!(after == snapshot && after_rows == original);
        if after != snapshot || after_rows != original {
            return Err("qualification changed authoritative snapshot or authoring history".into());
        }
        store.validate_full()?;
        Ok(())
    })();
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    if let Err(error) = &unchanged {
        report["authoring_error"] = json!(error.to_string());
    }
    report["status"] = json!(if result.is_ok() && unchanged.is_ok() {
        "passed"
    } else {
        "failed"
    });
    save(&mut output, &report)?;
    result?;
    unchanged
}

fn choice(b_frames: BFramePolicy) -> EncoderChoice {
    EncoderChoice {
        mode: EncoderMode::Hardware,
        b_frames,
    }
}

fn identity(attempt: &str) -> Result<RenderIdentity> {
    Ok(RenderIdentity {
        request_id: RequestId::new("typed-encoder-qualification")?,
        attempt_id: AttemptId::new(attempt)?,
    })
}

fn request(
    package: &Path,
    revision: &RevisionId,
    range: FrameRange,
    attempt: &str,
) -> Result<RenderPictureRequest> {
    Ok(RenderPictureRequest {
        package: package.to_owned(),
        revision: revision.clone(),
        range: Some(range),
        identity: identity(attempt)?,
        cancellation_token: CancellationToken::new(format!("{attempt}-cancel"))?,
    })
}

struct Run<'a> {
    runtime: &'a RenderWorkerRuntime,
    package: &'a Path,
    revision: &'a RevisionId,
    range: FrameRange,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Run<'_> {
    fn encode(
        &self,
        attempt: &str,
        b_frames: BFramePolicy,
        report: &mut Value,
    ) -> Result<std::result::Result<EncodedCandidate, EncodedRenderError>> {
        let mut limits = EncodedWorkerLimits::default();
        limits.encode.maximum_output_bytes = MAX_MOVIE_BYTES;
        let mut progress = Vec::new();
        let mut progress_overflow = false;
        let result = encoded_render::encode(
            self.runtime,
            request(self.package, self.revision, self.range, attempt)?,
            choice(b_frames),
            limits,
            self.cancelled,
            self.deadline,
            |value| {
                if progress.len() < 1024 {
                    progress.push(json!({"frames": value.completed_frames, "total_frames": value.total_frames,
                        "samples": value.completed_audio_samples, "total_samples": value.total_audio_samples}));
                } else {
                    progress_overflow = true;
                }
            },
        );
        report["progress"] = json!(progress);
        report["progress_overflow"] = json!(progress_overflow);
        if progress_overflow {
            return Err("qualification progress bound exceeded".into());
        }
        Ok(result)
    }
}

fn qualify(run: &Run<'_>, report_path: &Path, report: &mut Value) -> Result {
    let runtime = run.runtime;
    let cancelled = run.cancelled;
    let deadline = run.deadline;
    report["hardware_two"] = json!({"requested_choice": choice(BFramePolicy::TargetTwo)});
    let error = match run.encode(
        "hardware-two",
        BFramePolicy::TargetTwo,
        &mut report["hardware_two"],
    )? {
        Err(error) => error,
        Ok(mut candidate) => {
            report["hardware_two"]["candidate_returned"] = json!(true);
            report["hardware_two"]["unexpected_manifest"] =
                serde_json::to_value(candidate.manifest())?;
            report["hardware_two"]["unexpected_movie"] = copy_candidate(
                &mut candidate,
                &report_path.with_extension("unexpected-hardware-two.mp4"),
                cancelled,
                deadline,
            )?;
            return Err(
                "hardware TargetTwo unexpectedly succeeded; expected rejection was not reproduced"
                    .into(),
            );
        }
    };
    report["hardware_two"]["diagnostic"] = json!(error.to_string());
    report["hardware_two"]["cleanup_confirmed"] = json!(error.cleanup_confirmed());
    report["hardware_two"]["candidate_returned"] = json!(false);
    if let EncodedRenderError::WorkerFailure(failure) = &error {
        report["hardware_two"]["typed_failure"] = serde_json::to_value(failure)?;
    }
    if !error.cleanup_confirmed()
        || !matches!(&error, EncodedRenderError::WorkerFailure(failure)
        if failure.kind == EncodedFailureKind::Encoder(EncodeFailureKind::VideoTimestampOrder))
    {
        return Err("hardware TargetTwo did not return the exact typed timestamp rejection with proven cleanup".into());
    }
    report["hardware_two"]["passed"] = json!(true);

    report["hardware_none"] = json!({"requested_choice": choice(BFramePolicy::None)});
    let candidate = match run.encode(
        "hardware-none",
        BFramePolicy::None,
        &mut report["hardware_none"],
    )? {
        Ok(candidate) => candidate,
        Err(error) => {
            report["hardware_none"]["diagnostic"] = json!(error.to_string());
            report["hardware_none"]["cleanup_confirmed"] = json!(error.cleanup_confirmed());
            return Err(error.into());
        }
    };
    report["hardware_none"]["manifest"] = serde_json::to_value(candidate.manifest())?;
    let mut verified = match verification::verify(
        runtime,
        candidate,
        VerificationRequest {
            identity: identity("hardware-none-verify")?,
            cancellation_token: CancellationToken::new("hardware-none-verify-cancel")?,
            limits: VerificationLimits {
                maximum_bytes: MAX_MOVIE_BYTES,
                maximum_packets: 1_000_000,
            },
        },
        cancelled,
        deadline,
        |_| {},
    ) {
        Ok(verified) => verified,
        Err(mut failure) => {
            report["hardware_none"]["verification_failure"] = json!({"diagnostic": failure.error.to_string(), "cleanup_confirmed": failure.error.cleanup_confirmed()});
            let path = report_path.with_extension("unverified-hardware-none.mp4");
            match copy_candidate(&mut failure.candidate, &path, cancelled, deadline) {
                Ok(movie) => report["hardware_none"]["unverified_movie"] = movie,
                Err(error) => {
                    report["hardware_none"]["retention_error"] =
                        json!({"path": path, "error": error.to_string()});
                }
            }
            return Err(failure);
        }
    };
    report["hardware_none"]["verification"] = serde_json::to_value(verified.report())?;
    let movie_path = report_path.with_extension("verified-hardware-none.mp4");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&movie_path)?;
    let copied = verified.copy_to(&mut file, cancelled, deadline)?;
    file.sync_all()?;
    let retained = file_identity(&movie_path, deadline)?;
    if copied != verified.report().movie_bytes
        || retained["sha256"] != json!(verified.report().movie_sha256)
        || retained["bytes"] != json!(verified.report().movie_bytes)
    {
        return Err("retained verified movie differs from fresh verification".into());
    }
    report["hardware_none"]["movie"] = retained;
    report["hardware_none"]["passed"] = json!(true);
    Ok(())
}

fn copy_candidate(
    candidate: &mut EncodedCandidate,
    path: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Value> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    candidate.copy_to(&mut file, cancelled, deadline)?;
    file.sync_all()?;
    file_identity(path, deadline)
}

fn file_identity(path: &Path, deadline: Instant) -> Result<Value> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if Instant::now() >= deadline {
            return Err("qualification hashing exceeded deadline".into());
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(u64::try_from(count)?)
            .ok_or("hash byte count overflow")?;
        if bytes > MAX_MOVIE_BYTES {
            return Err("qualification hashed file exceeds bound".into());
        }
        hasher.update(&buffer[..count]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(json!({"path": path, "bytes": bytes, "sha256": digest}))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Authoring {
    sha256: String,
    bytes: u64,
    table_rows: Vec<u64>,
}

fn authored(package: &Path) -> Result<Authoring> {
    let mut connection = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let transaction = connection.transaction()?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut table_rows = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        hasher.update(query.as_bytes());
        let mut statement = transaction.prepare(query)?;
        let mut rows = statement.query([])?;
        let mut count = 0_u64;
        while let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let length = u64::try_from(value.len())?;
            bytes = bytes
                .checked_add(length)
                .ok_or("authored byte count overflow")?;
            count = count.checked_add(1).ok_or("authored row count overflow")?;
            if bytes > 16 * 1024 * 1024 || count > 4096 {
                return Err("qualification authoring bound".into());
            }
            hasher.update(length.to_le_bytes());
            hasher.update(value.as_bytes());
        }
        table_rows.push(count);
    }
    Ok(Authoring {
        sha256: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes,
        table_rows,
    })
}

fn save(file: &mut File, report: &Value) -> Result {
    let bytes = serde_json::to_vec_pretty(report)?;
    if bytes.len() > MAX_EVIDENCE_BYTES {
        return Err("qualification report bound".into());
    }
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
