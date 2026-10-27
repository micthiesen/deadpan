# Bounded intrinsic-processing discovery, 2026-10-27

`RenderPlan::audio_intrinsic_preparation` now classifies the immutable plan's
owned current/historical time-pitch processing without constructing processing
graphs, compiling historical layouts, resolving media or allocating storage.
This is an optional planning primitive. Audio does not call it yet; it supplies
no resource-fit, permission, source-admission or batching proof. General
sound-bearing Split and the full product goal remain open.

## Exact result and scope

`AudioIntrinsicPreparation` has three outcomes, each retaining actual work:

| Outcome | Meaning |
| --- | --- |
| `NotRequired { work }` | Every owned inventory was examined and no intrinsic Preserve/Shift input or processor was found. |
| `MayBeRequired { work }` | A current or retained operand can require processing; its reachability on the next read is unknown. |
| `WorkLimit { work }` | The next record was not examined. This is not a negative proof. |

Hosts must bill `work()` even when selecting scalar fallback. The query validates
existing `AudioQueryLimits` before any visit, charges each plan/layout/timing/node
record before inspecting it, and stops before exceeding its allowance. It builds
no spans or seen set and deliberately rescans duplicate layouts. Repeat plays
are never expanded. Cancellation/deadline checks belong to the caller around
this bounded query, consistent with existing Plan queries.

The scan covers all compiled nodes, including definitions/overrides, the plan's
own retained sound layout when present, and every binding timing layout. Later
sound-journal clocks and unrelated historical branches count conservatively.
Current nodes check both `preserve_input` and the exact compiled
`pitch.processes(scale == ExactRatio::ONE)` predicate. Frozen layouts reuse
Core's `retime_processes`, which checks integral recipes and effective derived
clocks. Transparent unity Preserve Partitions remain eligible; nonunity Preserve
and nonzero Shift do not. No authored-enum shortcut or historical compilation.

Source mappings and root sound delivery are affine sample maps, without an
independent pitch-preserving recipe. Automatic source-occurrence processing uses
the same structural Retime predicate. Arbitrary caller-created projected root,
signal or stage operands remain outside this query. Even after `NotRequired`, a
future cold Audio walker must inspect and reject unsupported actual projected
boundary forms before allocating extra histories. This is not a claim that all
projected PCM allocation is absent.

Eleven integration tests and one unit case cover ordinary/compact structures,
unity/nonunity policies, sparse overrides, owned input behind an effective unity
clock, current and retained inventories, all journal timings, exact/partial work,
invalid limits, routed sounds and derived-only frozen processing. The last case
uses an admitted frozen layout directly: current binding/context admission
refuses that derived Preserve form. The test preserves those refusals rather
than inventing a binding-valid document or weakening Core.

## Verification

Same qualified macOS/M5 Max/Rust environment and locked, opt-level-2 debug build
as the preceding Components checkpoint. Main guards 1,402 inputs; the isolated
published runtime guards 1,174. Only two exports/module-list files and two new
files change. The inherited Plan module list remains uncommitted in main; its
clean published patch adds only the same three module/export lines. The other
three task files match byte-for-byte between main and the isolated tree.

| Check | Result |
| --- | --- |
| Main combined Plan suite | 552 pass across 29 Cargo-inventoried, copied executables; zero failed/ignored. |
| Main library / new integration target | 102 / 11 pass, included in the combined total. |
| Isolated combined Plan suite | 539 pass across 29 pinned executables; zero failed/ignored. |
| Isolated library / new integration target | 89 / 11 pass, included in the combined total. |
| Strict check | `cargo xtask check --no-test`: formatting and strict all-target Core/Plan/Audio/Store lint pass. |
| Independent source review | No supported false-`NotRequired` finding in the declared scope. |

Each target's command, binary identity, result and output is retained. All runs
are terminal, with guarded sources and executed binaries unchanged before/after.
The main/all-target build takes 51.85 seconds and isolated build 50.56 seconds;
combined suites take 2.73/2.63 seconds. These are verification records, not a
performance comparison. No Audio/media/Split matrix, native ABI, GUI, packaging
or whole-product gate is rerun for this unused metadata API. Their prior results
remain historical evidence. No requirement or gate closes.

### Corrected setup evidence

Production query logic stays unchanged through all fixture corrections. The first
build finds two extra constructor arguments in tests. After correction, one of
eleven integration tests passes; ten fixtures fail Core's ban on media-less
Source nodes. Valid metadata-only Original assets fix that setup. The next run
passes nine; two fixtures still try to bind through an opaque Preserve boundary.
Moving the processor into a separate retained branch keeps the binding valid and
still tests conservative scanning of the entire owned table. All eleven then
pass. None of those fixes relaxes admission or a query assertion.

Early parent commentary and its relay in the source review mistakenly label a
previous combined Plan total as a 541-test library run. Actual library counts
are 102/89, as recorded per executable. The count correction is explicit in the
evidence, and both complete 29-target suites are subsequently run with the exact
552/539 totals above. Failed source snapshots, diagnostics and intermediate
results remain available rather than being overwritten.

## Continuation and evidence

The remaining policy carrier/visitor must obtain exact Original/sound/routed
suppression and exhaustion facts before PCM allocation, reserve its metadata,
preserve both callback orders and bill reconstruction. A cold resource fold
still has to account for all future retained outputs/tiles and recursive scratch
before simultaneous histories allocate. This capability query alone cannot
enable batching or certify a failing read's fit. Runtime graph reads remain
single-output and native general Split guards remain in place.

Raw records remain in `/tmp/deadpan-intrinsic-capability-2026-10-27`. The
[inventory](../../tools/media-qualification/evidence/2026-10-27-intrinsic-capability/inventory.json)
retains exact live/published patches, before snapshots, final pins/lineage,
independent review, all 29-target outputs for both trees, fixture failures and
the count correction. All 223 gzip payloads verify stored and expanded hashes:
3,000,159 stored bytes, 12,005,112 expanded bytes. Inventory SHA-256:
`a6eac23f267c6a7eb5a4415e705683f863dadc65ac3d390375c8101240499375`.
