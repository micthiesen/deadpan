# Discontiguous whole-node selectors

Status: implemented and locally verified within the scope below. No requirement
or gate is newly complete. Results apply only to the recorded stage.

## Selection and one genuine union

The native select command admits a strict JSON list of 1 through 64 distinct
authored NodeIds in ordinary Your edit with Linked role. Members may have
different immediate Sequence parents. One bounded depth-first traversal resolves
canonical structural order, including distinct zero-duration siblings at the
same boundary. Missing, outside-scope, root/scope, duplicate and
ancestor-overlapping targets refuse. Repeat, Retime and partition interiors
retain their existing refusal.

Selection, recording, named calls and cut-dot retain the exact member addresses
and re-resolve each staged document. Cursor motion retains a finished set;
ordinary Visual selection replaces it. Native provenance checks the project,
revision, scope path, capture timing and every actual parent/child/range. A set
has no single enclosing range. No component is silently replaced by a selected
beat or cursor-derived target.

Yank captures one EditedSet from the entry document. Cut captures that union
before reverse-canonical ordinary DeleteRipple leaves run in one Compound.
Empty-only cuts preserve time while changing structure. One register value and
common immutable capture revision cover the full set; timeline Undo retains the
independent register value. Historical admission reconstructs every component.

Seam paste inserts one neutral editable Sequence with separate member identity
pools and one shared mark pool. Source-clock bindings spanning several members
remain one logical mark. Relationships that cannot close within supported
capsules refuse explicitly. The outer sound-clock journal has a dedicated
timing slot; imported member records cannot collide with its intermediate
snapshot. Aggregate node limits count authored nodes, with aliases bounded
separately.

SQLite schema 84 adds the closed edited_set register kind, while core document
schema stays 55. Prior unused development packages refuse before writes under
the owner's existing breaking-format authorization. This is not a migration.

## Public old-code controls

Base: f4f7481ed804f8a5c25b39b76b7ecc8bc1529cc4.

The complete 1,977-file base Rust set and six supplemental inputs were compared
directly with Git blobs. The baseline adds only the two public control tests:
1,979 frozen Rust files in total.

The structural fixture has Lead 3f, an ordinary outer Group with two empty
siblings, Source A 8f, an unselected 2f gap and an inner Group containing Source
B 8f, then Tail 3f. A reverse-listed set must resolve to canonical order from
an unrelated cursor, selected beat and Visual range. Existing individual
captures/deletions prove the actual parent/range and zero-time identities.
Separate ordinary register operations overwrite the bank and are not a union.

Only the final strict positive Nodes program admission fails on the old code,
with unknown variant nodes, in 0.009984s. Executed SHA-256:
3c95f3074116bab08dc56ce24a86b46e8839cc682d3d9ee68a3b155d84c74ff4.
After implementation the positive extension checks four canonical capsules
against the ordinary capture oracle, independent of the misleading invocation.

A separate public SetMark/Split fixture proves one logical Source-clock mark
with two physical bindings. Independent component pastes produce two logical
marks, while an ordinary whole capture preserves one mark. The baseline control
passes in 0.011532s. Executed SHA-256:
5c1dcc5b51c79557f98ca4707f8d70f941243ad68e45c6146e38351f922afc2e.
The union regression requires one copied logical mark with both bindings.

## Focused checks and retained failures

The focused core run passes all 11 cases: nine union capture/placement cases and
the two public controls. The corrected Store target passes four cases, the
spawned CLI target two, and the native target 46. These 63 checks retain exact
case inventories and executed binary hashes.

The first Store test expected empty authored children immediately under the
outer union group. Ordinary slice placement has neutral per-member wrappers;
the corrected assertion inspects the actual authored empty children within
those wrappers. The first app test double-escaped a Unicode JSON literal; the
corrected test exercises the intended decoded identity. Neither fix broadens
production admission.

Generated command-reference and routing-baseline rewrite failures are retained.
Routing generation needs a second pass because completion filtering reads the
compiled-in usage table. The final generated diff only adds Nodes selection
usage and its completion guidance; shortcuts are unchanged.

The first broad build exposes a missing PasteEditedSet arm in the core test
allocator. The second exposes preview/export-only helpers embedded in the shared
recipe module also compiled by golden_renders. The verification test moves into
the preview_export integration root, and its recipe builder becomes public.
A third build retains the intermediate private-builder error. These are compile
failures, not passing runs. The broad-run frozen source delta from debug eighth
contains only the two CLI test files affected by that relocation.

## Broader regression

The final broad archive builds 195 binaries and runs 4,570 selected cases:
all 4,570 pass in 240.381s at eight-thread concurrency, with 781 filtered or
ignored cases skipped. The selection covers Core, Plan, Audio, Store and native
app plus affected CLI semantic/register/selection cases. Its exact inventory
includes all new Core/Store/CLI union tests. All 1,990 Rust files, six
supplemental inputs, 160 selected test executables and five helpers remain
byte-identical during execution. The driver duration is 245.145s, including
post-run identity checks; it is separate from Nextest's test interval.

Strict checking then catches one needless clone in a Store test and one
boolean native allow-list match that Clippy requires as matches!. The Store
test uses a borrowed one-element slice; the native rewrite retains exactly its
accepted alternatives and false default. Earlier frozen broad/UI reports remain
attributed to their own candidates. The four affected Store cases pass again;
strict checks, 46 affected native cases and a fresh full UI suite are recorded
separately for the final candidate. All 1,273 selected UI-feature cases pass in
69.326s, including the four headless integration cases; eleven ignored cases
are skipped. The 46 affected normal-app cases pass in 0.211s. The changed-package
xtask check, UI-feature all-target Clippy with warnings denied and formatting
all pass. Frozen source, six inputs and executed binaries/helpers remain exact.

## Native workflow and layout

The first four debug replays retain fixture/assertion failures: duplicate uses
a neutral wrapper until public Ungroup promotes the empty sibling; unrelated-node
comparison must exclude selected descendants; Undo produces a new revision;
and Paste intentionally clears dot under the existing repeat contract.
The corrected replay tests cut-dot before independent paste. No production
dot contract changes.

The fifth replay passes 140 workflow checks, but visual inspection exposes
elided exact IDs and unrelated single-beat widgets. A full-width wrapping,
read-only set inspector fixes those issues. The sixth passes 143 checks,
including actual paint bounds for a complete member and explicit selected-loop
refusal. A seventh build catches an incorrect LastEdit method name in the new
set footer; the eighth passes all 143 checks across 520 frames plus Kestrel.
Three layout warnings remain in that report.

Independent review fixes two authority bypasses: saved framing could use the
navigation beat, and a set ending in a Hold could retain single-Hold AI
authority. Native regressions require no bank/document writes, no AI request or
preparation, and refusal of an old captured AI target. Global AI cancellation
remains available. A final follow-up review finds no actionable regression in
AI, playback, inspector or selection retention.

Loop selection also refuses a set instead of falling back to its navigation
beat and including gaps. Ordinary cursor playback and separate sound audition
retain their existing domains. The inspector lists member ID, actual parent,
range and empty state; the footer teaches union copy/cut and clearing the set.
The debug eighth compact capture was inspected.

Both final release modes pass 143 workflow checks plus Kestrel: visual uses
546 frames and performance 677. The release executable SHA-256 is
6e728a988c6804ea02dfa4d188dcb179c295f8557a1591108a9ddb66636ea06a.
Quiet input-to-state p99 is 1.977292ms over 147 samples. Commit p99 is
85.749541ms over 13 samples, and input-to-picture completion p99 is 94.499416ms
over 40 samples. These small-scenario measurements do not qualify maximum-size
sets, physical display latency or full product performance. GPU readback is off
in the timing run, with power/thermal state uncontrolled.

Both reports retain 39 second-retry frames and three runs of consecutive
retrying frames with distinct causes. Visual also retains its intermediate
screenshot-budget warning. The ignored-painted-retry and repeated-identical-
retry guards pass. A 960x640 capture (089) with a settled main viewer was inspected:
the first complete empty member, actual parent, exact range, count, separate
clocks and union hints remain readable. Its beat thumbnail still says Loading;
this is inspector/main-viewer layout evidence, not a fully settled thumbnail
capture. Captures 087 and 119 also had pending picture work and were excluded.

## Real-media preflight

The debug real-media test passes with the required run-ignored flag; the first
zero-case invocation is retained as a setup failure. It uses the qualified
cfr-bframes Original at 30000/1001, separate ordinary parents, two empty children,
a 2f unselected Background gap and one Source-clock mark split across selected
sources. A public macro selects in reverse, copies, cuts, clears and seam-pastes
one union. Immutable capture admission, chronological replay, Undo/Redo and
reopened register equality are checked.

Its literal oracle is Original [12,30), Background 2f, Original [30,42):
32 pictures. One complete audio window covers [0,51251), with verified zero
offset, zero flags/uncovered samples/low-SNR blocks. Debug minimum block SNR is
55.303613dB. This compares canonical preview with encoded audio. It supplies
neither an independent PCM oracle nor listening/full-mastering qualification.
The final release test runs from its verified compact archive and passes in
2.862s. It independently verifies all 32 literal picture expectations and the
complete 51,251-sample canonical audio window at zero offset, with no flags,
uncovered samples or low-SNR blocks. Minimum block SNR remains 55.303613dB;
thresholds and the verifier are unchanged.

The emitted MP4 identity and deterministic nine-file owner archive are retained.
Safe extraction rehashes every file; the retained release CLI validates the
extracted package and passes doctor. The package owns its complete Original.
All final Rust/input and retained native/media/helper hashes remain unchanged
through the recorded runs. The debug preflight does not substitute for this
fresh release result.

## Independent source audit

A separate audit recomputes the complete base Git object identities, baseline
controls and all six base supplemental inputs, then checks the broad 1,990-file
Rust map and inputs twice against the checkout. All match. It also inspects the
two-file test-layout delta from debug eighth; production Rust is unchanged.
The canonical broad-candidate Rust-map SHA-256 is
8c810c90835388915f223c596c62cccb1dce6e79a779797a003622e1009787a3.
Separate audits retain the later test cleanup and native boolean-match rewrite.
The latter checks all eleven accepted alternatives, identical scrutinee, absence
of guards and default false. All 1,990 final Rust paths and six inputs match on
two checkout reads. The lint-final map SHA-256 is
a22da25b7cd8f99c1059817ee26ae20c1ae87a7c05295eaccaef2214050b9044.
These are source audits, not substitutes for runtime or final commit evidence.

## Retained evidence

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-22-discontiguous-selectors/inventory.json)
records hashes and byte counts for the reports, failed candidates, exact
source/input archives, selected capture, MP4 and portable project. Test and
helper identities are retained separately from source and Git metadata.
An independent audit verifies the initial inventory, all mapped source/input
archives, test inventories and receipts, raw latency samples, actual MP4 and
owner archive. The report and the explicit layout-receipt repair note are
included in the rebuilt inventory; neither changes a runtime result.
Post-commit Git-blob verification binds all 1,990 Rust files and six supplemental
inputs from the executed release snapshot to code commit
`eea765a96929be926fd3def97c2de6844df7827d`. The separate
`code-source-git-audit.json` receipt records every blob and SHA-256.

## Remaining scope

Arbitrary positive half-open interval sets and transcript-search-result
selectors remain open. Spec 6.2 requires both explicit range selection and
search-derived ranges. Range capsules can repeat one Source's authored context
and mark bindings, requiring per-component provenance beyond the whole-node
rules here. Corrected transcript proposals need freshness beyond document
revision, including correction version and proposal identity. Remaining temporal
families, sound owners, mastering, renderer and complete workflows remain in scope.
