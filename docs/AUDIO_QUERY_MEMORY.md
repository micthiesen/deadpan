# Audio query ownership and memory

Routed preparation can read a borrowed `PreparedRoutedRootQuery` repeatedly.
The owned entrypoint delegates to the same processing loop. Borrowing retains
source admission, dependency observation, cancellation, preparation limits,
sample labels, suppression and relative depth. It does not establish permission
to reuse media without re-admission.

`AudioRoutedRoot`, `AudioSourceOccurrence`, `AudioSourceOccurrences` and
`AudioProcessingQuery` expose `allocated_bytes_bound(maximum_work)`. The result
contains accounted owned heap bytes and the traversal work actually performed.
Core `SoundRoute` supplies the corresponding private-arena accounting.

The field visitors include vector capacity, nested elements, boxed payloads,
aligned Arc headers and payloads, identifier text, projection/tape/mix graphs,
owner paths, route grids/windows and private ripple/index storage. Shared Arcs
are conservatively counted at every owning edge. Checked arithmetic and work
and depth bounds stop overflow or unbounded traversal. Borrowed `RenderPlan`
storage is excluded. A cache that extends a plan's lifetime must account for
that pin separately and coordinate it with plan pruning.

These are bounds on requested Rust storage, not measurements of process RSS,
allocator bookkeeping, native DSP workspaces or decoder caches. They describe
an existing retained graph. They do not establish a constructor's transient
peak or reserve memory before construction. A future cache needs those
additional proofs, must share existing residency limits, and must keep pinned
entries charged until their allocations are no longer retained.

For a prepared historical query cache, repeat static preflight on each hit
before any media access. Validate exact plan membership and requested bounds;
preflight the complete retained input even for route gaps; and preflight each
stored raw processing query into the current request's projected/intrinsic and
dependency ledgers. Reuse the existing preflight helpers rather than replaying
saved work totals. The ordinary reader must still admit actual source bytes.
Projected-domain queries currently discard some policy metadata during
construction, so they need a separate replay design before caching.

See [the groundwork qualification](qualification/audio-query-groundwork-2026-10-26.md).

Clock discovery uses `source_voice_occurrence_supports` for ordered paths and
exact processed sample bounds. These planning-only results do not retain ordinary
provider, gain-owner or tape graphs and cannot authorize PCM access. Full voice
construction remains mandatory before media. Preserve candidates retain the full
constructor's validation with the same path and budget; ordinary candidates share
its scalar support and checked remap arithmetic. This is not a retained-query
cache. See [support discovery](qualification/occurrence-support-discovery-2026-10-27.md).

Historical sound binding can reuse structural admission when its compiled plan
holds the exact same immutable frozen-layout allocation. Each call still checks
processing capability, frame rate, selected asset contract and source support;
detached-equal layouts receive full validation. After-birth history can consume
the caller's checked binding after exact plan/scope/recipe checks and complete
journal validation. Both paths charge work they actually perform. This retains
no additional plan or query storage and grants no media-admission shortcut.
See [binding reuse](qualification/sound-binding-reuse-2026-10-27.md).
