use std::io::{Read, Write};

use deadpan_encode::EncodeLimits;
use deadpan_jobs::{
    CancellationToken,
    process::{ResponseKind, SupervisorError, WorkerProtocol},
    protocol::{read_frame, write_frame},
};
use serde::{Deserialize, Serialize};

use super::{MAX_PROBE_BYTES, MAX_PROBE_PACKETS, ProbeReport, ProbeSpec};
use crate::{encoded_render::protocol::EncodedFailure, render_worker::protocol::RenderIdentity};

pub use crate::encoded_render::protocol::{MOVIE_REF, OUTPUT_SCOPE};
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Probe {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
        spec: ProbeSpec,
        limits: EncodeLimits,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
    },
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Probe {
                protocol,
                spec,
                limits,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                let native = spec.contract()?.native_contract()?;
                limits
                    .validate_for(&native)
                    .map_err(|error| error.to_string())?;
                if limits.maximum_output_bytes > MAX_PROBE_BYTES
                    || limits.maximum_packets > MAX_PROBE_PACKETS
                    || !(1..=120_000).contains(timeout_millis)
                {
                    return Err("probe exceeds byte or deadline bound".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Progress {
        protocol: u32,
        identity: RenderIdentity,
        completed_frames: u64,
        total_frames: u64,
    },
    Completed {
        protocol: u32,
        identity: RenderIdentity,
        report: Box<ProbeReport>,
    },
    Failed {
        protocol: u32,
        identity: RenderIdentity,
        failure: EncodedFailure,
    },
    Cancelled {
        protocol: u32,
        identity: RenderIdentity,
    },
}

impl WorkerMessage {
    fn identity(&self) -> &RenderIdentity {
        match self {
            Self::Progress { identity, .. }
            | Self::Completed { identity, .. }
            | Self::Failed { identity, .. }
            | Self::Cancelled { identity, .. } => identity,
        }
    }
    fn version(&self) -> u32 {
        match self {
            Self::Progress { protocol, .. }
            | Self::Completed { protocol, .. }
            | Self::Failed { protocol, .. }
            | Self::Cancelled { protocol, .. } => *protocol,
        }
    }
}

fn version(value: u32) -> Result<(), String> {
    if value == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err("unsupported encoder probe protocol".into())
    }
}

pub fn read_host_message(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let request: Option<HostMessage> = read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(request) = &request {
        request.validate()?;
    }
    Ok(request)
}
pub fn write_worker_message(
    writer: &mut impl Write,
    message: &WorkerMessage,
) -> Result<(), String> {
    version(message.version())?;
    write_frame(writer, message).map_err(|error| error.to_string())
}

pub struct ProbeProtocol {
    identity: RenderIdentity,
    token: CancellationToken,
    spec: ProbeSpec,
    limits: EncodeLimits,
}
impl WorkerProtocol for ProbeProtocol {
    type Request = HostMessage;
    type Response = WorkerMessage;
    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        let HostMessage::Probe {
            identity,
            cancellation_token,
            spec,
            limits,
            ..
        } = request
        else {
            return Err(SupervisorError::Request(
                "initial probe request must be Probe".into(),
            ));
        };
        Ok(Self {
            identity: identity.clone(),
            token: cancellation_token.clone(),
            spec: spec.clone(),
            limits: *limits,
        })
    }
    fn cancellation(&self) -> HostMessage {
        HostMessage::Cancel {
            protocol: PROTOCOL_VERSION,
            identity: self.identity.clone(),
            cancellation_token: self.token.clone(),
        }
    }
    fn write_request(writer: &mut impl Write, request: &HostMessage) -> Result<(), String> {
        request.validate()?;
        write_frame(writer, request).map_err(|error| error.to_string())
    }
    fn read_response(reader: &mut impl Read) -> Result<Option<WorkerMessage>, String> {
        let message: Option<WorkerMessage> =
            read_frame(reader).map_err(|error| error.to_string())?;
        if let Some(message) = &message {
            version(message.version())?;
        }
        Ok(message)
    }
    fn classify(&self, response: &WorkerMessage) -> Result<ResponseKind, String> {
        version(response.version())?;
        if response.identity() != &self.identity {
            return Err("probe response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Progress {
                completed_frames,
                total_frames,
                ..
            } => {
                if *total_frames != self.spec.contract()?.picture.frame_count
                    || completed_frames > total_frames
                {
                    return Err("probe progress changed its frame count".into());
                }
                Ok(ResponseKind::Progress)
            }
            WorkerMessage::Completed { report, .. } => {
                report.validate(self.limits)?;
                if report.spec != self.spec {
                    return Err("probe completion changed its specification".into());
                }
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}
