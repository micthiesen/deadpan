//! Ticketed named-snapshot operations on the project owner thread.

use deadpan_core::RevisionId;
use deadpan_store::takes::{Request as StoreRequest, TakeCatalog};

#[derive(Clone, Debug)]
pub enum Operation {
    List,
    Apply(StoreRequest),
}

#[derive(Clone, Debug)]
pub struct Request {
    pub ticket: u64,
    pub session: u64,
    pub revision: RevisionId,
    pub operation: Operation,
}

#[derive(Clone, Debug)]
pub struct Receipt {
    pub catalog: TakeCatalog,
    /// A saved restore remains acknowledged if refreshing the workspace fails.
    pub committed_revision: Option<RevisionId>,
    pub changed: bool,
    pub refresh_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub ticket: u64,
    pub session: u64,
    pub result: Result<Receipt, String>,
}
