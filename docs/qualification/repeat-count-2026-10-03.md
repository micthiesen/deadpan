# Repeat count recording and dot, 2026-10-03

Status: count recording and dot verified. No product gate is complete.

## Change

`:repeat N` records its captured intent: set the total play count of the
selected Repeat, or wrap another selected beat. `.` reapplies a saved count
to a newly selected Repeat. It preserves the selected register and durable
register bank. Every Visual selection, including an empty selection, refuses
the setter without moving the cursor or changing the selection.

The native UI and headless Macro API share `SetRepeatPlays` through semantic
Apply. The planner requires an explicit direct child of the current ordinary
Sequence, allocates one fresh leaf revision and uses the existing exact timing
setter. Surviving play identities, gaps and downstream sample clocks retain
the core setter's behavior. A same-count setter still authors a fresh revision.
Recording stores the count rather than an old node identity. A Macro with
wrapping and multiple setters commits as one history entry.

Successful Apply proves dot intent before refreshing the workspace. A saved
refresh failure retains the committed receipt and reopen guidance. Exact
retries return their retained receipt without reinstating an older dot action;
named Macro runs remain unproved. No project schema change is required.

## Environment

Base: `9870a4cbc706ad8cce8b59d4cb13ec5c2d3a9c33`.
Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1.
Checks use locked dependencies and `/tmp/deadpan-ui-ffmpeg/prefix`.

## Verification

[Retained evidence](../../tools/media-qualification/evidence/2026-10-03-repeat-count/metadata.json)
contains source inventories, commands, compressed logs and replay reports,
selected screenshots, native observations and a SHA-256 inventory.

- The full workspace gate passed 3,799 tests, zero ignored, including doc tests.
- The optional `ui-harness` app configuration passed 764 tests, including four
  headless integration cases. It overlaps the workspace total and is reported
  separately.
- Four rendered workflows passed 1,043 checks: count setters 67, Repeat
  operators 223, dot 381 and Macros 372.
- Each rendered run passed 3,319,728 production-router Kestrel cases against
  62 reserved bindings, with no conflicts or live-source drift.
- Formatting, the locked release build and strict all-target lint passed in the
  default workspace and optional app configurations. The debug build retains
  the existing oversized `__eh_frame` linker warning.

The full workspace gate used stable source inventory
`dd00cc7eda98233c93012ab17c1ffa051322aea775145d4eaecbd8b7c62304cc`.
Only the optional Repeat replay changed afterward for its fixture correction
and minimum-window assertions. The final source inventory is
`ee933f6c2f5e470b1d6c2ff2ff5f93824901e73539b3ab330a8f51402ca344d2`;
the rendered and native binary is
`d09b3e5da01daa4b354474af8d0b127e6d91d87767d1cab7215d138231bbbe5e`.
The optional app tests and all rendered runs use that final source. Unchanged
backend tests were not repeated after the replay-only correction.

Image review confirms the recording state, two independent Repeat cards, exact
count hint and explicit Visual refusal at 1280×820 and 960×640. At minimum size
the normal copied-register/count state retains a roughly 122-point picture;
this check qualifies the new hint's paint, not complete picture-dominance or
workspace layout acceptance.

The dot replay emitted one `[h264] decode_slice_header error` diagnostic while
passing all assertions. Its cause remains unqualified; the complete stderr
is retained. No decoder implementation changed in this increment.

### Release replay

Release binary
`ef9921d7758ef3293c63b0f89cfebb43b5ea5c2e6bbc2a76df8175018ad022d1`
passed 66 checks without screenshot readback. The visual run's additional
check retains its project for native QA; performance mode deletes that fixture.
There were no failed or timed-out timing samples. No owned Cargo build, other
replay or native QA app was active during measurement.

| Interval | Samples | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Input handling | 53 | 0.882 ms | 1.102 ms |
| UI frame CPU | 302 | 0.701 ms | 3.518 ms |
| Picture request to offscreen GPU completion | 21 | 2.287 ms | 5.667 ms |
| Input to offscreen picture completion, including cold import | 14 | 88.625 ms | 88.625 ms |

This is a small real-video fixture with mixed interactions. The final row
includes initialization and is not a warm edit/seek benchmark. Background load,
thermal state, full-size projects, physical display latency, acoustic output
and the complete performance gate remain unqualified.

## Native keyboard observations

A developer bundle opened a private copy of the closed replay package. Native
keys and accessibility state verified:

1. `:repeat 5` changes the first Repeat, showing Edit 0/1080 and 600 frames.
2. `j`, register `a`, then `.` changes the second Repeat to five plays, showing
   Edit 600/1200. Register `a` remains selected.
3. `qz`, `:repeat 6` records one setter and shows Edit 600/1320. `q` saves it.
4. One Undo restores five plays. `:macro z` restores six plays; one Undo returns
   five plays. The register choice survives every step.
5. Cmd-Q exits with code 0, and a targeted process lookup confirms absence.

Closed headless reads independently retain two five-play root Repeats and
Macro `z` with exactly one `set_repeat_plays` instruction for six plays. The
developer bundle still requires external build-host libraries.

## Review and retained failures

Independent core and host/native reviews found no production issue. Replay
review strengthened assertions for the register chosen before `:repeat` and
the exact Visual selection, cursor and child retained after refused dot.
Final replay review caught a fixture assumption: edited paste selects its
neutral containing Sequence, not the copied Repeat inside it. The corrected
fixture retains a real historical copy in the bank, reuses the Original and
wraps that new direct child before testing the setter and dot. It also checks
the exact dot key and count hint's paint at 960×640.

The rendered regression failed against the old implementation at recording
a total-play setter. Its initial build first caught an incorrect nested
Option assertion in the replay. The full workspace build then caught a new
test comparing a receipt type without equality. The test now compares its
identity, bank version, commit and complete Applied result explicitly; no
production equality implementation was added.

## Limits

Temporal editing inside Repeat/Retime contents, occurrence-aware Macro
selectors and dot for the remaining edit kinds are still required. Rendered
input does not establish physical keyboard delivery, OS IME, VoiceOver,
acoustic output or release packaging. Debug runs do not qualify performance.
