# General Split sound research, 2026-10-26

This is an **incomplete research checkpoint**, not product qualification.
The production candidate remains uncommitted and native scoped Split remains
blocked. No acceptance criterion, requirement or delivery gate closes here.

The [retained collection](../../tools/media-qualification/evidence/2026-10-26-general-split-research/README.md)
contains the clean candidate patch against `c99d0404`, its nine new source files,
32 terminal development runs, original failures, source pins and exact filter
input captures. It reconstructs the final candidate, not every earlier failed
iteration. Executables are omitted from the published collection; retained local
binary identities and commands are recorded. All checks use locked dependencies,
Rust 1.97.1 and the selected qualified FFmpeg/ORT prefixes in the receipts.
The direct `core-sound-regression-fourth` run used the earlier third-run Core
binary; unchanged guards on that execution do not make it current-source evidence.

Independent audit verifies 175 payloads (9,484,953 stored bytes), all 152 gzip
expansions (31,105,960 bytes), 32 run/stream/count records, 23 reconstructed tracked
source paths and nine new source files. The final inventory SHA-256 is
`124fc436998367fa72dcf7cc213285b10aa014a15925813024b754a07e3eb397`.

## Implemented candidate

Complete-context Split proofs now cover composite owners, non-Sequence parents,
refinements and copied nested forests. An explicit stable-play isolation relation
preserves earlier sound clocks while keeping their ancestor processor shared.
Occurrence Split isolates before ordinary sound capture. Malformed relations,
unpaired groups and crossed/incomplete Split forests remain rejected.

Immutable RenderPlans cache exact sound-clock correspondence proofs. Their keys
include scopes, clocks, Repeat maps, groups, Split/isolation relations and the
owner pair where required. Every owner alias is checked. Compilation retains its
aggregate proof-work bound; queries charge retrieval work, and source admission
remains independent. The per-read work limit stays 16,777,216.

Memoryless gain processing skips validated empty treatment slices after applying
Repeat escalation, and charges only the stages actually evaluated. The original
trace found repeated whole-recipe charges for the escalation-only `0..0` call.
No limiter, query or preparation limit was raised. Temporary traces are removed.

## Verification and exact failures

- `core-plan-full-thirteenth`: **2,119 pass**, 98 suites, zero ignored. This is
  the broad Core/Plan candidate before the later immutable proof cache.
- `plan-full-postcache-seventeenth`: **505 pass**, 27 suites, zero ignored,
  including four new cache/budget/certificate/authority tests. These overlap the
  earlier broad run and must not be added to it as distinct test coverage.
- `strict-postcache-seventeenth`: strict Core/Plan/Audio all-target lint passes.
  The clean final candidate matches all **2,215** source pins from this run.
- `audio-full-split-postcache-seventeenth`: **two pass, five fail**. The pristine
  audible sound/EQ/saturation/wet-return/limiter witness and pristine read-partition
  control pass. The Source/Sequence first Split and processing-depth matrix fail
  exact authored PCM equality. Selected play, nested selected play and root-sound
  cases still exhaust preparation/query work. The cache does not resolve those
  remaining work failures.

The first Source/Sequence mismatch is stereo sample **5,977**: expected
`[0.94367766, -0.99999905]`, actual `[0.9436776, -0.99999905]`. Do not relax the
comparison. Exact f64 input capture proves a substantial missing contribution:
the root EQ tile starts at 5,120, reads input samples `[3,072, 8,192)`, and loses
the matching right-fragment sound from the Split seam **8,008** through **8,191**.
The Original-only input is byte-identical; with the sound permitted, its missing
contribution also changes shared nonlinear drive. EQ look-ahead exposes the
failure before the seam. The tiny first emitted difference is not evidence that
the defect is merely harmless floating-point variation.

The clean run reproduces the failure independently of tracing. Trace run sixteen
changed only a `cfg(test)` module-path annotation during execution; that module is
excluded from the ordinary dependency library. The receipt retains this exact
source change and the trace comparison's raw vector hashes.

## Remaining implementation

Delivered fragment continuity and processing-history scope are different
authorities. The Source case's two fragments keep narrow full-Source journals;
all three references have no retained Split alias. The external Partition forest
is intentionally outside that processing scope. Existing permission families also
require a historical alias for the current temporal context, excluding the root.
Simply widening the journal or removing that context guard would grant extra
processing history and is not an acceptable fix.

Implement a separate checked delivery-continuity proof that can join only already
prepared current fragment routes within the exact current context and tile. It
must preserve each route's support, source admission, silence permissions and
individual treatments. Cover complete forests, descendants, refinements, nested
plays, independent copies and later structural changes; do not infer identity
from labels or audio lineage alone. Resolve the remaining preparation-work
failures without raising limits. Store/history, native command and receipt
integration, macros, real emitted media and measured performance remain owed.
