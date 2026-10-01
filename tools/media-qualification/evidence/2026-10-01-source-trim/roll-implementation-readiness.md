# Roll implementation readiness

Read-only planning, 2026-10-01. This supplements `next-roll-design.md` after the
editorial-edge correction. No checkout edits or runtime execution. Full spec
§§6.3–6.4 and §7.7 scope remains required.

## Minimal core extraction

Add internal `source_edit/edge.rs`; retain public Trim names and wire shape.
Move these functions from `source_trim.rs` without changing behavior:

- `limits` → `edge_limits(&Admission, SourceTrimEdge)` returning the existing
  two `SourceTrimLimit` values. Make their `minimum_integer`/`maximum_integer`
  helpers crate-visible; do not duplicate strict-bound rounding in Roll.
- `candidate` → `edge_candidate(document, admission, edge, applied)` returning
  a named `SourceEdgeCandidate`: `allocation`, `effective`, `window`, `prefix`,
  `source`, `needs_wrapper`. Move `translate_video`, `translate_audio` and
  `translate_range` with it. The zero candidate retains exact original mapping
  representation and needs no wrapper.
- Derive `needs_wrapper` in this shared result from the current direct-Source
  test in `resolve`; an existing Partition always keeps its identity.

Leave `source_trim::resolve`, its unary output-duration check, `root_operation`
and target/suffix timing windows in Trim. Roll must not call two full Trim
resolvers or reducers: their temporary total durations, independent clamps and
reanchors are wrong for a fixed-duration pair.

`source_roll` admits both captured literal adjacent siblings against the same
document with `source_edit::admit`. Intersect left-Out and right-In exact limits,
including strictness and limiting side, then clamp once. Pass the same applied
integer delta to both edge candidates. Construct only final outputs:

```text
old pair [T,J), [J,U)
new pair [T,J+d), [J+d,U)
```

The shared candidate retains fractional padding, hidden W where permitted,
full measured spans, independent audio offset and dormant linked audio. It
already rejects FitBeat and unsupported picture lead/tail. It grows physical
owners only. Left Out can grow a tail; right In can add a prefix. Neither
requires a hypothetical `document.duration() + d` intermediate check.

Use `SourceRollResolution` with per-side candidates/admission identities and
pair seam/range, requested/applied delta, exact bounds and clamp side/reason.
Do not attach unary root operations or suffix timing windows to its report.

## Atomic reducer and completed edge intent

1. Validate pair, applied nonzero delta, command metadata and required identities
   before changing any state. Use the ordinary outer command transaction path.
2. Extend `RootSoundEditCapture::prepare` with the fixed-duration MoveRange
   branch for Roll. Resolve the pair, detach exact sound events/routes, retain
   total duration and restore them after final structure. `Replace`, even with
   equal lengths, removes sound support and is unsuitable. Keep the existing
   outer `SoundAllowanceEdit` capture/restore path; it already captures generic
   commands, including permissions on unchanged Hold/gap issuers.
3. Call `capture_unbound_audio_bindings` once on the old soundless tree. Install
   that state once. Existing lattices, resumes, chronological reanchors and
   frozen layouts survive. Do not use Trim's `capture_timing`, phase-only
   layouts, suffix-owner traversal or appended reanchor steps.
4. Install both final Sources/allocations. Reuse Trim's small physical-owner
   installation block, either as a narrow internal helper or explicit initial
   code: preserve framing's old owner domain on duration growth, translate
   gain/mute keys for a right prefix, and rebase only that right owned binding.
   Retained content has the same absolute transform: right start moves +d while
   its physical allocation start also moves +d; any prefix cancels likewise.
5. After both allocations exist, call `source_edit::mark_edges` once on the
   final left allocation with `{start:false,end:true}`. Literal adjacency means
   its incident neighbor is exactly the final right allocation. This marks
   left end and right start, preserves policies and all outside edges, and
   needs no neighbor wrapper. Neutral raw support remains transparent. A
   marked owner is retained by subsequent Split under the completed lifecycle.
6. Reconcile lineage and transform marks once from old to final structure with
   `transform_marks_with_source_prefix` for the right physical Source. Left
   never prepends. Physical content-point anchors gain that prefix; Source PTS,
   edge sentinels and unresolved bindings retain their semantics. Existing
   reconstruction handles crop loss for ancestor/occurrence anchors. Physical
   Source-local/PTS marks may remain stored Bound while occurrence queries are
   unavailable; do not force stored crop loss. Then prune/validate and let the
   outer transaction restore root sounds/allowances and form one inverse.

### Identity bounds

Keep explicit `left_wrapper`/`right_wrapper` options, but at most one is needed:
only the contracting direct Source needs a fresh Partition. Existing Partitions
need no new ID. Enforce exact option presence, freshness and the final node
budget; do not accept unused IDs. There are no split, clone, mark or Repeat
identity pools. One `AudioTimingId` suffices; its allocation must equal the new
revision. Use existing capture freshness rules and retain no new timing when
every recipe is already bound. No ordinal range or successor arithmetic is
needed. Zero previews still validate request metadata and both receipts, then
return no transaction; raw zero authoring fails.

## Native draft considerations

The current `project::slip::{Target,Proposal,Prepared}` and service draft provide
the correct session/revision, saved-receipt and exact-proposal pattern. A Roll
target adds explicitly captured left/right children and ranges, including a
captured missing neighbor. Prefer the selected child's Out seam with its literal
next sibling for the first native Roll entry; display both sides. Never infer a
replacement neighbor from a cursor or late service reply.

If §7.7 Tab retains adjustments, it changes only the active editing operation.
It must not commit a unary Trim/Slip/Roll or reset earlier adjustments. Preserve
one immutable entry document, target pair/absence, restoration state and draft
generation. Every refined candidate needs a new proposal identity, admitted
base, stopped boundary pictures and waveform for the complete draft. Clamp
reports describe applied movement on that complete candidate. Enter commits the
exact prepared command once; Escape discards every adjustment. Reuse the saved
receipt-before-refresh rule and invalidate old GPU/service results on any mode
or amount change.

**A combined authoring contract is required before retaining mixed adjustments.**
The three current scalar commands do not express a combined In/Out/Slip/Roll
draft as one transaction. Independent per-tab scalar amounts also do not define
operation order or clamping when handles change. The smallest honest approach
is backend Roll first, then a bounded typed draft resolver/reducer which replays
ordered accepted adjustments from the immutable entry state, computes the final
geometry, and performs one old-tree capture and final lifecycle transform.
Coalesce only consecutive same-operation adjustments; reversing across modes
cannot silently reorder prior intent. Any net-zero final draft returns the exact
entry document, without retained temporary growth, markers or identities.

Do not implement that draft by repeatedly calling public reducers against
intermediate documents: they capture new clocks, author seams and transform
marks/root sounds prematurely. Mixed ripple/overwrite changes require their
combined final timing/root-bus transformation before that UI promise is enabled.
This is a required subsequent design boundary, not permission to omit modes,
waveforms, outgoing/incoming frames or the final single-transaction workflow.

## Focused implementation witnesses

- Extraction leaves existing Trim resolution and serialized inverse tests
  unchanged; Roll adds strict fractional shared limits and a large total-duration
  case that would fail an artificial unary extension.
- Signed-origin VFR frames at both sides of J+d; root duration and suffix exact
  identities unchanged; right prefix/left tail, dormant audio and all four
  direct/Partition combinations.
- 30000/1001 absolute seam transfer (3→4 transfers 1601 samples), 44.1 kHz source,
  independent offset, existing resumes/reanchors and one old capture. Compare
  raw PCM separately from both new editorial ramps, preserve explicit Hard,
  test tiny/fractional incident voices and chunk independence.
- Seam biases and multiple mark bindings, physical prefix, stored/query crop
  distinction, exact inverse; unchanged root event/route/allowance objects.
- At most one wrapper, all-bound no extra clock, node/capture limits, either
  receipt rejection, zero/stale requests, one durable history step and reopen
  Undo/Redo. Future mixed-mode tests must include mode switching with pending
  replies, clamp then reverse, net-zero restoration and saved refresh failure.

## Root decision after native draft review

Use bounded typed accepted In/Out/Slip/Roll state for the later combined draft.
This supersedes the ordered-adjustment/event-log suggestion above. Preserve each
operation's meaning even when two intents produce the same visible geometry.
A nudge resolves against the full accepted state and retains only applied values,
so reversing after a clamp responds immediately. Re-evaluating that same state
is deterministic; different gesture orders can encounter different clamps.
The complete timing, binding, root-sound and mark algebra remains required before
this mixed-mode UI can be implemented. See `native-trim-draft-review.md`.
