# Corrected transcript-search selectors

Status: implemented and locally verified within the scope below. No requirement
or delivery gate is newly complete.

| Local check | Result |
| --- | --- |
| Public committed-baseline controls | Expected old-wire refusal; independent analysis-freshness control passes |
| Focused third candidate | 124 pass |
| Final ordinary/UI-feature Clippy and formatting | Pass with warnings denied |
| Final complete UI-feature suite | 1,292 pass; 11 existing ignored model/playback/stress cases remain outside this checkpoint |
| Second debug native replay | 103 workflow checks plus Kestrel pass; three retained layout warnings |
| Separate retained-movie preflight | 120 pictures and explicit full 192,192-sample window pass at zero offset; new test's automatic-window assertion failed first |
| Broader regressions | 4,872 pass; one passing output-handle warning also passes an isolated retained-binary run |
| Release native visual/performance | 103 workflow checks plus Kestrel pass in each mode; all three compact captures inspected |
| Release export and portable owner validation | 120 pictures and all 192,192 samples pass at zero offset; extracted nine-file package validates and passes doctor |

## Intent and supported editing authority

The candidate adds `:select search="hello world"`, the semantic selector
`{"type":"transcript_search","query":"hello world"}` and recorded
`SelectTranscriptSearch`. The raw bounded query is preserved through finished
selection and repeated actions. Analysis uses the established normalization and
final-word prefix matching. Editing refuses empty normalized queries, more than
32 terms, more than 4,096 raw matches, excessive projection work or more than
64 normalized disjoint intervals. Exhaustion never returns a partial selection.

Search selects complete forward occurrences in the current ordinary Sequence
scope in Your edit with Linked role. Exact inverse mappings are enclosed in
whole Edit frames and overlapping or touching enclosures merge. The finished
selection remains a set, including when it has one component. Copy and Cut use
the existing entry-snapshot capture, descending atomic deletions and durable
EditedSet register. Scalar consumers and unsupported temporal interiors retain
their existing refusals. This does not complete arbitrary temporal editing.

## Complete phrase proof

One bounded provider-clock coverage query uses the canonical plan walker.
Cutaways do not change the Original speech clock. Structural provenance
distinguishes Source leaves from forward-playing Holds and explicit/default
Repeat gaps. Only positive Source clocks inside both the authored Source
selection and the measured Original picture interval contribute support.
Held endpoint slack and unmeasured audio lead or tail cannot fill a phrase.

Adjacent pieces may cross a split Source ID or change positive slope if their
root and Original endpoints meet exactly. A jump, removed pause, missing word,
reverse segment, Hold, gap or changed shared Repeat iteration breaks that
proof. The entire first-word start through last-word end, including intervening
pauses, must fit one proven chain. Exact fractional terminals are legal coverage
endpoints; an enclosing whole-frame selection outside the current exact scope
is still ineligible. This is logical Original speech carried by the picture
provider, not proof of audible PCM after independent audio treatments.

## Analysis authority

The host pins one exact TranscriptKey and qualified Original receipt. Its
snapshot contains domain-separated canonical proposal and correction hashes,
the correction rule, correction version and explicit Absent/Present state.
Current corrections and key-scoped Undo/Redo history must be readable. Skipped
word correction regions refuse. Adding an unrelated transcript key does not
invalidate an unchanged pinned key.

Both authored and register-only Compound publication recheck the consumed
snapshot inside the same Immediate SQLite transaction before preparation.
Selection-only success checks head, project, bank and analysis together in a
read transaction. Guards are host authority, never authored history or copied
content, so later proposal replacement cannot disable historical Undo/Redo.
The shared provider loads one frozen analysis snapshot for word motions and
search within an invocation. This does not establish activity or shot freshness.

Native command entry pins the displayed snapshot, including its absence.
Completion preserves a durable success if newer analysis arrives, while stale
selection installation and recording continuation are declined.
An installed native search selection also retains that guard. Replacing words
or corrections clears its old geometry without changing the document; capture
and set inspection reject it before reconciliation as well. Ordinary manual
selections remain independent of transcript updates. Compact status summaries
abbreviate long queries; the inspector and saved intent retain their exact text.

## Committed-baseline public controls

Base: `565cb1e8227976e7889bb5506c8b9bfa2f27604e`. All 2,000 base Rust files
and six supplemental inputs were compared with Git blobs before adding the two
public controls. Their frozen baseline contains 2,002 Rust files.

The Core control first proves literal separated range selection, capture,
atomic reverse deletion, inverse application and paste without a hull. Only
its final strict `transcript_search` wire assertion fails on the old code, in
0.012560s. Executed SHA-256:
`6af85b718f2a8e9b5dfccbba7645096c9f4a27bf3d7bfb4fd44a973e825cfd55`.

The Store control passes in 0.092372s. Same-key proposal replacement and
correction Apply/Undo/Redo change search results without changing authored head
or register version. Undo retains a Present empty correction row, distinct from
the initial Absent state. Executed SHA-256:
`4d4ffecac6618c91ff4f6d80a8d2cb0a68ebd467e30dfc3640ec180fe12f039e`.

The first setup attempt compiled Core but failed Store compilation because the
control imported RegisterBank from the crate root instead of its public
registers module. No control ran in that attempt. Its frozen sources and logs
are retained separately. The corrected baseline uses the public module path.

## Candidate verification

Scratch source snapshots and logs are under
`/tmp/deadpan-transcript-selectors-2026-10-22`. The first backend archive build
finds an internal Core planner test fixture still supplying the old closure
directly. The fixture now uses the compatibility adapter; no passing test claim
is taken from that failed build. Native integration is being verified separately.

The second frozen backend candidate compiles in 6m29s. Its first filtered run
passes 32 of 34 cases; both failures are fractional-clock fixtures using
Preserve pitch without a retained processing operand. The corrected fixtures
use FollowSpeed, preserving production admission. An additional selection from
the same retained archive passes all eight Core-provider and three public CLI
host cases. One pure Core case receives nextest's leaky-process-output warning.
The same retained executable passes an isolated direct run in 0.022293s with
both captured pipes reaching EOF. The concurrent warning remains retained;
this check does not qualify the later changed source.

Independent review finds two availability defects: coverage over the whole
root can exhaust a small group's budget on unrelated content, and unconditional
transcript consumption can make pause/shot-only motions depend on unrelated
word analysis. The next candidate queries only the captured exact scope and
uses a word-consumption hook before checking the cached speech timeline.
Regressions cover 513 unrelated Repeat spans and qualified pause/shot actor
requests with captured transcript absence after proposal replacement.

The clock review's proposed forward-Hold admission is declined after
reassessment: inserted Holds do not supply forward Original speech, and the
documented Source-leaf contract excludes them. The older word-navigation
projection's Picture::Source-only behavior is not the new selector's authority.
The reviewer withdraws that finding. Review of the narrowed freshness fix finds
no additional issue; execution evidence for the next candidate remains pending.

The first combined native archive build finds three missing exhaustive selector
display arms, one missing guard field in a legacy fixture and two mutable test
accesses through the new immutable host-plan wrapper. No test ran from that
failed build. A further review identifies unbounded raw-query footer display;
the next candidate abbreviates only UI summaries and retains the exact query
in the inspector and semantic intent. Review also finds that a finished native
search highlight can outlive its analysis snapshot even when a later command
correctly uses current analysis. The next candidate binds that installed
geometry to the guard and clears it on annotation replacement. Pure descriptor
tests and the real-service native replay cover that transition.

The second combined candidate compiles and runs 124 focused cases: 121 pass
and three fail. Two fractional fixtures still create a fractional root, which
the public document validator correctly refuses. They now retain the tested
3/2 scope inside an integral root with an independent 1/2 tail; no production
validation changes. The actor's all-table comparison detects an independent
automatic retention watermark, with every other table unchanged. Its fixture
now disables automatic retention before creating the project, using the
existing service switch; the complete database-cell comparison remains intact.
All 2,012 Rust files, six supplemental inputs and executed binary hashes remain
unchanged through that failed run. These repairs await the next frozen run.

The third combined candidate passes all 124 focused cases in 5.831833s, with
all frozen source/input and executed binary identities unchanged. Its real
export preflight reaches the independent 120-picture ordering and then fails
the new test's assumption that automatic audio selection returns one window;
the verifier returns four bounded windows. A separate read-only invocation of
that same retained executable explicitly requests `--samples 0:192192` and
passes every picture plus that complete audio interval, with zero uncovered
samples, verified zero offset and no audio flags. The test must request that
interval explicitly before its final qualification. The failed assertion and
the separate complete-window report are both retained.

The first debug native replay passes Kestrel and reaches 66 workflow checks,
then exposes a wrong paste oracle in the new scenario: ordinary `p` inserts
after the selected beat, so placing the cursor inside the final beat does not
create an interior seam. The scenario now selects that final beat and checks
the actual seam at frame 99 against a literal complete picture order. No
production paste behavior changes. The second debug replay passes all 103
workflow checks plus Kestrel over 240 frames, including command-entry and
installed-selection analysis replacement. Three layout warnings are retained;
none is a failed retry invariant. Compact normal and long-query captures were
inspected: the main picture and action keycaps remain visible, the footer
summary is bounded, and the inspector retains the raw query in its scroll area.

The first strict check refuses an unnecessary cloned reference in the new
public Store control. The comparison now uses `std::slice::from_ref`, with no
behavior change or lint suppression. Its failed log and frozen sources remain
separate from the rerun. Commands and routing references are regenerated from
the production registry; the first generator invocation used the old embedded
completion list, and a later correct test-binary invocation restores the new
usage row. One accidental empty-library invocation ran zero tests and supplies
no verification claim.

Further all-target checking finds one older CLI test helper declaring the raw
Core plan return type instead of the guarded host plan, then flags growth of
the native request enum from the new analysis snapshot. The helper now retains
the host wrapper. Native request IDs share the immutable guard through `Arc`;
contents and captured absence are still compared exactly. No enum-size lint is
suppressed. These failed candidates and their source manifests are retained;
the later complete suites qualify the final pointer representation.

The first broader archive compiles 205 binaries in 14m20s and runs 4,872
cases: 4,871 pass and one new provider fixture fails. That fixture enters at
the only pause's start and asks for a later pause; production correctly
reports none. Forward pause/shot motion now starts before the interval, while
object selection stays within it. The no-word-authority assertions remain.
The focused expression had not selected this case because its binary name
differs from the exact `transcript_search` target; the corrected focused
expression explicitly includes `transcript_search_provider`. Source comparison
proves that only this integration-test file changes after the broader failure.

The corrected provider fixture passes all 13 cases. The second broader run
passes all 4,872 selected cases across 170 test binaries in 261.814s, with all
frozen Rust, supplemental inputs and executed hashes unchanged. Nextest marks
the passing existing app test
`project::tests::consecutive_cached_insertions_report_distinct_committed_nodes`
for a lingering output handle. The same retained executable passes an exact
isolated run in 0.243665s, with both pipes reaching EOF. Its SHA-256 is
`0e875ecb8f3d92de855fd14177f8005da7a3874e9ff05d65862406b74b6791da`.
The concurrent warning is retained; the isolated check does not erase it.

## Final strict checks and release native replay

The final ordinary strict check passes in 189.311s, UI-feature Clippy with
warnings denied in 55.738s, and formatting in 6.486s. The complete UI archive's
1,292 passing cases precede only the corrected Core provider test cursor.
`ui-to-final-attribution.json` proves every production Rust file, every app/CLI
test and all six supplemental inputs are identical. The final broader run
includes all 13 corrected provider cases. Native request size is fixed without
suppressing its lint, and no timeout is raised.

The release app builds in 275.227s. Visual and quiet performance replay each
pass all 103 workflow checks plus Kestrel. They paint 268 and 528 frames
respectively; no semantic, retry-invariant or timing assertion fails. Quiet
input-to-state p99 is 1.579542ms over 104 samples. The visual run retains three
warnings and the performance run two: screenshot capacity in visual mode,
31 frames needing a second layout retry, and five runs of consecutive retries
whose frames each have a distinct input/cause. Full reports preserve the causes.

The three final 960×640 captures were inspected. The normal query keeps the
main picture, clocks, action keycaps and selected ranges visible. A 512-character
punctuation prefix plus `needle` stays exact in the scrollable inspector while
the footer summary remains bounded. The pasted union occupies [99,120), and
the main viewer shows Original picture 8 at Edit boundary 99. The new group's
thumbnail explicitly remains Loading in that capture; it is not represented as
a settled thumbnail. Native offscreen replay does not establish physical input,
IME, display or VoiceOver speech behavior.

## Release media, portable project and retained evidence

The release test passes in 4.744473s, including a 1.732082s render and
1.033465s emitted-file verification. Synthetic `needle` annotations on the
qualified full 120-picture CFR Original resolve to literal Edit [8,15),
[29,36), [50,57). One shared host macro selects, captures, cuts, clears,
moves and appends the union. Undo/Redo, reopen and a portable owned-media copy
retain the historical captures. The independent complete picture oracle is:

```
[0,8), [15,29), [36,50), [57,120), [8,15), [29,36), [50,57)
```

Every emitted picture passes. The explicitly requested canonical audio window
[0,192192) passes at verified zero offset, with no uncovered samples, flags or
low-SNR blocks. Minimum block SNR is 29.534687dB, minimum luma PSNR 55.807726dB
and minimum chroma PSNR 53.592950dB. Encoder priming is declared as 2,048
samples; no event alignment, packet dropping or shifted PCM hides it.
This proves canonical preview versus emitted audio, not independent DSP,
listening quality, ASR accuracy or full mastering.

The 41,078-byte MP4 SHA-256 is
`b98798e85d4ab215b88c1ffc144fc92a7255c31880060135a9550aa83258e6d9`.
The 98,863-byte owner archive SHA-256 is
`0158dd905e5abdfca78ee6bd87ea300f4768700d4bd84c80bc59eea1eda1afa2`.
Its nine files are safely extracted and rehashed. The extracted project passes
`project validate` and `doctor --project`; all paths and bytes remain unchanged
after both checks.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-22-transcript-search-selectors/README.md)
contains reports, exact executed binary identities, failed and passing logs,
six selected captures, the emitted movie and owner archive. Content-addressed
source payloads and per-stage manifests reconstruct every frozen candidate
without duplicating unchanged files. The code-source audit compares all 2,012
Rust paths and six supplemental inputs directly with the published code commit's
Git blobs. Rust 1.97.1, the pinned FFmpeg prefix, Mac17,7 M5 Max/128GiB and
macOS 26.5.2 (25F84) are recorded with the actual commands. Release app SHA-256:
`76467e8628b92783bd8b319c98ab2c6bc22c7382122621e82c0690f5a0624029`.
Reused unchanged helpers are identified explicitly; they are not described as
freshly built. No source, supplemental input or executed binary changes through
the final release checks. SQLite 84/core 55 are unchanged.
