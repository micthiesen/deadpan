use std::io::{Read, Write};

use deadpan_jobs::{
    CancellationToken, Diagnostic,
    process::{ResponseKind, SupervisorError, WorkerProtocol},
    protocol::{read_frame, write_frame},
};
use serde::{Deserialize, Serialize};

use super::{VerificationLimits, VerificationProgress, VerificationReport};
use crate::{encoded_render::protocol::EncodedManifest, render_worker::protocol::RenderIdentity};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Inspect {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
        manifest: Box<EncodedManifest>,
        limits: VerificationLimits,
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
            Self::Inspect {
                protocol,
                manifest,
                limits,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                manifest.validate()?;
                limits.validate()?;
                if manifest.movie.byte_length() > limits.maximum_bytes
                    || manifest
                        .report
                        .video_packets
                        .checked_add(manifest.report.audio_packets)
                        .is_none_or(|count| count > limits.maximum_packets)
                    || !(1..=86_400_000).contains(timeout_millis)
                {
                    return Err(
                        "verification request exceeds bytes, packets or timeout bounds".into(),
                    );
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Progress {
        protocol: u32,
        identity: RenderIdentity,
        progress: VerificationProgress,
    },
    Completed {
        protocol: u32,
        identity: RenderIdentity,
        report: Box<VerificationReport>,
    },
    Failed {
        protocol: u32,
        identity: RenderIdentity,
        diagnostic: Diagnostic,
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
    pub fn validate(&self) -> Result<(), String> {
        let protocol = match self {
            Self::Progress {
                protocol, progress, ..
            } => {
                if progress.total == 0 || progress.completed > progress.total {
                    return Err("verification progress exceeds its stage".into());
                }
                *protocol
            }
            Self::Completed {
                protocol, report, ..
            } => {
                report.validate(VerificationLimits::default())?;
                *protocol
            }
            Self::Failed { protocol, .. } | Self::Cancelled { protocol, .. } => *protocol,
        };
        version(protocol)
    }
}

fn version(value: u32) -> Result<(), String> {
    if value == VERSION {
        Ok(())
    } else {
        Err("unsupported finished-file verification protocol".into())
    }
}

pub struct VerificationProtocol {
    identity: RenderIdentity,
    token: CancellationToken,
    manifest: EncodedManifest,
    limits: VerificationLimits,
}

impl WorkerProtocol for VerificationProtocol {
    type Request = HostMessage;
    type Response = WorkerMessage;
    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        let HostMessage::Inspect {
            identity,
            cancellation_token,
            manifest,
            limits,
            ..
        } = request
        else {
            return Err(SupervisorError::Request(
                "initial verification message must inspect".into(),
            ));
        };
        Ok(Self {
            identity: identity.clone(),
            token: cancellation_token.clone(),
            manifest: *manifest.clone(),
            limits: *limits,
        })
    }
    fn cancellation(&self) -> HostMessage {
        HostMessage::Cancel {
            protocol: VERSION,
            identity: self.identity.clone(),
            cancellation_token: self.token.clone(),
        }
    }
    fn write_request(writer: &mut impl Write, request: &HostMessage) -> Result<(), String> {
        request.validate()?;
        write_frame(writer, request).map_err(|e| e.to_string())
    }
    fn read_response(reader: &mut impl Read) -> Result<Option<WorkerMessage>, String> {
        let value: Option<WorkerMessage> = read_frame(reader).map_err(|e| e.to_string())?;
        if let Some(value) = &value {
            value.validate()?;
        }
        Ok(value)
    }
    fn classify(&self, response: &WorkerMessage) -> Result<ResponseKind, String> {
        response.validate()?;
        if response.identity() != &self.identity {
            return Err("verification response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Progress { progress, .. } => {
                let native = self.manifest.contract.native_contract()?;
                let expected = match progress.stage {
                    super::VerificationStage::Packets => {
                        self.manifest.report.video_packets + self.manifest.report.audio_packets
                    }
                    super::VerificationStage::Pictures => native.video_frames(),
                    super::VerificationStage::ManualAudio
                    | super::VerificationStage::OrdinaryAudio => native.audio_samples(),
                };
                if progress.total != expected {
                    return Err("verification progress differs from captured work".into());
                }
                Ok(ResponseKind::Progress)
            }
            WorkerMessage::Completed { report, .. } => {
                report.validate(self.limits)?;
                bind(report, &self.manifest)?;
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } | WorkerMessage::Cancelled { .. } => {
                Ok(ResponseKind::Terminal)
            }
        }
    }
}

pub(super) fn bind(report: &VerificationReport, manifest: &EncodedManifest) -> Result<(), String> {
    if report.contract != manifest.contract
        || report.document_sha256 != manifest.document_sha256
        || &report.movie_sha256 != manifest.movie.sha256()
        || report.movie_bytes != manifest.movie.byte_length()
        || report.video_packets != manifest.report.video_packets
        || report.audio_packets != manifest.report.audio_packets
    {
        return Err("verification result changed its captured bytes or contract".into());
    }
    Ok(())
}

pub(super) fn read_host(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let value: Option<HostMessage> = read_frame(reader).map_err(|e| e.to_string())?;
    if let Some(value) = &value {
        value.validate()?;
    }
    Ok(value)
}

pub(super) fn write_worker(writer: &mut impl Write, value: &WorkerMessage) -> Result<(), String> {
    value.validate()?;
    write_frame(writer, value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
