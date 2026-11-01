# Projected PreserveTape mask destinations

Predecessor: `59874646dba6974da77e2cad08e9bc4822fd5afe`.

The actual routed projected Signal path now funds a new owned destination after
ordinary PreserveTape suppression completes. Its source Vec remains live through
destination reservation, copying, Box publication and final checks. The prepared
Arc remains live for dependency/depth handoff. Public route mapping funds its
separate destination through the existing owned-mask path.

The ordinary policy query, inventory and source Vec remain unadmitted. This
checkpoint does not establish whole preparation or grant product requirement or
Gate credit. General sound-bearing Split remains disabled.

## Runtime boundary

`read_projected_signal_controlled` retains the existing preparation, sampling,
policy query, dependency preflight, range validation and PCM-zeroing order. Only
after successful suppression does it pass the actual Vec to the existing native
`into_admitted` helper. That helper reserves the actual final range count and
future storage Box through the current mask context, copies while the source is
alive, publishes, checks control, then explicitly drops the ordinary source.
An empty result creates neither destination storage nor a mask account.

No Point policy factory, grid rule, endpoint setting, cap or canonical query fee
changes. Native destination work remains separate from Plan query claims. The
production path has no replay fallback. Test-only G replay composes with older
replay scopes; the preceding current Components capture bypasses only G.

## Immutable before evidence

The new corpus is
`crates/deadpan-audio/tests/fixtures/projected-policy-destination-baseline-v1/corpus.json`,
115,803 bytes, SHA-256
`93912d8a589912b7ef41f0bd523db8da2a6cdec0d2ea5db1b6ba6f9941dffa51`.
It was frozen before runtime integration. Its 34 predecessors remain exact in
both source copies.

The corpus separately names six prepublication F qualification captures and six
fresh postpublication F captures. Their literal work observations and distinct
full-body hashes remain evidence, without creating a cross-process work golden.
Portable references identify their archive records and exact digests.

The checked native fixture uses a verified WAV, explicit silent Hold, and actual
public Tape/projection/route constructors. All reads are valid 256-frame windows.
The positive window returns the literal `615..768` policy range with source
Vec/public Plain capacity one. Source-only windows return an empty mask with
capacity zero. Each SourceReady receipt records 768 request-local projected PCM
frames and zero projected disk bytes. Persistent renderer cache counts stay
zero, including shared-renderer calls; these runs do not prove cache reuse.

Both native3 bodies are 58,670 bytes, SHA-256
`9e2349a6948302c51872cb4669637f256c8521e182e607b0885bd02e3d9767ad`.
Their canonical bodies are 23,896 bytes, SHA-256
`d77865693996e74e6d1bee6a33510fde658a70e1f8919fc4cff67aea243b5ee9`.
Final-source G replay reproduces those bytes exactly in both copies.

## Active verification

| Final build2 | Main | Isolated shipping copy |
|---|---:|---:|
| Audio test targets | 21 | 21 |
| Passed tests | 1,140 | 1,134 |
| Library tests | 559 | 553 |
| Ignored manual entrypoints in the matrix | 27 | 27 |
| Known unfinished Split tests filtered | 7 | 0 |

Every final target was pinned from its actual optimized Cargo artifact and run.
Debug assertions and overflow checks remain enabled. All eight remaining checks
pass: Main workspace `cargo xtask check --no-test`, isolated all-target Audio
Clippy, both formatting checks, both rustdoc runs and both hygiene runs. Rustdoc
contains zero executable examples. No release bundle or model bundle was built.

Three real-context tests isolate the post-preparation destination boundary:

- The exact ceiling, computed from existing projected residency plus actual
  native shell and Signal payload Layouts, returns an owned direct Signal mask
  before public route mapping. Its allocation differs from the recorded source
  Vec, dependencies/depth remain correct, and actual Vec/Map/Storage debits are
  one each.
- One frame below that payload ceiling genuinely refuses after policy completes,
  with no accepted payload, Vec, Map or Storage allocation and no leaked payload
  charge. The admitted account shell remains as the controller's existing state.
- An empty mask succeeds at the existing 768-frame prepared residency without
  creating an account or destination work.

These are warm request-memo boundaries, not cold public-read minima. The held
positive mask survives releasing the extra prepared Arc, ReadWork, provider and
controller; dropping the mask releases its payload before the remaining observer
shell. Ordinary Plain source disposal follows the helper's explicit Rust drop
ordering. No physical allocator/deallocator observer or source lease is claimed.

The ordinary public comparison checks every captured case field, every public
block field and every f32 bit against the frozen corpus. Only the positive
`suppressed_owned` flag is expected to change. All four cases pass in every
complete current capture; ranges, capacities, provider observations and
SourceReady receipts remain literal. The SourceReady work values, 189/194, are
cumulative pre-copy checkpoints and are not isolated G copy fees.

## Complete capture results

All 18 overall capture invocations pass: six historical modes per copy and
three current repeats per copy. Five older complete bodies reproduce their
original hashes exactly. The preceding Components mode remains a variable-work
corpus rather than one byte golden.

Every current capture preserves all 744 native outputs, including 264 successes
and 480 errors, and all 42 internal/transfer result fields. Those 744 requests
are compatibility observations; they are not 744 demonstrations of this G
producer. The four added routed cases provide the actual G public-path evidence.

Each current body contains 477 completed interval claims and 954 reconciled
native/host lane equations. The existing claimless partial and five-unit residual
stay unresolved. No query debit is inferred from that residual.

Current canonical bodies are 38,554,371 bytes each:

| Repeat | Main SHA-256 | Isolated SHA-256 |
|---|---|---|
| active | `cef172ebf9b8dbb1aaed27b2874109aad65e7081c6939680d3dcaaa88dac3b60` | `ef140d066ce656acc84ad435006a999c9c32a62c7c1b497ad35048c870f10849` |
| active2 | `76cb20e891da99876bd42a344d6604293226d9439f113a305ef685dc46d0b79f` | `e14218c00bafcd25cbb0c2c5569dc87f68c4e95660eea408647c7b5769f7e3cc` |
| active3 | `e4da5e869e34d60cf75a1920402eee8e988c587aa9df0f03ea38f225871e8d56` | `5f2d334a350df29b27b411510f76b21b410eb2c4b1029b66cdf35feb398ab4e1` |

The exhaustive comparison records 16 intentional G metadata additions/changes
per body and 304 literal work differences versus F, on 80 paths, with maximum
absolute delta three. Across all 15 G pairs, it records 855 differences on the
same 80 paths, maximum absolute delta five. These occur only in the existing
Components `dynamic_positive` case's typed absolute work checkpoints. Every
value, path, context and delta remains in the audit. No other data differences
were found, and no F-specific path whitelist was carried forward automatically.

An additional independent local-debit audit checks all 9,348 G producer rows:
source initialization succeeds with exactly four Zero units and unchanged cache
work; recorded first/last accepted Marks each debit one unit; bit popcounts equal
actual writes. No current complete-body byte identity is claimed.

## Failures, scope and continuation

Rejected static fixture candidates, actual build1 constructor/digest errors and
the first observer-arity Clippy failure remain recorded with their corrections.
The initial analysis pin-check failure and its stderr remain recorded. Its
source bytes were not recoverable; the provenance says so, while preserving the
exact successful analyzer, command, output and supplemental audits. No failed
invocation is described as passing.

Independent baseline, corpus, source, lifetime and final capture reviews pass.
Portable archive references are checked against the final inventory before
publication.

Main's 2,932 and the isolated copy's 2,604 other initial named inputs remain
exact. The 10 owned paths match between copies; all 2,614 named isolated source
inputs are the shipping authority. Main's inherited WIP remains unpublished.

Ordinary policy source queries/inventories/final Vecs, nested override and
SourceVoice provider coverage, fractional/negative retained geometry, graph
construction, exhausted vectors, PCM/dependencies and whole preparation remain
outside this checkpoint. Next implement the private provider-aware child kernel
under the [reviewed source-admission design](../design/projected-policy-source-admission.md),
preserving its empty-provider and Source output-carrier ordering constraints.

[Evidence inventory](../../tools/media-qualification/evidence/2026-10-31-projected-policy-destination/inventory.json).
