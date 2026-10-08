//! Generation worker protocol over the shared bounded process supervisor.
//!
//! Existing generation callers keep the same process and event types. Protocol
//! versions, identities and terminal message semantics remain generation-owned.

use std::io::{Read, Write};

use crate::process::{ResponseKind, SupervisedProcess, WorkerProtocol};
use crate::protocol::{
    HostMessage, MessageIdentity, ProtocolVersion, WorkerMessage, read_worker_message,
    write_host_message,
};

pub use crate::process::{LogTail, ProcessLimits, ProcessSpec, SupervisorError};

pub type WorkerProcess = SupervisedProcess<GenerationProtocol>;
pub type ProcessEvent = crate::process::ProcessEvent<WorkerMessage>;

/// The strict generation adapter, separate from other worker wire protocols.
pub struct GenerationProtocol {
    identity: MessageIdentity,
    protocol: ProtocolVersion,
    cancel_message: HostMessage,
}

impl WorkerProtocol for GenerationProtocol {
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Model;

    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(request: &Self::Request) -> Result<Self, SupervisorError> {
        request
            .validate()
            .map_err(|error| SupervisorError::Request(error.to_string()))?;
        match request {
            HostMessage::GenerateHold {
                protocol,
                identity,
                cancellation_token,
                ..
            }
            | HostMessage::GenerateBridge {
                protocol,
                identity,
                cancellation_token,
                ..
            }
            | HostMessage::GenerateExtension {
                protocol,
                identity,
                cancellation_token,
                ..
            } => Ok(Self {
                identity: identity.clone(),
                protocol: *protocol,
                cancel_message: HostMessage::Cancel {
                    protocol: *protocol,
                    identity: identity.clone(),
                    cancellation_token: cancellation_token.clone(),
                },
            }),
            HostMessage::Cancel { .. } => Err(SupervisorError::Configuration(
                "initial request must generate a hold",
            )),
        }
    }

    fn cancellation(&self) -> Self::Request {
        self.cancel_message.clone()
    }

    fn write_request(writer: &mut impl Write, request: &Self::Request) -> Result<(), String> {
        write_host_message(writer, request).map_err(|error| error.to_string())
    }

    fn read_response(reader: &mut impl Read) -> Result<Option<Self::Response>, String> {
        read_worker_message(reader).map_err(|error| error.to_string())
    }

    fn classify(&self, response: &Self::Response) -> Result<ResponseKind, String> {
        response.validate().map_err(|error| error.to_string())?;
        if response.protocol() != self.protocol {
            return Err("worker response protocol differs from this attempt".into());
        }
        if response.identity() != &self.identity {
            return Err("worker response identity differs from this attempt".into());
        }
        Ok(match response {
            WorkerMessage::Completed { .. }
            | WorkerMessage::CompletedBridge { .. }
            | WorkerMessage::CompletedExtension { .. } => ResponseKind::Completed,
            WorkerMessage::Failed { .. } | WorkerMessage::Cancelled { .. } => {
                ResponseKind::Terminal
            }
            WorkerMessage::Stage { .. } | WorkerMessage::Progress { .. } => ResponseKind::Progress,
        })
    }
}
