//! Local analysis annotations. Pure: no I/O, model runtime, media or database.
//!
//! Analysis proposes; it never edits. A transcript records where words were
//! heard in the Original's audio clock. Consumers convert those exact times to
//! picture or edit coordinates and keep low-confidence words visibly approximate.

pub mod transcript;

pub use transcript::*;
