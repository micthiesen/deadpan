//! Generated-media compatibility surface over the shared byte-object store.
//!
//! The public names remain stable because callers deal in generated artifacts;
//! containment, hashing, publication, and immutable snapshots are implemented
//! once in the crate-private object-storage layer.

pub use deadpan_core::{GeneratedContentId, GeneratedObjectRef};

pub(crate) use crate::object_storage::GeneratedStorage;
pub use crate::object_storage::{
    ObjectLimits as GeneratedMediaLimits, ObjectStorageError as GeneratedMediaError,
    VerifiedObject as VerifiedGeneratedObject,
};
