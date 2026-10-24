# Whole-Hold following creep, 2026-10-24

Whole ordinary Holds now accept Keep-center following creep, and targets resolve
through a Hold's retained captured geometry. Implementation, independent review,
final native and emitted-media checks, raw-result audit and immutable package
verification pass. DP-08 remains Partial.

## Behavior

An unscoped whole ordinary Source or Hold with a plain Follow accepts `:creep`
without an explicit replacement target. The shared builder receives a typed
`OrdinaryFollowOwner`, classified from the actual selected node. A Hold's
complete output duration is the owner clock, independent of its frozen or
generated provider. Live and fallback scales retain independent bases; the
existing exact factor, curve and Q32 frame-center interpolation remain shared.
Recording, named replay and direct-edit dot keep semantic Keep intent.

Visual ranges, including a range covering the whole owner, Repeat occurrences,
other owner types and already animated Follow paths retain explicit refusals.
The native gate also requires a direct child of the current ordinary Sequence
with no enclosing Repeat. Explicit target replacement keeps its existing
behavior. This adds authoring authority without narrowing the owner types
accepted by raw validated Follow payloads.

Native pause insertion retains `CapturedFraming` even for an identity view.
Previously, a target on any captured context always used its fallback. The new
shared `follow_pose_in_context` projects a point or region center through the
retained view. It applies measured source aspect and Fit/Fill, then each captured
pose followed by that scope's clip, fits the complete clipped canvas into the
next retained canvas, fits the last canvas into the current canvas and applies
live inner scopes. A Fill point may enter through the first pose before the
first clip. Every completed clip is irreversible; an outer zoom-out cannot
revive a removed target.

Missing measured aspect, an invalid context, source/span mismatch, arithmetic
overflow or a clipped center selects the existing evaluated fallback while
scale animation continues. The no-context `follow_pose` wrapper preserves its
previous behavior. The Hold's provider, captured geometry, sound recipe and
duration are unchanged. Plan and temporary Camera preview use the same helper;
Plan also resolves retained Repeat-gap contexts through it. That shared
evaluation does not add Repeat occurrence creep-authoring authority.

Camera shows only the active saved target's full label when several targets
share the picture. Picker pages use compact digits, with names and states in
the inspector; off-page saved markers are hidden. The hint explains captured
clipping and continuing fallback scale. These changes came from inspection of
the first passing replay and subsequent review.

Core 58 / SQLite 87 remain current. No format migration, new dependency or
provider/audio mutation is introduced.

## Review and checks

Independent general and geometry reviews found no remaining production issue.
The test review added actual half-open neighboring-frame assertions and an
independent integer half-even Q32 fallback-scale oracle covering every Hold
frame. A later general review found unnumbered off-page saved markers. The
paint guard now skips them before drawing, with a face-picker regression that
counts actual painted CURSOR-stroked shapes across pages and checks revision
stability. The reviewer confirmed the fix by inspection; runtime results belong
to the final replay below.

| Check | Observed result |
| --- | --- |
| Full Core and Plan suites | 2,047 passed. All sixteen scoped inputs remained unchanged during the run; all nine Core/Plan paths still match. |
| Final focused CLI Hold creep integration | One passed; 57 filtered out. All twenty final scoped inputs remain unchanged. |
| Composed-renderer projection comparison | Two passed, including 960 point/region, aspect, rotation, canvas, captured-view and live-scope combinations plus the legacy control. |
| First App unit suite | 1,304 passed; eleven existing opt-in cases ignored. This preceded final overlay polish. |
| First native `hold-follow-creep` replay | 64 workflow checks plus the Kestrel audit passed. Its label overlap led to the final polish. |
| Final App unit suite | 1,304 passed; eleven existing opt-in cases ignored. All twenty scoped inputs remained unchanged. |
| Final strict all-target Core/Plan/CLI/Render/App Clippy, formatting and release build | Passed with all twenty scoped inputs unchanged. |
| Final native Hold / faces / targets / zoom / points replays | 67 / 69 / 41 / 213 / 34 workflow checks plus each Kestrel audit passed. Final Hold and paged-face images inspected. |
| Actual final native project export and portable document | All 132 pictures, complete 211,411-sample stereo audio window and exact copied document pass; all nine CLI commands succeed. |

The focused Plan lane passes five tests, including a reproducible 256-case
point/region identity-capture equivalence property. The complete picture-plan
lane passes 127 tests. These overlap the full Core/Plan count. The renderer
comparison uses independently composed floating-point picture geometry to
check exact Plan centers and clipping, including both visible and clipped
cases. It is a geometry comparison, not a GPU or performance measurement.

The broad Core/Plan receipt covers sixteen inputs. App and CLI test assertions
were strengthened after that run finished; the registry help, renderer oracle
and picker paint regression expanded the final scoped source set to twenty.
The first renderer receipt covers seventeen inputs, so its two test sources
have a separate pin record. No earlier broad receipt is claimed to cover a
later source revision.

The first raw-result audit found a provenance gap in the earlier strengthened
CLI receipt: its source bytes preceded rustfmt, and the unformatted text had
not been retained to establish that difference independently. Its first failed
audit remains retained. A fresh focused CLI run against the exact final twenty
inputs passes one test with unchanged sources. No product failure or product
source change prompted this recheck; the other qualified lanes remain valid.

Initial fixture failures are retained: the Plan fixture assigned Follow before
admitting its target map, and the CLI fixture initially used the wrong root
clone. They were corrected before the passing lanes. An early Plan property
run retains a Proptest persistence warning; the final property uses a fixed
seed with no file persistence. The generated-command writer's intentional
exit 101 and the existing oversized debug compact-unwind linker warning remain
explicit. Ignored tests and zero-match filters prove nothing.

## Native witness and media

The replay uses the real project writer, qualified 120-picture counter Original
at 30000/1001, production command routing, exact picture plan and offscreen
Metal renderer. It saves a point at source center `[580000,500000]`, applies a
2× Source crop and inserts a twelve-frame Hold at Edit 40. The Hold freezes
Original 39 and retains that crop.

A second saved point at `[680000,500000]` resolves through the captured crop
to the Q32 round-even canvas center for `(.7,.5)`, distinct from its exact
`(.5,.5)` fallback. Keep-center creep from 1.1× to 2.2× retains the target
and evaluates all twelve owner-frame centers. The replay checks neighboring Edit frames
39/40/51/52, their exact Original identities 39/39/39/40 and the half-open
Source/Freeze boundaries. Other nodes, provider, capture, audio, both cursors
and the 132-frame edit remain unchanged by creep.

Named replay, dot, Undo/Redo and Camera no-change Enter/cancel preserve the
intent. A correction moves the second point to source x=.98, outside the
captured crop. Camera and every Hold frame use the fallback center while
the independently computed scale continues changing. Undo restores the live
center. The final native project ends on the restored live Follow for export.

The final App is
`e46f0e4fca6c22be1f4eeb43de05040c56be3e32b465023c7c79b855bb27a126`;
the CLI is
`7a3062a6b042de2ad81d6c9bf96048eefc40dd64f8d265d71ae6837188cdd74b`.
Twenty final source paths and five sibling executable identities are frozen.
All five replays keep their source and binary identities unchanged. Each
Kestrel audit checks 62 reservations and 21,885,504 routes without conflict;
live and pinned source both hash to
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The scenarios retain two, three, two, three and two warnings respectively,
covering layout retries with distinct input causes and screenshot capacity.
No ignored-layout retry or repeated-same-cause failure occurs. These runs grant
no latency or physical-input claim. The face regression retains scripted
positive detector observations; the Hold and point cases do not substitute a
detector or tracker.

Root inspected final Hold images 053 and 057 for the readable active target's
full label and secondary point marker, 069 for committed creep, 112 for the
clipped fallback and 117 for the restored committed Follow. Face image 099
shows numbered proposals on page two without the off-page saved marker. A
separate native assertion checks compact multiple-saved-target picker digits
and complete inspector names; Hold images 053/057 do not show that picker.
The image remains dominant and the inspector retains state. This is offscreen
native rendering, not a physical-display claim.

The actual saved final project renders 132 pictures at 30000/1001, 320×180.
Every decoded picture passes the committed shared picture-path comparison.
There are 216 observable and 46 unobservable neighboring-picture comparisons;
unique index observability is not claimed for every counter picture. The full
`[0,211411)` stereo audio window at 48 kHz passes with zero uncovered samples
and zero offset, including opening and closing segments. The audio edit retains
media time 2,048 samples with no empty edit or physical gap. No event-based
alignment or encoder-block tolerance conceals an offset. Audio is compared
with the canonical limited bus, not an independent DSP oracle or listening
judgment.

The emitted 44,541-byte MP4 hashes to
`276e0755b2fbd8c8904e549695442bb514648f147893b2ae6fad8b9f1a2d70e7`.
All nine CLI commands succeed and the portable authored document is exactly
equal. Independent SQLite inspection confirms equality of every selected
immutable revision, patch, history, receipt, redo, state and transaction-step
table, including eighteen immutable revisions, eleven history rows and
eighteen verified receipts. The final raw-result audit reports no findings;
receipt SHA-256 is
`16a0b9743a790dc682d7b4820f61bd3a2e287968168528770b8ccfc931e4522f`.
The initial CLI provenance stop remains archived byte-for-byte.

These checks establish no physical keyboard/display, OS IME delivery,
VoiceOver speech, listening, real-person target quality or latency target.
The existing owner-verification scope applies.

## Evidence and remaining scope

Task scratch is `/tmp/deadpan-hold-follow-creep-2026-10-24`; source base is
`efd84b88324d1b71dfe339b19c097c376e64783f`. Final scoped source snapshots,
binary identities and independent raw-result audit are retained. Earlier
point-target evidence is unchanged.

The immutable [Primary evidence](../../tools/media-qualification/evidence/2026-10-24-hold-follow-creep)
has 221 payloads, 13,198,233 bytes plus its inventory. Inventory SHA-256 is
`49b624de11bde3b8f6268f6bd8c5ebe0cfe521203de1fddffd36e0f29c2ccf9d`.
Its archive ledger retains nineteen first-source members, five preserved
first-audit members, thirty final project/export members and thirteen actual
native-project members. Both authoritative SQLite databases and complete
authored histories are retained. There are no omitted sidecars or authored
members in these archives. The collection charges 200,389,133 raw payload bytes
against the 256 MiB bound.

The first independent package verification passes with no findings and empty
stderr. It verifies all 218 encoding rows and 67 regular TAR members totaling
4,597,416 bytes, with zero metadata or authored omissions. Primary bytes, all
twenty scoped final source paths and the raw audit/history remain unchanged.
The frozen receipt SHA-256 is
`72a96336a73c3079c871036e0201fe19a9232b4bc16326ce0ad274a770d6f4ec`.
The separate [verification supplement](../../tools/media-qualification/evidence/2026-10-24-hold-follow-creep-verification)
retains eight payloads, 36,035 bytes plus its inventory
`bd6c39be5a907b9519fb9003ef593629b5655b3519c652f6cab34c023832e1e8`.
Root's complete inventory readback and every payload hash pass.
Authored diff checks pass. The complete staged check reports thirteen trailing
blank lines in retained raw logs and an evidence note; those immutable bytes
are preserved exactly.

General non-face region proposals, pointer target dragging, following-creep
authoring beyond whole ordinary Sources and Holds, and the remaining normative
scope stay open. No requirement or gate closes. Coverage remains 89%, eight
Complete and sixteen Partial.
