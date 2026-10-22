# Named-mark semantic range selectors

Status: implemented, independently reviewed and verified on this Mac within
the scope below. Independent evidence audit reports no discrepancies.
No requirement or gate is newly complete.

The native command `:select marks start=<exactMarkId> end=<exactMarkId>` selects
a finished range in the active ordinary Edit Sequence with the Linked role.
Recorded selection and consuming native operators preserve NamedMarkTarget
addresses. Named calls and dot-repeat resolve those addresses against each
staged document. Missing, ambiguous, Original-clock, reversed, collapsed and
out-of-scope endpoints refuse without substituting a beat or cached range.
Existing temporal, role and per-instruction range limits remain authoritative.

## Public old-code control

Base `74616d32a40f9ea75ededcaedbe7edd8613bfec6`. An ordinary nested Sequence
contains Source 8f, Hold 2f and Source 8f at 30000/1001. The public AnchorIndex
resolves named Edit endpoints to [2,14). A direct closed capture equals the
existing Visual Yank capture, including assets, owned marks, parts and trace.
Bank aliases, neutral revision behavior, inverse patches and JSON round trips
pass. Growing the Hold to 4f moves the range end to 16 while preserving the
marks' local addresses; the new 14f capture and Undo/Redo controls pass.

Only the final positive `mark_range` SemanticProgram wire-admission assertion
fails, in 0.011s. The test freezes 1,963 Rust files. A Git-blob audit proves
1,961 base files unchanged, with only the witness registration and the new
witness different from the 1,962-file base. Executed test SHA-256:
`8fd2cd33739dc60b1f22943959eba592218b0a4db5454df56a4d8c2a6ab9bd02`.
This is a structural control, not media or audio qualification.

## Implementation and review

The core resolver reuses AnchorIndex exact ordering, occurrence ambiguity and
ties-to-even quantization. Both exact and rounded endpoints must fit the
captured ordinary Sequence. Semantic selections contain addresses rather than
cached frame bounds. Native display geometry resolves those same addresses;
command entry captures role as well as session, revision, scope, cursor and
selection. Selection-only execution leaves authored history and registers
unchanged. Existing atomic capture/edit/bank rules apply to consuming runs.

Removing Copy from SemanticSelector requires explicit clones through keyboard
actions, bindings and receipts. The generated routing baseline changes only
the select command's usage and its completion description. Existing key routes
and parser examples remain unchanged. Place slice recording accepts a named
replacement only when its freshly resolved range equals the captured proposal.

Independent general and authority reviews report no remaining findings. A
test review identified missing native Cut dot-repeat coverage, which is now
included. A proposed escalating-Repeat fallback defect was withdrawn after
confirming the existing second context capture already refused before submit;
the code now captures once to avoid duplicate resolution. Documentation and
the release fixture received a separate read-only review. A follow-up corrected
an overbroad documentation restriction: named selections can feed existing
role-only consumers within one direct child. They do not grant cross-owner
role authority. The added regression verifies exact audio mute and repeated
sound mappings, unchanged picture/time, atomic inverse and no register writes;
both consumers reject a cross-child named range before allocation.

## Retained failed candidates

Initial all-target compilation found new-test constructor/type mistakes,
non-Copy transfer sites and a harness visibility mismatch. These were corrected
without changing selector admission policy. Subsequent normal and UI-feature
all-target checks pass.

The first focused CLI test used trace indices that omitted nested-call rows;
it now finds the instruction by its unique type. Its next run retained a
read-write Store while invoking the CLI writer and correctly received
HostOwnerUnavailable. The fixture now releases that owner before the CLI call.
The registry's intentional rewrite tests then fail as designed while generating
COMMANDS.md and the routing baseline. Their reviewed outputs pass normal tests.

The first native replay used `:repeat 3`, which correctly refused a Visual
range because it is the selected-beat/count command. The test now uses
`:wrap-repeat 3`. The second replay reached the stale-context check but its
direct test cursor injection omitted the corresponding picture request; that
test-only setup now requests the new picture. Neither failure justified a
production fallback or weaker stale-context check.

## Focused and native debug verification

Seven core, one Store and five app focused cases pass. After the CLI fixture
corrections, its durable staged-call case passes in 0.482s. All 297 affected
UI-feature tests pass in 36.308s. Frozen source, supplemental input and executed
test-binary hashes remain unchanged during each reported run.

The final debug replay passes 95 workflow checks across 242 frames, plus the
Kestrel audit, at 960x640. It exercises recording, mark rebinding, named calls,
Repeat and Cut dot-repeat, unresolved-mark refusal, stale-command refusal and
Undo cleanup. The retained compact capture was inspected: exact mark IDs,
[10..20) geometry, 10f duration, separate clocks, focus and key teaching remain
readable. The replay retains the intermediate screenshot-budget warning,
27 second-retry frames and eight consecutive-retry runs with distinct causes.
Ignored painted retries and three identical retries without new input do not
occur. Only the native test setup changed after the passing 297-case run.

## Broader regression

The broad run executes 4,532 cases in 190.263s: 4,531 pass, three are slow,
one passing adversarial case reports an open output handle, and one existing
native new-project test times out waiting for import. All 1,969 Rust files,
154 executed test binaries and five helpers retain their pre-run hashes.
The exact retained app binary passes the import case in isolation in 0.366s;
the retained adversarial binary passes its marked case in 0.096s and returns
stdout/stderr EOF. The parallel-run anomalies remain retained; no production
fix or definitive cause is claimed from an isolated pass.

Afterward, only the new role-composition test and its module registration
change. A complete hash comparison proves production Rust identical to the
broad candidate. All eight named-mark core cases pass in 0.017s, including
the new one-child role controls and cross-child refusals.

## Native carrier size correction and strict checks

The first strict check rejects four native carrier sizes after mark addresses
enlarge selection and instruction values. The correction boxes the pending
copy selection, inline macro instruction and semantic replacement receipt.
It preserves their contents and leaves core and headless wire formats intact.
Twenty app files change, including the constructor updates; all non-app Rust
remains identical. Independent review reports no findings in that exact delta.
All 1,220 normal app checks then pass in 72.104s and all 1,266 UI-feature
checks pass in 72.861s with eight test threads, including the earlier import
timeout case, and no failures or output markers.
The earlier production-identity comparison applies only to the role-test delta.

The next strict run flags only three test mutations that allocate replacement
Boxes. They now replace the existing inner values. A full source comparison
proves production Rust identical to both passing app suites. The three affected
cases pass again in both builds. Final affected-package strict checks,
UI-feature all-target Clippy, formatting and whitespace checks pass.

## Release native workspace

Both release modes pass 95 workflow checks plus the Kestrel audit. Visual mode
paints 272 frames and performance mode 481. Quiet input-to-state p99 is
2.174459ms over 94 samples, with no failed or timed-out state samples. This
measures this two-mark workflow on this Mac, not large-project selector latency.

The final 960x640 capture was inspected. Exact mark IDs, Edit [10..20), 10f
duration, separate clocks, focused Beats, operator key teaching and the footer
notice remain readable without overlap. Its pixels match the earlier inspected
debug checkpoint. Both modes retain 27 second-retry frames; visual mode has
seven consecutive-retry runs with distinct causes and performance has one.
Visual mode also reaches the intermediate screenshot budget. The ignored-paint
and repeated-identical-retry guards pass; raw warnings remain in evidence.

Executed release app SHA-256:
`6d562c518fa60149cab981b10fa36eaa16363487286ec934d7dbdbb7984a9314`.
The app is freshly built; the unchanged retained media/transcribe/track helpers
are reused with their separately recorded hashes.

## Release media and owner package

The release case passes in 2.640s. It retains a qualified cfr-bframes Original
at 30000/1001, grows a silent Background Hold from 2f to 3f, then resolves
Yank/Cut at [11,14) and Repeat at [13,16) against their actual preceding staged
documents. Both three-frame register captures validate against their retained
transaction-step snapshots. Full chronological history replays before and
after public Undo/Redo, with unchanged register contents and zero receipt-only
shortcuts.

All 33 final pictures match the independent ordinal/background oracle and
emitted-file verifier. One complete audio window compares 52,853 presented
samples against canonical preview at verified zero offset, with no flags,
uncovered samples or low-SNR blocks. Minimum block SNR is 58.441757dB, above
the unchanged 1.5dB limit. This is canonical preview/encoded readback, not an
independent PCM oracle. The emitted MP4 and complete reports are retained.

The nine-file owner package is archived, safely extracted and rehashed. The
retained release CLI validates the extracted package and passes doctor. Frozen
Rust, supplemental inputs and executed native/media binaries retain their
pre-run hashes through final verification.

## Scope

Listening and full mastering remain outside this selector qualification.
Explicit node, search-result and discontiguous selectors, wider temporal
families, sound owners, renderer work and complete workflows remain open.

## Evidence

The [retained inventory](../../tools/media-qualification/evidence/2026-10-22-named-mark-selectors/inventory.json)
contains execution logs, requests, reports, exact binary identities, captures,
the emitted MP4 and extracted-owner validation. Twelve complete Rust source
stages share nine content-deduplicated archives; each archive is decompressed
and every member rehashed. Final lock/configuration, generated command reference
and routing inputs are archived separately. Future selector drafts and artifact
sign-in links are excluded.

The independent audit rehashes the 245-entry pre-publication inventory and verifies all
12 source-stage/manifest pairs against their nine unique archives. It confirms
the recorded baseline, failed candidates, isolated reruns, final suites, strict
checks, release replay, media and owner validation without inferring a cause
for the earlier parallel-run anomalies. Supplemental input archives are
verified by the collector and root guards, outside that independent audit.

After committing `374b31df91afec56781efc39cea618680af82314`, a direct Git-blob
audit verifies all 1,970 tracked Rust files and six supplemental inputs against
the final executed source manifest. Its receipt is retained in the inventory;
the foreign fuzz lockfile and Claude directory are excluded from this work.
