# Scoped frozen-layout early fallback audit

## Finding

The hypothesized work path matches current source for a root Sequence containing 10,000 direct fresh Holds. capture_for_insertion_scoped first validates and walks the complete document, collecting binding placements. For each newly captured Hold recipe, capture_recipe stores the Hold's physical NodeId in the placement template; template_aliases inserts that physical ID into required. The root Sequence has no Hold recipe. In this shape, required therefore has 10,000 distinct Hold IDs while document.nodes().len() is 10,001.

FrozenAudioLayout::capture_scoped currently runs precharge, duration setup and a full node pass (frozen checks, parent/duration info and WireCount), an override/gap parent pass, exact wire-length calculation, ancestor closure, then provisional full-tree traversal/construction. The constructed result has all 10,000 required Holds plus the root, so nodes.len() >= document.nodes().len() returns None. Its production caller then invokes complete FrozenAudioLayout::capture(document). This confirms redundant scoped work by source inspection; it does not measure that work's cost or establish its share of pause latency.

## Candidate guard and proof boundary

A guard after precharge of required.len() >= document.nodes().len().saturating_sub(1) can return Ok(None) and let the existing caller perform the already-required full capture. For valid, admitted documents, aliases are distinct set members. If there are at least N−1 valid aliases among N physical nodes, at most one node is omitted. The scoped projection cannot reduce node count:

- Any retained descendant forces every ancestor, including root, into the closure. Thus an omitted node with a retained descendant is not possible.
- An omitted Sequence child is represented by a spacer (including zero-duration empty Sequence spacers); an omitted branch child of a non-Sequence owner also keeps its keyed spacer. Either case costs one output node.
- A one-node document uses a zero threshold and falls back even with no aliases. This is conservative and harmless.
- Unknown aliases are not evidence of N−1 valid aliases. The cardinality guard may fire for an invalid set, but both paths return to the complete capture at the caller. That preserves the command path's outcome.

The production caller treats None as a request for complete capture. Do not claim direct capture_scoped error equivalence unless the guard is positioned to preserve its errors too. To preserve prefix/refusal ordering for direct calls, place it after precharge, derived-clock fallback, retained-duration acquisition and the duration-count check. A guard immediately after precharge would skip a possible document.durations()? error on malformed/direct inputs. The caller validates first, but direct unit tests can exercise the scoped helper. The full layout capture remains authoritative for per-node frozen restrictions, Retime mapping, missing aliases and full wire-size refusal.

precharge must remain before the guard. It enforces frozen aggregate edge/run limits before cloning/indexing; existing test crates/deadpan-core/tests/audio_reference.rs::capture_enforces_the_frozen_only_aggregate_run_cap_and_accepts_its_exact_boundary demonstrates valid authored input that complete frozen capture rejects at the aggregate limit. Keep that refusal before fallback.

## Counterexamples and exact regression coverage

Add parity tests around audio_reference.rs::scoped_capture_matches_the_complete_capture_at_ten_thousand_children and the randomized scoped-slice property tests:

1. **Root and one omitted node:** require root plus all but one leaf; require all leaves but omit root (ancestor closure restores root); for both, assert scoped returns None at the threshold and the caller-equivalent fallback equals full capture, including exact wire bytes.
2. **Single root / empty Sequence:** N=1 with empty required set and with root required. Assert conservative fallback and exact complete result. Also test a root with one empty Sequence child: omitting that child still yields a same-ID spacer, so the constructed slice cannot shrink.
3. **Sparse Repeat branches:** create a Repeat with ordinary child, sparse play override root, and sparse gap override root. Require every valid node but one, choosing each branch root and a branch descendant in separate cases. Verify the full capture fallback preserves override/gap keys, all stable iteration identities and slices for every required alias. ProjectDocument::children explicitly includes these roots.
4. **Missing aliases:** supply N−1 set entries with one unknown alias. The direct scoped call and caller fallback must both produce the same full-capture result/error as before; malformed document validation must still win at capture_with_placements.
5. **Refusal order:** use a document whose individual Repeat runs are valid but combined frozen run count exceeds MAX_FROZEN_AUDIO_RUNS, plus a threshold-sized required set. Assert capture_scoped still returns LimitExceeded before fallback. Retain complete wire-count-vs-serialization equality and test a full-result fallback's JSON round-trip.

The key observable should be semantic/refusal equality, not an elapsed-time assertion. Existing compact capture tests cover proper small slices and should remain intact. This hypothesis predicts skipping the scoped passes on the all-but-one case; any latency gain needs a separately controlled run with phase timings and overhead accounted for.
