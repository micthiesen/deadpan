//! Metered values shared by plan queries.

#[path = "audio_bound_query.rs"]
pub(in crate::plan) mod query;

/// A Repeat's gap recipe is a different operand from the Repeat's node output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BindingTarget {
    Node(usize),
    Gap(usize),
}
