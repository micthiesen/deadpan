# Indexed current Repeat rate validation proposal

Scratch only. No checkout writes, Cargo, or runtime claim.

`indexed-current.patch` changes the owned helper/re-export/tests. `caller.patch`
is a separate proposed one-call-site integration for root's file.

The first parameter becomes a Copy `RepeatRateContext<'a> { document:
&'a ProjectDocument, layout: &'a FrozenAudioLayout }`. Both borrows must describe
one pre-edit document; root already captures and caches that layout once. The
historical `layout` parameter remains separate and names the binding's timing.

Current validation spends indexed ancestry work from
`current.layout.scoped_repeats(current.layout.root(), owner, remaining)`.
DefaultGap must equal the declared Repeat and have zero enclosing Repeat axes.
Node uses must have exactly the declared Repeat axis. The separately computed
current branch must contain the node, proved by indexed
`scale_in_scope(branch, owner, remaining)`; its scale is not used as historical
units. Owned-gap positivity spends one lookup of exact cached
`current.layout.node_extent(branch)`. No recursive walk remains.

These query costs depend on ancestry depth plus existing compact run/override
and historical-template class costs. Snapshot construction remains once per
complete root query, outside the per-owner helper. Test adapters capture a
snapshot for each independent test invocation only.

The patch preserves nine existing tests and adds
`indexed_current_ancestry_work_does_not_scan_unrelated_owners`: 1,024 unrelated
root Holds before the target must leave this helper's work exactly equal to the
small graph's work. This assertion fails with the old full-graph DFS. Existing
owned zero-gap and million-play tests cover the replacement extent lookup and
compactness. No new carrier/crop semantics, play enumeration, or ClockScope
routing changes.

The prior seven-argument production signature had an unnecessary Clippy
`too_many_arguments` expectation. This patch removes it.
