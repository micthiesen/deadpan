//! Supervised, descriptor-verified landmark observations. Only the pure host
//! policy decides whether an observed candidate is admissible.

use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::generated_region::RegionSeeds;
use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
use deadpan_jobs::landmarks::{
    self, BoundaryInputs, HostMessage, InspectionObservations, LandmarkProtocol,
    RegionRuntimeReport, RuntimeReport, WorkerMessage,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::CanonicalMedia;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{QualificationError, RetainedConditioning};

mod extension;
pub(super) use extension::expected_pts as extension_picture_pts;
pub(super) use extension::inspect as inspect_extension;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionTimings {
    pub decode_millis: u64,
    pub vision_millis: u64,
    pub elapsed_millis: u64,
}

pub(super) struct Observed {
    pub batch: InspectionObservations,
    pub runtime: RuntimeReport,
    pub region_runtime: Option<RegionRuntimeReport>,
    pub timings: InspectionTimings,
}

pub(super) fn inspect(
    executable: &Path,
    native: &mut CanonicalMedia,
    conditioning: &mut RetainedConditioning,
    contract: VideoContract,
    region_seeds: Option<RegionSeeds>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Observed, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    control.check()?;
    let directory = tempfile::Builder::new()
        .prefix("deadpan-landmarks-")
        .tempdir()?;
    std::fs::create_dir(directory.path().join("input"))?;
    std::fs::create_dir(directory.path().join("output"))?;
    let pinned = ArtifactWorkspace::open(directory.path())?;
    let source_object = native.object().clone();
    let source_hash = native
        .verified_source_input()
        .map_err(invalid)?
        .identity()
        .sha256();
    let source = copy_input(
        native,
        &source_object,
        source_hash,
        directory.path(),
        "native.mkv",
        landmarks::MAX_SOURCE_BYTES,
        &control,
    )?;
    let mut boundary = |left, name| {
        let input = conditioning.boundary_mut(left);
        let object = input.object().clone();
        let hash = decode_hash(input.declaration().sha256());
        copy_input(
            input,
            &object,
            hash,
            directory.path(),
            name,
            landmarks::MAX_PNG_BYTES,
            &control,
        )
    };
    let boundaries = BoundaryInputs {
        left: boundary(true, "left.png")?,
        right: boundary(false, "right.png")?,
    };
    let expected = picture_pts(contract)?;
    let output_scope = WorkspaceRef::new("output").map_err(invalid)?;
    let remaining = control.remaining()?.min(Duration::from_secs(3600));
    let request = HostMessage::InspectLandmarks {
        protocol: landmarks::VERSION,
        request: RequestId::new("bridge-landmarks").map_err(invalid)?,
        attempt: AttemptId::new("inspection").map_err(invalid)?,
        cancellation_token: CancellationToken::new("cancel-inspection").map_err(invalid)?,
        source,
        stream: landmarks::ExpectedStream {
            stream_index: 0,
            width: contract.width,
            height: contract.height,
            time_base_num: 1,
            time_base_den: 1000,
            rotation_quarter_turns: 0,
        },
        picture_pts: expected.clone(),
        boundaries: Some(Box::new(boundaries)),
        region_seeds: region_seeds.map(Box::new),
        output_scope: output_scope.clone(),
        maximum_output_bytes: landmarks::MAX_OBSERVATION_BYTES,
        timeout_millis: u64::try_from(remaining.as_millis()).map_err(invalid)?,
    };
    request.validate().map_err(invalid)?;
    let (snapshot, completion) = execute_inspection(
        executable,
        directory.path(),
        &pinned,
        request,
        &output_scope,
        &control,
    )?;
    let batch: InspectionObservations = serde_json::from_reader(snapshot)?;
    control.check()?;
    batch
        .validate(&expected, region_seeds.as_ref())
        .map_err(invalid)?;
    if region_seeds.is_some() != completion.region_runtime.is_some() {
        return Err(invalid(
            "region runtime differs from requested capture availability",
        ));
    }
    if batch.landmarks.boundaries.is_none() {
        return Err(invalid(
            "worker omitted the requested conditioning observations",
        ));
    }
    Ok(Observed {
        batch,
        runtime: completion.runtime,
        region_runtime: completion.region_runtime,
        timings: completion.timings,
    })
}

/// Shared checked launch, teardown and private output snapshot for both
/// inspection operations. No observations escape a live worker workspace.
fn execute_inspection(
    executable: &Path,
    directory: &Path,
    pinned: &ArtifactWorkspace,
    request: HostMessage,
    output_scope: &WorkspaceRef,
    control: &Control<'_>,
) -> Result<(deadpan_jobs::artifact::HashedArtifactSnapshot, Completion), QualificationError> {
    let remaining = control.remaining()?.min(Duration::from_secs(3600));
    let mut process = SupervisedProcess::<LandmarkProtocol>::spawn(
        ProcessSpec {
            executable: executable.to_path_buf(),
            arguments: vec![landmarks::WORKER_ARGUMENT.into()],
            environment: Default::default(),
            workspace: directory.to_path_buf(),
            limits: ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: Duration::from_secs(2).min(remaining),
                exit_grace: Duration::from_secs(5).min(remaining),
            },
        },
        request,
    )
    .map_err(invalid)?;
    let result = supervise(&mut process, control);
    // Teardown has its own real monotonic bound, even after cancellation or
    // the qualification deadline. Never admit output before confirmed cleanup.
    let stopped = process
        .finish_owned_work(Instant::now() + Duration::from_secs(5))
        .map_err(|error| invalid(format!("worker cleanup is unconfirmed: {error}")))?;
    if stopped.pump_panicked() {
        return Err(invalid(
            "worker I/O pump panicked; observations cannot be admitted",
        ));
    }
    let completion = result?;
    if !stopped.status().success() {
        return Err(invalid(format!(
            "worker completion has failed exit status {}",
            stopped.status()
        )));
    }
    control.check()?;
    let snapshot = pinned
        .snapshot_with_control(
            output_scope,
            &completion.artifact,
            ArtifactLimits::new(landmarks::MAX_OBSERVATION_BYTES)?,
            || control.snapshot_check(),
        )
        .map_err(super::qualification::snapshot_error)?;
    Ok((snapshot, completion))
}

struct Completion {
    artifact: WorkspaceArtifact,
    runtime: RuntimeReport,
    region_runtime: Option<RegionRuntimeReport>,
    timings: InspectionTimings,
}

fn supervise(
    process: &mut SupervisedProcess<LandmarkProtocol>,
    control: &Control<'_>,
) -> Result<Completion, QualificationError> {
    let mut completion = None;
    let mut failure = None;
    let mut cancelling = false;
    while !process.is_finished() {
        let now = Instant::now();
        if control.cancelled.load(Ordering::Acquire) && !cancelling {
            cancelling = true;
            process.request_cancel(now).map_err(invalid)?;
        }
        if now >= control.deadline {
            return Err(if cancelling {
                QualificationError::Cancelled
            } else {
                QualificationError::Deadline
            });
        }
        for event in process.poll(now).map_err(invalid)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    WorkerMessage::Progress { .. } => {}
                    WorkerMessage::Completed {
                        observations,
                        runtime,
                        region_runtime,
                        decode_millis,
                        vision_millis,
                        elapsed_millis,
                        ..
                    } => {
                        completion = Some(Completion {
                            artifact: observations,
                            runtime,
                            region_runtime,
                            timings: InspectionTimings {
                                decode_millis,
                                vision_millis,
                                elapsed_millis,
                            },
                        });
                    }
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| invalid(diagnostic.as_str()));
                    }
                    WorkerMessage::Cancelled { .. } if cancelling => {}
                    WorkerMessage::Cancelled { .. } => {
                        failure.get_or_insert_with(|| invalid("unrequested worker cancellation"));
                    }
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert_with(|| invalid(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated) && !cancelling {
                        failure.get_or_insert_with(|| invalid(format!("worker exited {status}")));
                    }
                }
            }
        }
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    control.check()?;
    if let Some(error) = failure {
        return Err(error);
    }
    completion.ok_or_else(|| invalid("worker exited without a complete observation batch"))
}

pub(super) fn picture_pts(contract: VideoContract) -> Result<Vec<i64>, QualificationError> {
    if !(2..=landmarks::MAX_FRAMES as u32).contains(&contract.frames) {
        return Err(invalid(
            "native frame count exceeds the landmark inspection bound",
        ));
    }
    (0..contract.frames)
        .map(|ordinal| contract.matroska_pts(ordinal).map_err(invalid))
        .collect()
}

fn copy_input(
    input: &mut (impl Read + Seek),
    object: &GeneratedObjectRef,
    expected_sha256: [u8; 32],
    directory: &Path,
    name: &str,
    maximum: u64,
    control: &Control<'_>,
) -> Result<WorkspaceArtifact, QualificationError> {
    control.check()?;
    if object.byte_length() == 0 || object.byte_length() > maximum {
        return Err(invalid("input exceeds the bounded copy limit"));
    }
    input.rewind()?;
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("input").join(name))?;
        let mut sha256 = sha2::Sha256::new();
        let mut blake3 = blake3::Hasher::new();
        let mut total = 0_u64;
        let mut bytes = [0; 64 * 1024];
        loop {
            control.check()?;
            let read = input.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .ok_or_else(|| invalid("input length overflow"))?;
            if total > object.byte_length() {
                return Err(invalid("input grew while copying"));
            }
            sha256.update(&bytes[..read]);
            blake3.update(&bytes[..read]);
            output.write_all(&bytes[..read])?;
        }
        let digest: [u8; 32] = sha256.finalize().into();
        if total != object.byte_length()
            || digest != expected_sha256
            || blake3.finalize().to_hex().as_str() != object.content().digest()
        {
            return Err(invalid("copied input differs from its retained identity"));
        }
        control.check()?;
        WorkspaceArtifact::new(
            WorkspaceRef::new(format!("input/{name}")).map_err(invalid)?,
            Sha256::new(
                digest
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
            .map_err(invalid)?,
            total,
        )
        .map_err(invalid)
    })();
    input.rewind()?;
    result
}

fn decode_hash(hash: &Sha256) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash.as_str()[index * 2..index * 2 + 2], 16)
            .expect("Sha256 validates lowercase hexadecimal bytes");
    }
    bytes
}

struct Control<'a> {
    deadline: Instant,
    cancelled: &'a AtomicBool,
}

impl Control<'_> {
    fn remaining(&self) -> Result<Duration, QualificationError> {
        self.check()?;
        Ok(self.deadline.saturating_duration_since(Instant::now()))
    }
    fn check(&self) -> Result<(), QualificationError> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(QualificationError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(QualificationError::Deadline)
        } else {
            Ok(())
        }
    }
    fn snapshot_check(&self) -> Result<(), SnapshotInterruption> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(SnapshotInterruption::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(SnapshotInterruption::Deadline)
        } else {
            Ok(())
        }
    }
}

fn invalid(error: impl std::fmt::Display) -> QualificationError {
    QualificationError::Quality(format!("landmark inspection: {error}"))
}

#[cfg(test)]
#[path = "landmark_inspection_tests.rs"]
mod tests;
