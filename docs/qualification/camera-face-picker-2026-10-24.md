# Camera face picker, 2026-10-24

Camera now includes detected face proposals in its numbered target picker.
The local implementation, review, checks, native replays and actual export
described below pass. DP-08 remains Partial.

## Behavior

`f` requests faces in the exact installed Original picture through the existing
bounded background service. Saved targets and five fixed points keep their
positions as proposals append. Tab and Shift+Tab cycle pages of nine; the
viewer shows compact page digits and the inspector names each proposal.
Pending, empty and failed detections leave ordinary choices available.
Closing or changing Camera invalidates its proposals. Request, session,
revision, source, PTS, scope, cursor and installed-picture guards reject late
results. Completion replaces the service's pending notice only while that
notice still belongs to detection, preserving a newer edit's notice.

Selecting a proposal saves only that face through typed `SetTarget`, with its
measured display aspect and picture-to-shot-end source span. Only the successful
own-save receipt and rebase install the new draft center and target. Refusal
retains the previous draft and permits retry. Enter commits framing separately;
Escape restores entry framing while keeping the explicitly accepted target.
The target and framing therefore have separate Undo steps. Switching an
existing Follow preserves its clock, scale bases, fallback and scale transition.
Detection alone creates no revision. The explicit `target=face:N` zoom command
keeps its existing combined target-and-framing transaction.

No dependency, Core representation, SQLite format, render or DSP change is
needed. Core 58 / SQLite 87 remain. Point targets and general non-face region
proposals remain required.

## Review and checks

General, async-invariant and test-quality reviews ran independently. Review
found a successful save's deferred selection could be lost when a later refused
request replaced its reply. The fix admits the saved receipt only at the
current head and matches session, base revision and target ID. Its regression
covers a later refusal, a superseding receipt and refusal of the save itself.
Review also prompted saved-target identity/digit assertions while proposals
arrive. Follow-up reviews leave no concrete findings.

| Check | Result |
| --- | --- |
| Final App binary unit suite | 1,301 passed; 11 existing opt-in cases ignored. |
| Focused Camera tests | 35 passed; included in the suite. |
| Face service tests | Four passed, including a delayed completion after a real typed target save. |
| Strict App all-target Clippy, with `ui-harness` | Passed. |
| Workspace formatting | Passed. |
| Final release build | Passed. |
| Final `faces` replay | 66 workflow checks plus Kestrel audit passed. |
| Final `targets` replay | 41 workflow checks plus Kestrel audit passed. |
| Final `zoom` replay | 213 workflow checks plus Kestrel audit passed. |

Counts overlap. The eleven ignored cases are not qualification evidence. The
first broader run found the generated command reference stale; it was rewritten
through its prescribed writer and subsequently passes, including in the final
full suite. That writer intentionally exits 101 after rewriting. The initial
worker compile's four borrow/visibility errors and the known debug-linker
compact-unwind warning are retained in the raw logs.

Authored diff checks pass. The complete staged check reports nine trailing
blank lines in retained raw logs/maps; those evidence bytes are preserved.

Each final Kestrel audit checks 62 reservations and 21,885,504 production routes
with no conflict. Live source and pinned source both hash to
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
No new modified global shortcut is added.

## Native and media evidence

The final release App hashes to
`9aec8c29f87b9a97dece459c3bd8dc66d70ce712865eba75a108ac7cfae69e5d`.
The first App was `7919f72c…`. Its semantic replay passed, but actual image
inspection found overlapping long proposal badges and a stale running notice.
Compact digits and completion notice ownership fix both. Final Camera and
committed-editor images were inspected for hierarchy, readable targeting,
focus and keyboard discoverability. The inspector's longer target lists scroll.

The final face replay includes two real Vision no-face detections on the
counter fixture, delayed completion after Camera closes, stable fixed and saved
digits, twelve positive proposals across two pages, separate target/framing
Undo and Redo, Cancel restoration and a subject change on animated Follow.
Positive faces replace only the worker observations; picture qualification,
host admission, service, writes, history, geometry and GPU are real.

The actual project saved by that final replay renders 120 pictures at
30000/1001, 320×180. Independent decoding against the committed shared picture
path checks every picture with no failure. The complete `[0, 192192)` stereo
audio window passes with no uncovered samples or nonzero offset. There are
116 observable and 122 unobservable neighboring-picture comparisons; unique
index observability is not claimed for every counter picture. Audio comparison
uses the canonical limited bus, not an independent DSP oracle or listening.
The emitted file retains its explicit encoder-delay edit metadata.

Movie SHA-256:
`7c2036efceb5ae97179e8b2132944ec3b702dd178f7b42e9ac802f10fc87e798`
(128,183 bytes). All nine CLI operations pass, including complete source and
portable-package validation, copying and checkpoints. The portable document
equals the saved source document exactly; native checks exercise the separate
history steps. Independent SQLite inspection agrees on thirteen immutable
revision rows and seven history rows in the source and portable copy.

The final CLI is
`e9ad52a23d3f91ac198007f18e69811d0ec09797e24c53230255f4e083150701`.
CLI and the three helper identities are unchanged from the preceding following
creep qualification. Ten changed implementation/test sources are frozen around
the final checks, replay and export. Retained executables are private scratch
copies; their identities, invocation receipts and source snapshots are retained
in the evidence package.

The targets reports still contain an old skip sentence saying face proposals
are unimplemented. The separate passing faces scenario supplies that coverage.
After the runs, one skip string in `harness/targets.rs` was corrected. An exact
before/after byte proof records this additional metadata-only source amendment;
its new wording was not executed in the retained replays. Original reports are
unchanged.

## Limits

Metal replays ran on this M5 Max Mac with Rust 1.97.1. They use offscreen native
UI and simulated audio delivery. Native pickers supply a scripted selected
path. Physical display/input, OS IME, VoiceOver speech and acoustic judgement
are outside these checks. Positive observations on the counter fixture establish
the editor workflow, not detector accuracy on people or tracking quality.

All final reports retain their nonfailure warnings: screenshot allowance and
layout retries with distinct input causes. Ignored-retry and repeated-cause
guards pass. These runs make no latency, percentile, memory or performance
target claim. The full product goal and remaining DP-08 scopes stay open.

Raw evidence: [Camera face picker](../../tools/media-qualification/evidence/2026-10-24-camera-face-picker).

The immutable Primary has 138 payloads (6,157,873 bytes) plus its inventory,
SHA-256 `eff0d0c2542c329a1cedd82bbcff1100be9fa5aceb115823284d7bb3ce7ab98e`.
Independent first-execution verification passes and leaves Primary unchanged:
135 encoding rows, thirteen ordinary gzip payloads, two safe TARs with 39
regular members and no omissions. The checkpoint archive retains its actual
empty WAL and 32,768-byte SHM files. No executable binary or live `.rs` source
is published; source snapshots use `.rs.txt`.

The [verification supplement](../../tools/media-qualification/evidence/2026-10-24-camera-face-picker-verification)
retains nine payloads (34,245 bytes) plus its inventory,
`09ed95b195b211efd9517b987aaba11ff4c6bcd293172235c70073ad952b5a44`.
Its verification receipt is `fc671eacce4209a93771cd6224f20ed5e87dc868b4157db701f8912bf34c8fbe`.
The [final Camera image](../../tools/media-qualification/evidence/2026-10-24-camera-face-picker/screenshots/camera-proposals-page-two.png)
and [committed editor image](../../tools/media-qualification/evidence/2026-10-24-camera-face-picker/screenshots/committed-following-creep.png)
are retained alongside the pre-polish image and unmodified raw reports.
