//! Authenticated, bounded local transport for the live project writer.
//!
//! Authentication is possession of a private discovery secret in the current
//! user's filesystem namespace. This makes no OS peer-credential or sandbox
//! claim. Semantic requests and reliable command receipts belong to the owner.
//! One connection carries one request and one reply, with no implicit retries.

use deadpan_store::ProjectStore;
use deadpan_store::host_owner::{PackageIdentity, WriterOwnerHandle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};
use uuid::Uuid;

mod client;
mod namespace;
mod server;
mod wire;

pub use client::Client;
pub use server::Endpoint;

pub const VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES + 4096;
pub const MAX_CONNECTIONS: usize = 4;
pub const MAX_BUFFER_BYTES: usize = 256 * 1024 * 1024;
/// Every admitted connection keeps enough capacity for a compact receipt even
/// when another reply or an incoming frame consumes the ordinary data budget.
pub const FALLBACK_REPLY_BYTES: usize = 16 * 1024;
const DATA_BUFFER_BYTES: usize = MAX_BUFFER_BYTES - MAX_CONNECTIONS * FALLBACK_REPLY_BYTES;
/// Each poll performs at most this much socket I/O across all connections.
pub const POLL_BYTES: usize = 256 * 1024;
pub const POLL_SYSCALLS: usize = 32;
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Errors contain fixed diagnostics only. Never include secrets, peer bytes,
/// filesystem paths or a nested OS error in a transport error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(deny_unknown_fields)]
#[error("{code}: {message}")]
pub struct HostError {
    pub code: String,
    pub message: String,
}
impl HostError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    fn io() -> Self {
        Self::new("HostIo", "Local host transport I/O failed")
    }
    fn invalid() -> Self {
        Self::new("HostProtocol", "Invalid local host control frame")
    }
    fn unavailable() -> Self {
        Self::new(
            "HostUnavailable",
            "The discovered project owner is unavailable",
        )
    }
    fn stale() -> Self {
        Self::new(
            "HostOwnerChanged",
            "The project owner or its private namespace changed",
        )
    }
    fn limit() -> Self {
        Self::new("HostLimit", "Local host transport capacity exceeded")
    }
    fn timeout() -> Self {
        Self::new("HostTimeout", "Local host transport deadline expired")
    }
    fn uncertain() -> Self {
        Self::new(
            "HostOutcomeUnknown",
            "The request may have executed; do not repeat the mutation without inspecting its receipt",
        )
    }
}

/// A connection-local authority that cannot be restored from wire data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionTicket {
    endpoint: Uuid,
    serial: u64,
}

pub struct Incoming {
    pub ticket: ConnectionTicket,
    pub request_id: Uuid,
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    owner_id: Uuid,
    secret: String,
    package_identity: PackageIdentity,
    request_id: Uuid,
    payload: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u32,
    owner_id: Uuid,
    package_identity: PackageIdentity,
    request_id: Uuid,
    result: ResponseResult,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum ResponseResult {
    Ok { payload: Value },
    Error { error: HostError },
}

#[cfg(test)]
mod tests;
