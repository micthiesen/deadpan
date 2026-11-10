# Incremental render-plan fragments (2026-11-10)

Specification §17.1 asks plans to cache structural fragments and a small edit
to recompile affected ancestors rather than the whole project. This record
covers DP-16.4: `RenderPlan::compile_from(previous, document)` reuses node
fragments of an earlier plan, recompiles only changed nodes, re-derives the
prefix entries of each Sequence above a resized child, and produces a plan
identical to `RenderPlan::compile(document)`.

## Design

A compiled plan is a vector of node fragments (`PlanNode`) in node-identity
order plus whole-plan indexes. Each fragment is a pure function of:

- the node's authored value (`BeatNode`: kind, label, edges, treatments,
  framing, cutaways, captions);
- its own derived Retime clock and its Repeat play and gap overrides;
- the document's presentation basis (frame rate) and asset catalog, which
  Hold, cutaway and Source audio compilation read;
- its own recipe duration and extent;
- the duration and extent of each direct structural child and override root
  (Sequence prefix entries and Repeat layouts read nothing deeper);
- the positions of its children, which follow sorted identities.

Fragments are now `Arc<PlanNode>`, and every plan keeps its inputs
(`FragmentInputs`: a reference-counted handle to the document's node map from
the new `ProjectDocument::shared_nodes`, plus derived clocks and overrides).
`compile_from`:

1. Refuses all reuse when the basis, the asset catalog or the exact/integral
   extent mode differ (`Reuse::new`).
2. Matches old and new nodes with one merge of their sorted identities, so
   every later test is positional, never a map search.
3. For each new node, reuses the old fragment only when the identity existed,
   the node value is equal (skipped when both documents share one node map
   allocation), the derived clock and overrides are equal, its own size is
   equal and every structural child kept its identity and size. A reused
   fragment is shared as is, or copied with renumbered child positions when
   identities were inserted or removed before them.
4. A Sequence whose node is unchanged but whose children's sizes changed (an
   ancestor of a resized edit) keeps every other value, takes its new size and
   re-derives its prefix entries from zero with the new child extents. Leading
   entries before the first changed child are equal by construction and kept.
5. Every other node compiles afresh through the same `compile_node` the full
   path uses.

The whole-plan passes still run in full on every compile: the parent vector,
the definition index (whose absolute definition offsets change after any
earlier edit), sound-clock correspondences, delivery forests, root sounds,
the frozen sound-clock layout and the plan-wide audio flags. Plans hold no
root sample allocation: `RenderPlan::audio` rounds absolute project-frame
endpoints once per query, so nothing absolute is shifted from an older plan.

`previous` may be any plan; reuse rests only on equal inputs. Plans built
from frozen audio layouts carry no fragment inputs and are never reused.
`reused_fragments()` reports how many fragments were shared (re-derived
Sequences are not counted). It is excluded from the plan's value.

The native workspace refresh (`project::service::snapshot`) and `perf edit`
compile each new head from the previous workspace plan inside the head's
validation scope.

One unrelated change lowers both paths: the definition index read every
Hold's authored node with a map search; it now walks the compiled and
authored nodes together in identity order.

## Equivalence evidence

`deadpan_plan::with_full_compile_oracle` makes every `compile_from` also
compile from scratch and panic unless the complete `Debug` renderings of both
plans are equal (every field, including indexes, layouts, sound caches and
fragment inputs; the definition index's identity token renders as `()`).

- `crates/deadpan-plan/tests/incremental_plan.rs`:
  - six seeded runs of 60 random real core commands (Split, Rename,
    SetHoldDuration, WrapRepeat, SetRepeat, InsertPlays, Delete, Move, Group,
    SetRetime, SetAudioEdge) over a document with nested groups, a Repeat
    with gaps and a Retime. Each admitted edit (at least 100 are required;
    277 were admitted) compiles from the previous plan, then Undo (the
    inverse patch) from that plan, Redo from the undone plan, the edit from a
    random older plan and from an unrelated initial plan, all under the
    oracle;
  - a label edit reuses every fragment except its own node, and a resized
    Hold recompiles only itself while its group and the root re-derive
    entries;
  - growing one root child and shrinking another by the same amount keeps the
    root's node and extent and still re-derives its entries;
  - the oracle comparison detects a changed edge policy that keeps every
    duration.
- Every native workspace refresh in `deadpan-app` tests runs under the oracle
  (`compile_from_previous` in `project/service.rs`), so the app's service,
  semantic, Repeat, Retime, derived-clock, sound, generation and render test
  sequences all check the incremental plan against a full compile.
- Mutation checks, run once by hand against the final reuse code: removing
  the node-equality test, disabling the Sequence child-size test, ignoring
  renumbered positions in the stable-Sequence shortcut, or summing old child
  extents in re-derived entries each makes the new tests fail through the
  oracle (an earlier revision also failed when sharing fragments without
  renumbering). Weakening the Repeat default-child size test is not detected:
  a Repeat's own extent changes with its child unless every play is
  overridden. The check stays as a conservative guard.

## Measurements

Release binaries on this Mac (AC power, no thermal or performance warning),
built from `ae5f82a8b` (base) and from the working tree of this change (new),
in separate target directories:

| Binary | SHA-256 |
| --- | --- |
| `perf` base (`ae5f82a8b`) | `896a60db4e08f1de643695adcce06e82461f7d7054657d5fbf295680fca26c50` |
| `perf` new | `27132f42210b7b1e0eac8954c9f3f69be8ccf0c51c19444b1d9bc10f15d99da0` |
| `deadpan-cli` (fixture import) | `a200ad3b7d5703844a2d5436d3d5707a98c8c58d569edb92d91360751b26475d` |

The measured new binary differs from the committed source only by a lint
rewrite (`% 2 == 0` to `is_multiple_of(2)`) in `perf edit`'s alternation.
One-minute load was 2.50 to 2.68 at every stage start and end (the suite's
quiet threshold is 3.0). Caches were warm; each stage used a fresh private
package copy. Fixtures: the 10,000-Hold package from `perf make-large`, and a
real-media package imported with `project create-original` from the suite's
1080p60 120 s H.264/AAC recipe (`testsrc2`, x264 `veryfast`, BT.709 limited).
The user's existing packages predate the current schema and were refused.

`perf edit --cycles 30` (30 committed edits per kind). `workspace_refresh_ms`
is the head snapshot plus plan compile, as the native workspace refresh does;
base compiles from scratch, new from the previous plan. The last two columns
are both compiles of the same new head in the new binary, after the measured
edit, in alternating order:

| Fixture, edit | Base refresh p50 / p95 | New refresh p50 / p95 | Full compile p50 / p95 | Incremental p50 / p95 | Fragments reused p50 |
| --- | --- | --- | --- | --- | --- |
| 10,000 Holds, Split | 4.010 / 4.515 ms | 2.446 / 2.529 ms | 3.607 / 3.925 ms | 2.161 / 2.287 ms | 10,092 |
| 10,000 Holds, pause | 4.206 / 4.589 ms | 2.499 / 2.661 ms | 3.751 / 3.932 ms | 2.227 / 2.364 ms | 10,095 |
| 10,000 Holds, Repeat wrap | 4.277 / 4.964 ms | 2.375 / 2.573 ms | 3.600 / 3.869 ms | 2.110 / 2.295 ms | 10,099 |
| 10,000 Holds, Undo | 4.124 / 4.473 ms | 2.492 / 2.715 ms | 3.698 / 3.907 ms | 2.161 / 2.320 ms | 10,099 |
| 1080p60 Original, Split | 0.077 / 0.097 ms | 0.056 / 0.074 ms | 0.030 / 0.055 ms | 0.013 / 0.025 ms | 71 |
| 1080p60 Original, pause | 0.082 / 0.103 ms | 0.066 / 0.080 ms | 0.034 / 0.057 ms | 0.016 / 0.024 ms | 73 |
| 1080p60 Original, Repeat wrap | 0.080 / 0.102 ms | 0.061 / 0.074 ms | 0.032 / 0.055 ms | 0.014 / 0.025 ms | 77 |
| 1080p60 Original, Undo | 0.076 / 0.102 ms | 0.054 / 0.077 ms | 0.032 / 0.058 ms | 0.014 / 0.024 ms | 77 |

On the 10,000-beat package the refresh falls by about 40% (1.6 to 1.9 ms per
edit) and the median edit total from 16.1 to 14.6 ms (Split). The real-media
package grows from 2 to about 80 nodes over the run, so its absolute times
are small.

`perf scale` (in memory, outside any validation scope, so each compile also
derives durations; two rounds, five samples each, ten alternating pairs for
the split rows). The split rows compile one interior Split of the middle root
beat from scratch and from the unedited plan:

| Document | Base full p50 | New full p50 | Split, full p50 / p95 | Split, incremental p50 / p95 |
| --- | --- | --- | --- | --- |
| 10,000 Holds | 4.562, 4.608 ms | 4.610, 4.567 ms | 4.222 / 4.607 ms | 3.099 / 3.439 ms |
| 10,000 Original Source copies (real asset) | 7.713, 7.723 ms | 8.206, 8.388 ms | 8.207 / 8.438 ms | 5.430 / 5.893 ms |
| 1,000 Original Source copies | 0.706, 0.752 ms | 0.778, 0.771 ms | 0.742 / 0.791 ms | 0.471 / 0.505 ms |

A from-scratch compile of Source-heavy documents is 3 to 10% slower than
before (one shared allocation per fragment and captured fragment inputs);
it now runs only when no previous plan exists (opening a project). The
10,000-Source `perf make-long` package could not be measured by `perf edit`:
its per-cycle JSON size probe exceeds the 64 MiB document limit.

## Remaining work

- Whole-plan passes still scale with the document: the definition index,
  parents, sound-clock caches and plan-wide flags. In a validation scope they
  took about 0.8 ms of a 2.0 ms incremental compile at 10,000 Holds in an instrumented release run. A change
  to them needs its own equivalence proof.
- Reuse is against one given plan, keyed by node identity and value
  equality; there is no content-hash fragment cache spanning unrelated
  revisions, and identical subtrees under different identities are compiled
  separately.
- Attachment invalidation (sounds, captions, cutaways as separately cached
  fragments) and persistent per-node audio caches remain open; root sounds
  and sound-clock structures compile in full.
- Store hosts outside the native workspace (generation, boundary
  replacement, macro planning) still compile from scratch.
