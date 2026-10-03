//! Durable typed copies restored by the project service, without media handles.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{AssetId, ProjectId, SourceQualificationId};

use super::slice;

#[derive(Clone, Debug)]
pub struct Bank {
    pub session: u64,
    pub project: ProjectId,
    pub version: u64,
    /// The double quote names the default copy; a through z name saved copies.
    pub entries: BTreeMap<char, Value>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinals: Range<u64>,
    },
    Edited(Arc<slice::Captured>),
    Macro(Arc<deadpan_core::SemanticProgram>),
}

#[derive(Clone, Debug)]
pub struct OriginalRequest {
    pub id: slice::CopyId,
    pub register: Option<char>,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub ordinals: Range<u64>,
}

#[derive(Clone, Debug)]
pub struct OriginalUpdate {
    pub id: slice::CopyId,
    pub result: Result<(), String>,
}
