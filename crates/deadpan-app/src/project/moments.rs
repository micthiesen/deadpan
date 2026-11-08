//! Ticketed named-moment catalog operations on the project owner thread.

use deadpan_store::moments::{MomentCatalog, Request as StoreRequest};

#[derive(Clone, Debug)]
pub enum Operation {
    List,
    Apply(StoreRequest),
}

#[derive(Clone, Debug)]
pub struct Request {
    pub ticket: u64,
    pub session: u64,
    pub operation: Operation,
}

#[derive(Clone, Debug)]
pub struct Receipt {
    pub catalog: MomentCatalog,
    pub changed: bool,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub ticket: u64,
    pub session: u64,
    pub result: Result<Receipt, String>,
}
