# First-class point targets, 2026-10-24

Implementation, independent review, local checks, native replay and emitted
media verification pass. The full requirement remains DP-08 Partial.

## Behavior

Attention targets have explicit point or rectangle geometry. A point stores
only its normalized source center; a rectangle retains the original center
and size representation. Initial geometry, samples and corrections keep the
same kind. Points interpolate their centers with the existing exact source
clock, half-even rounding, confidence and Lost-state rules. Follow resolves
either kind through the same nested picture plan and measured display aspect.

In Camera, `n` opens the new-target editor and another `n` switches between
rectangle and point. Points expose only Center; Tab cannot select an invisible
size field. The rectangle size remembered while switching is temporary UI
state and never enters an authored point. Both use source-percent movement,
counts and Shift. Points draw a cross and center marker. Enter saves the
target separately from Camera framing; the own-save receipt continues the
captured draft on its saved revision. Following a newly saved target immediately
updates the draft's live center while retaining the prior pose as fallback.

`c` keeps the saved target kind. A point correction replaces the initial point
at the span start or inserts/updates a later correction. It invalidates samples
only in `[correction, next correction)` or through the span end, keeps other
samples and clocks, and respects the 256-correction bound atomically. It saves
one typed reversible `SetTarget`; it never launches tracking. Rectangle
tracking and retracking refuse points before media preparation or work. Face
proposals remain rectangles.

Point targets use the existing picker, Follow, whole ordinary Source creep,
copy, history and storage paths. Area-based AI guidance retains the selected
point identity and reports `point_target` unavailable; no area is invented.
The Python bridge admits the same center-only vocabulary, rejects mixed kinds
and rejects a point identity claiming an available area seed. Its retained
record parser also admits the existing measured display aspect, validating and
serializing it identically to Rust. A shared golden file binds both languages.

Core 58 / SQLite 87 remain current under the documented additive development
vocabulary. Existing rectangle bytes remain identical; earlier readers refuse
points because their rectangle decoder requires size. No package or historical
row is migrated or rewritten merely to add point support.

## Review and checks

Independent reviews cover Core geometry/corrections/Plan, App behavior and
async ownership, test quality, and Python protocol/area authority. No concrete
finding remains. A suspected proxy correction gap was dismissed: Camera entry
already requires an exact Original presentation ticket. The follow-up review
also checked error ownership after a successful target movement.

The cross-language audit found two real gaps before qualification: the worker's
unavailable-reason whitelist omitted `point_target`, and its retained-record
parser required rectangle size and omitted measured display aspect. Both are
fixed with actual parser regressions. Image inspection of the first passing
replay found stale rectangle wording and a type-switch refusal left visible
after valid movement. The final hint names the target and a successful target
movement clears the earlier Camera error, matching ordinary pose changes.

| Check | Result |
| --- | --- |
| Full Core and Plan suites | 2,041 passed. |
| Worker-focused Rust checks | 48 passed; overlap Core/Plan counts. |
| Shared Rust/Python point identity golden | One passed. |
| Python extension and bridge protocol suites | 39 and 23 passed. |
| First native `points` release replay | 33 checks plus Kestrel audit passed. |
| Final App unit suite | 1,304 passed; eleven existing opt-in cases ignored. |
| Full CLI and Models unit suites | 533 and 186 passed; six and one opt-in cases ignored. |
| Strict all-target Clippy across eight affected crates | Passed. |
| Workspace formatting and final release build/source check | Passed. |
| Final native points / targets / faces / zoom | 34 / 41 / 66 / 213 checks plus Kestrel audit passed. |

The Core/Plan run's broad input receipt also lists App and Models test files.
App comments/replay and the Models golden test changed during that run; every
listed Core/Plan source stayed byte-identical. A separate scope receipt records
those paths explicitly. Initial expected compile-conversion failures, the
formatting check, the generated-command writer's intentional exit 101 and an
initial zero-match Models filter remain in the raw evidence; the corrected
filter executes eight tests. Counts overlap and zero-match runs prove nothing.
Strict Clippy also caught an unnecessary rectangle-to-rectangle `.into()` in a
Models test fixture. Removing it changes no runtime behavior; the full Models
suite, strict check and final release source check then pass. The App test
executable was hashed during execution and again after exit; its bytes stayed
unchanged. Existing ignored tests are not qualification evidence.

## Native and media evidence

The first release App hashes to
`c4f96df66004d1ee6fb43eb698d622886d29b7f9d7fdb711fd25c1fca17761ec`.
Its point replay uses the real project writer, source qualification, exact
picture plan and offscreen Metal renderer. It uses no scripted detector or
tracker: point tracking is expected to refuse before launch. The fixture is
the 120-picture, 30000/1001, 320×180 counter Original.

The replay covers center-only creation, geometry-kind locking, tracking refusal,
direct Follow after saving, separate target/framing Undo and Redo, retained
live-target creep, indexed-PTS correction and source-time plan positions before
the span, before the correction and after it. Initial and corrected centers
are `[530000, 480000]` and `[610000, 550000]`. It preserves all 120 edit frames.

The final App is
`a87afae7c9bd81c4bdded69bfda29d8e226072bb647c2267507fa15ad1a8386c`;
the final CLI is
`1c748464bab0bacd6ec0c4f412bf522ceb16aefa264db4d78d3d775f17e40d95`.
Fifty-two scoped source paths, snapshots, five sibling executable identities
and terminal receipts are retained. Final point, correction and committed-editor
images were inspected for readable controls, focus, marker geometry and feedback.
No detector or tracker is substituted in the point scenario. The separate face
regression retains its existing scripted positive detector observations.

Each final replay's Kestrel audit checks 62 reservations and 21,885,504 routes
without conflict. Live and pinned source both hash to
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The four final scenarios retain two, two, three and three warnings respectively,
covering layout retries with distinct input causes and screenshot capacity.
No ignored-layout retry or repeated-same-cause failure occurs. These runs grant
no latency or physical-input claim.

The actual project saved by the final point replay renders 120 pictures at
30000/1001, 320×180. Every decoded picture passes comparison with the committed
shared picture path. The complete `[0,192192)` stereo audio window passes with
zero uncovered samples and zero offset, including the opening and closing
segments. There are 202 observable and 36 unobservable neighboring-picture
comparisons; unique index observability is not claimed for every counter
picture. Audio uses the canonical limited bus, not an independent DSP oracle.
The emitted file retains its explicit encoder-delay metadata: the audio edit's
media time is 2,048 samples, with no empty edit or physical gap. No event-based
alignment or tolerance conceals an offset.

The emitted 191,679-byte MP4 hashes to
`8f179d12c912fecbd21e8c610457b0131054ec5ed4f99f95c1b907d0171dbdc0`.
All nine CLI commands pass. The portable authored document is exactly equal;
independent immutable SQLite inspection confirms all selected history tables
match, including ten immutable revision rows and five history rows. The raw
evidence audit has no findings; receipt SHA-256 is
`dde49811202b5af9674c7f5eb6307f0c7b89ef2a01e7de6c70fad72438125650`.

These checks establish no
physical keyboard/display, OS IME delivery, VoiceOver speech, listening or
real-person target quality. The existing owner-verification scope applies.

The immutable [Primary evidence](../../tools/media-qualification/evidence/2026-10-24-point-targets)
has 254 payloads, 8,104,783 bytes plus its inventory. Inventory SHA-256 is
`1c1174e044fc2695361371ba532e5475cc09b0dc9bbdc8dd44bdcc951727da69`.
The archive ledger records 52 first-source members, 30 project/export members
and 13 actual-native-project members. The portable package's empty WAL
(zero bytes) and rebuildable SHM (32,768 bytes) are explicitly omitted under
the retained metadata policy, with hashes. No authored history or media is
omitted; both authoritative SQLite files are retained.
The first independent package verification passes with Primary unchanged:
251 encoding rows, 95 deterministic regular TAR members and 86,960,816 raw
payload bytes under the 256 MiB bound. Receipt SHA-256 is
`962dfdaa489e651acdc48ffc9064db1c6b98804314aa5414a3b27bc191aefa90`.
The separate [verification supplement](../../tools/media-qualification/evidence/2026-10-24-point-targets-verification)
retains eight payloads, 36,595 bytes plus inventory
`26d90b351aa2a91d870ecc316048529dd8ff22b162c2c6281b085728bd69424c`.
Its complete readback and every payload hash pass.
Authored diff checks pass. The complete staged check reports fourteen trailing
blank lines in retained raw test logs; those immutable evidence bytes are kept
exactly as executed.

## Remaining scope

General non-face region proposals, pointer rectangle dragging, broader
following-creep authoring scopes and other required target consumers remain.
This checkpoint does not close a requirement or gate, or make the app a
release candidate. Coverage remains 89%, eight Complete and sixteen Partial.
