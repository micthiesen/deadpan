# Group objects, 2026-10-03

This increment adds `ig` and `ag` to native keyboard editing, Visual selection,
registers, macros, dot and Place slice. It advances DP-05, DP-06, DP-20 and DP-21.
No requirement or product gate is complete.

## Behavior

`ig` selects the exact children of an ordinary Sequence, including empty
endpoints. `ag` selects the Sequence itself with its authored context. An
explicitly selected direct group wins; otherwise the containing nonroot group
is used. Neither the cursor nor equal frame boundaries infer ownership.

`yig`/`yag`, `dig`/`dag` and `rig`/`rag` compose with the existing operators.
Visual `ig`/`ag` retain the group identity and object kind. Finishing Visual
keeps that object through independent navigation; extending it with a motion
converts it into a time range. Register paste and Place replacement preserve
the exact target, including zero-duration groups.

After an inner edit the group survives. Editing from inside stays inside;
editing from outside keeps that group selected. A whole-group edit continues
in its outer Sequence. Cut selects the literal next sibling, then the previous
sibling, including empty neighbors. Macros resolve every subsequent instruction
from that returned context and commit authored changes as one Undo.

Capture provenance retains the effective parent, exact bounds and staged path
labels at the copy instruction. It is independent of the invocation and final
navigation scopes. Store admission still recaptures historical contents.

See [the selection contract](../STRUCTURAL_SELECTIONS.md) and
[semantic macros](../SEMANTIC_MACROS.md) for the wire format and full behavior.

## Review and corrections

Independent reviews covered exact replacement reducers, timing and ownership;
semantic object resolution and continuation; and native/host capture, Visual
state and Place admission. The reducer review found an unnecessary node-growth
preflight for Original replacement. Replacement removes at least one owner
before inserting one, so a document at its node limit must remain editable.
The guard was removed and a maximum-capacity regression case now passes.
Final root review found the same restriction in semantic Original paste. Its
guard now excludes exact Children replacement while retaining the existing
seam and time-range preflight. A semantic regression replaces an empty group
in a document with exactly 100,000 nodes and checks that the new Source takes
its place without increasing the node count.

Root review found that a failed captured macro context could make Object paste
fall back to the time-range route. Routing now depends on the retained Object
selection even when context capture failed. The operation refuses that failure
without queuing an edit or changing the register bank. The rendered replay
includes this injected capture failure.

The first audio run failed because the new test used a timing allocation that
did not match its request revision. Corrected fixtures pass without weakening
the production admission contract. An app invocation incorrectly requested a
library target; Deadpan is a binary crate and Cargo refused before compilation.

The first compiled app run passed 786 tests and failed seven new or affected
fixtures. Four had stale `ig`/`ag` prefix, alias or pending-scope expectations; another
expected no initial operator action instead of the existing `OfferInsert`.
The shortcut audit still counted three Visual states, and the cold Original
fixture unwrapped a pending response. Correcting those expectations left one
failure: that fixture uses a manually scheduled worker and had not completed
its job. It now asserts `PreparingInsertion`, completes the actual preparation,
then verifies the committed outer scope. Its targeted rerun passes.

The first CLI run passed 37 tests and failed a new comparison of independently
allocated node IDs between dry-run and commit. The corrected test checks the
same final parent, cursor and Visual state, then verifies each fresh selected
owner against its own preview or committed structure. All 38 CLI project-command
tests pass. The initial failure's tool-output excerpt is retained; its full log
was overwritten by the corrected run.

The first rendered Group run passed 358 checks before the new Place helper
incorrectly required a committed picture. The app had submitted the correct
unsaved proposal. The helper now uses the existing stable-proposal identity
check, including session, project, revision, draft/change, output frame and
Original frame. It requires requested, decoded and GPU-submitted pictures to
agree. Visual inspection also found that fast-paste buttons still said
“Replace range” for Object selections; they now say “Replace object,” and the
replay checks both configured button labels.

Strict lint rejected an eight-argument reducer. Its exact first/last endpoint
IDs now travel as one tuple parameter. All six exact replacement regressions
pass after that signature-only change.

The next workspace lint run caught two store integration fixtures still using
the old untagged Visual struct. Both now construct `SemanticVisualSelection::Time`;
all 38 source-registration tests pass. A rendered test build also caught a
wrong namespace for `EditorKey` in the new button assertion; the test uses its
existing local alias.

Place review found that the footer described the entry cursor instead of the
Object replacement. It now names the group path and exact removed interval.
The first caption assertion looked for an accessibility label, but egui exposes
this measured text as a value. The retained failure report and screenshot show
the correct text; the assertion now checks that value and actual visible paint.
An adjacent Time replacement replay also caught an incidental label change;
its established `Fixed Edit [..)` wording is restored.

The final app lint phase found an oversized captured-delete enum and a
test-only cloned reference. The capture now boxes its general `ProjectEdit`
variant while retaining the same constructors, target checks, cut request and
hints. A narrower trial left the existing service's Delete and DeleteRange
variants unconstructed in production; its warning and lint failure are retained.
The boxed version preserves that interface. The test uses `from_ref`.

## Verification

Checks use locked dependencies and retained source inventories. Counts overlap
between configurations and targeted reruns; they are not one combined suite.

| Check | Result |
| --- | --- |
| Full core suite before the capacity corrections | 814 passed across 46 targets |
| Complete edited-slice target after reducer capacity correction | 45 passed |
| Exact replacement cases after the lint signature change | 6 passed |
| Semantic suite after planner capacity correction | 87 passed |
| Store unit tests | 90 passed |
| Real source-registration integration target | 38 passed |
| Real-audio composite insertion target | 74 passed |
| CLI project commands | 38 passed |
| Default app unit and headless targets | 761 passed |
| Optional app unit configuration | 792 passed, then the one corrected worker fixture passed separately |
| Optional app headless target | 4 passed |
| Focused app Object tests | 10 passed |
| App deletion cases after boxed capture cleanup | 17 passed |

The optional app result is not a single clean full run. Full workspace tests
were not rerun for this increment. The retained inventories distinguish earlier
test builds from later caption, fixture and capacity corrections; targeted
tests cover those corrections.

The final source passes `cargo fmt --all -- --check`, the native UI harness
build, and both strict Clippy configurations: the full workspace with all
targets, and the app with all targets and `ui-harness`. Both use locked
dependencies and `-D warnings`. The source inventory stayed unchanged during
each check.

The final Group replay passes 428 checks, including exact Object admission,
source/proposed picture identity, cancellation, commit continuation, empty
contents, macro recording and visible caption paint. The complete Place slice
replay passes 846 checks and the existing Macro replay passes 372, for 1,646
workflow checks across those three runs. Each run also passes the production Kestrel audit: 7,652,784
routing cases against 62 reservations, with no conflicts. The live shortcut
source SHA-256 is
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

Final Group screenshots were inspected at 1280×820. The Object selection has
visible key hints and `Replace object` buttons. Place shows the first and last
included source pictures beside the unsaved edit, with the exact group path
and `[20..40)` replacement interval fully visible above its comparison bars.
These final captures use executable SHA-256
`d87f817522e95e7bad1171701863b5b983bc2e84bee7b8212ce3ac31807af40a`.

After the internal capture cleanup, the deletion replay passes 309 checks on
the final source, bringing the retained passing workflow runs to 1,955 checks.
It covers captured deletion commands, ranges, frame cuts and their history.
Its executable SHA-256 is
`cb10c558b066c531103dd6e38f005aaa2d54200608e125da7ffdf60fca8e9143`.
The final source inventory SHA-256 is
`a717eef96493a7bda5d11c49831f1b78d2db855dfa71f9a659173cf53fc9005b`.

The core cases cover group precedence, parent-effect exclusion, exact empty
endpoints, inside/outside continuation, literal empty neighbors, Visual
conversion, counted macros, Original/Edited replacement, malformed wire data,
late atomic failure and instruction limits. The capacity case builds a document
with exactly `MAX_DOCUMENT_NODES`, replaces its last empty child with a Source,
and verifies inverse restoration and JSON round trips.

The real-audio replacement case exercises positive-to-empty, empty-to-positive
and positive-to-positive replacements in nested Sequences at NTSC rate. It
checks retained prefix and suffix PCM without tolerance and compares the
transformed independent sound bus with the corresponding authored reference.

## Native window check

The bundled native app was exercised with keyboard events and accessibility
observations on a retained 120-frame NTSC project. Register `a` received an
exact `yig` capture. `vag`, then `v` and `h`, kept the whole-group target while
moving the cursor independently. Place replacement showed the copied endpoints;
Escape restored the selected object, scope and cursor. Both fast paste and
prepared Place commit replaced the whole group and continued in the outer
Sequence. Undo restored it.

Inside the group, `dig` retained an empty group. `vig` selected its explicit
empty contents and paste restored the copied 120 frames inside it. Two Undo
steps restored the earlier state. The native screenshot had readable Object
key hints and `Replace object` controls at 2× scale.

Cmd-Q exited with code 0 and a targeted process check found no remaining QA app.
Closed-project validation then passed. The saved register's exact Children
capture matched the restored group and child identities. Native observations
use executable SHA-256
`6209f1dcc39dc0c3dd20a47c81d716938a24f5c0da2aa4a0666fea4fdf431c11`.
The final caption correction is checked separately by rendered replay.

## Environment and limits

Base: `e848409f1c913bb47076d6d6810a1a822fa503dd`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1, locked dependencies and
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

`ib`/`ab` still require the separate beat-owned temporal attachment lifecycle.
Analysis-dependent objects, temporal occurrence editing and remaining semantic
operations are open. Physical keyboard, OS IME, VoiceOver, acoustic delivery
and large-project performance are not qualified by these checks. The native
run overlapped compilation, so it provides behavior and visual observations,
not performance measurements. Full DP-01 through DP-24 and Gates A through G
remain open or partial.

## Retained evidence

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-group-objects/metadata.json)
records every check phase, test counts, replay results, executable identities,
source differences, native observations and limits. The same directory retains
compressed logs and source inventories, review findings, three inspected
screenshots and reproduction scripts. [SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-group-objects/SHA256SUMS)
covers the retained files, including failed attempts.
