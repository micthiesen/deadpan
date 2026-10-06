//! Local analysis annotations. Pure: no I/O, model runtime, media or database.
//!
//! Analysis proposes; it never edits. A transcript records where words were
//! heard in the Original's audio clock; speech activity records where its
//! pauses are; a tracking path records where a selected target was seen.
//! Consumers convert those exact times to picture or edit
//! coordinates and keep low-confidence words visibly approximate. Manual
//! corrections are kept separately from these rebuildable proposals and
//! applied over them.

pub mod activity;
pub mod corrections;
pub mod shots;
pub mod tracking;
pub mod transcript;

pub use activity::*;
pub use corrections::*;
pub use shots::*;
pub use tracking::*;
pub use transcript::*;
