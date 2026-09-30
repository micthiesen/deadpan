# Visual slice replacement, 2026-09-30

## Scope

Select a half-open range in Your edit with `v`, motion and `v`. Open `:splice`
and choose **Replace selection · r** to preview a copied Original slice in its
place. The removed interval stays fixed as Original In/Out changes. The two
intervals share a display scale and show a signed duration change. Enter saves
one `ReplaceSource` command; one Undo restores the previous authored document.
Fast `p/P` uses the same replacement command when an Edit range is selected.

The captured ordinary Sequence owns the edit. Endpoints support Source,
ordinary Hold and supported transparent fragments; whole intervening composites
can be removed. Empty children at either endpoint survive in order. Repeat and
Retime occurrence destinations, copying/moving edited slices and separate
picture/audio placement remain required. This increment closes no DP requirement
or release gate. See [the contract](../SLICE_PLACEMENT.md).

Core schema 34 adds a direct persisted sound replacement map. Database schema
43 stores it. Under the user's unused-project policy, databases 39–42 now fail
before writes or backups; existing frozen adapters for 1–38 remain. No adapter
claims to migrate the rejected development versions.

## Verification

The locked workspace passes 2,641 tests, with none failed or ignored.
The UI-feature app run passes 372 tests. Strict workspace and UI-feature
Clippy checks pass. Workspace formatting passes after one enum-layout correction.
The production `place-slice` replay
passes 401 checks, and the Kestrel audit passes all 6,448 cases with no local
source drift. The replay covers active and finished forward/reverse selections,
raw input batches, independent Original/Edit ranges, pointer and keyboard mode
switching, native-button and synthetic IME ownership, fixed replacement targets,
short/equal/long proposals, exact Before/Proposed sample mapping, stale and absent
command-entry targets, fast paste and complete authored Undo.

The 960×640 and 1280×820 captures were inspected. Both endpoint pictures, large
destination picture, unsaved operation, exact removed/inserted bounds and
proportionate intervals are visible. The intermediate screenshot allowance was
reached; semantic checks continued and named checkpoints retained capacity.
Offscreen input does not establish physical IME, VoiceOver or audio delivery.

Focused core, plan, audio and store tests cover endpoint splits, nested groups,
retained framing and allowances, pinned marks, identity budgets, direct sound
mapping, old vocabulary rejection, exact commits, rollback, reopen and Undo/Redo.
The NTSC sample-4804 regression preserves a suffix sample that separate Delete
and Insert rounding would lose. Handwritten nonzero PCM oracles cover Source,
room tone, Repeat and Preserve suffixes at fractional frame rates.

Actual media checks independently decode all four proposed and committed join
pictures, including metadata and framing, and compare canonical PCM and limiter
gains. The final 256 samples agree with the retained saved suffix. The clock
fixture has impulses at samples 100, 48,000 and 191,992. Its replacement resumes
at Original frame 60, which is silent; the other three join sides and final
suffix contain nonzero decoded PCM. This does not establish acoustic quality.

## Native release check

The separate optimized Replacement QA app copied Original `[10..24)`, selected
Edit `[30..60)` and opened `:splice`. Insert was the default; pointer activation
selected Replace. The proposal showed removal `[30..60)`, insertion `[30..44)`
and a −16-frame change. Settled displayed Edit frames 30, 31, 44 and 45 showed
slates 029, 010, 023 and 060, respectively, with matching requested captions.

One Enter committed a 104-frame edit with five children and the new 14-frame
Source at `[30..44)`. One `u` restored 120 frames and three children. SQLite
backup comparisons confirm one `replace_source` transaction and one Undo,
complete authored-document restoration except the fresh revision, unchanged
rows in all 16 unrelated tables and a released writer lock after shutdown.
The user's Cursor QA app and its desktop/window placement were untouched.

Native accessibility exposed the slice controls, and keyboard selection,
counted movement, picture inspection, commit and Undo worked. This check makes
no claim about acoustic output, loop wrapping, physical IME or VoiceOver.
The native screenshot cut off at its right edge, including some footer hints;
the available capture API exposed no window bounds to distinguish capture
cropping from a window extending beyond the display. No inaccessible control
was established. Native screenshots were inspected in the automation session
but could not be exported. Retained layout evidence comes from the two fully
inspected offscreen captures, so native layout qualification remains limited.
The developer bundle contains the app icon but still relies on the external
pinned FFmpeg libraries; it does not qualify distribution packaging.

## Review corrections and failed checks

Independent review found two defects, now covered by regressions:

- The heading's later click region covered the new Replace/Insert button. It
  now excludes the button. Production pointer checks exercise both directions
  and return focus to the keyboard heading.
- Fast paste assigned its committed receipt after refreshing the workspace.
  A refresh failure could hide a saved edit and label cold preparation failed.
  The receipt now survives; the message explicitly says the edit was saved but
  the preview could not refresh. Cached and cold tests verify one durable edit,
  stale retry rejection and exact Undo.

The first compilation exposed a test-local `id` binding shadowing the fixture
helper. Renaming the binding fixed compilation. Two new core sound fixtures
initially supplied a Source with no media; the existing validator correctly
rejected them. They now use qualified event audio. The first app media test
incorrectly required nonzero sound after replacement's exit join at Original
frame 60; correcting that fixture expectation retained the independent nonzero
suffix comparison. No production audio behavior was changed for these failures.
Debug linking still reports the existing oversized `__eh_frame` warning.

The first full workspace run passed 2,311 tests before six store unit tests
failed in setup: they loaded archived core-33/database-40/41 data. Those tests
now create current-format projects through typed store APIs, with explicitly
synthetic media evidence. The five publication durability tests retain their
assertions; the audit retains three jobs, 26 attempts, 23 checkpoint retries
and two stored revisions. Archived SQL fixtures and runtime format refusal
remain unchanged. The current SHA-256 digest type does not implement `LowerHex`;
the new test helper uses explicit byte formatting after its first compile error.

Ordinary Delete's separate suffix-phase behavior was identified during design
but is outside this replacement change and remains open. Native Render-overlay
accessibility and the measured debug audio-starvation limitation also remain.

## Evidence

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-slice-replacement/)
records command lines, failures, source manifests, reviews and selected captures.
The fixture is `cfr-bframes.mp4`, 30000/1001 fps, decoded through the pinned LGPL
FFmpeg prefix on Apple M5 Max, 128 GiB, macOS 26.5.2, Metal and Rust 1.97.1.
Report metadata records the base commit, diff digest and source hashes.
