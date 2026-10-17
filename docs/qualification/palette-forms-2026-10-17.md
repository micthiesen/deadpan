# Independently selectable palette actions, 2026-10-17

The searchable Keys sheet now exposes distinct verbs and editor directions as
separate choices. Searching `:redo` runs Redo; choosing Previous frame moves
backward. This closes the two-form palette gap in DP-06. Other DP-06 work remains
on the requirement tracker.

Base revision `2cc6cdb3`, Apple M5 Max / Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1 and qualified FFmpeg 8.0.3 dependencies.

## Behavior

The grouped reference registry remains the authority. Palette forms retain a
parent action and an exact command or binding index through highlighting,
clicking and deferred execution. Execution rechecks the current context and
binding eligibility. An unavailable direction explains its limitation; it never
substitutes a usable sibling. The unfiltered reference remains grouped.

Distinct command forms include history, paste, J-/L-cut, retime, sound controls,
AI variants, recipes and macro recording. Key-only forms include pane, frame,
beat and semantic motions, object selectors and repeat actions. Gain steps and
AI comparison keys remain independently executable beside their parameterized
commands. True command aliases share a row.

Each row has its own name, keys and widget identity. Configured key aliases and
native Undo/Redo and jump-history shortcuts remain searchable. Exact full chords
rank before prose, including chords with spaces. Shared explanatory text remains
searchable at lower priority. Existing command-entry captures, prefix dispatch,
IME ownership and deferred-input ordering remain in use.

## Verification

The scoped gate passes 39 tests, and the same 39 pass with `ui-harness` enabled.
Tests cover every registered executable form, unique sibling names/identities,
command aliases, custom key aliases, native chords, context rejection and
retention of an exact choice when search results change. Strict Clippy passes
in normal and UI-harness configurations. Independent review found no remaining
implementation issue.

The first release visual attempt stopped at a new harness assertion that assumed
a split adds one node. The actual split added three structural nodes and committed
successfully. The harness now compares exact authored node maps before the split,
after Undo and after Redo. The failed report is retained. Subsequent review also
removed a duplicated result title and made paint assertions identify the exact
row and keycap instead of unrelated matching text behind Help.

Final release visual and performance `palette` replays each pass **34 checks**
plus the production Kestrel audit against the local shortcut source. The replay
proves durable Undo/Redo, separately painted direction rows, a clicked Previous
frame and an exact-key Next frame choice. Existing command prefill, prefix,
selection and unavailable-context checks also pass. Full-size Undo/Redo,
Previous/Next frame and disabled Trim captures were inspected: names, keycaps,
selection fill, search focus and explanation are visible within the existing
dark workspace design. No screenshot baseline was automatically accepted.

Performance mode recorded 73 input-to-state samples: median **0.693 ms**, p95
**19.334 ms**, maximum **27.058 ms**, with no failed or timed-out samples. These
are offscreen replay measurements, not physical input-to-display latency. Both
runs retain the existing settled layout-retry warnings; the visual run also
reached its intermediate-capture allowance while retaining named checkpoints.
No layout failure was ignored.

Executed release app SHA-256:
`552cb62b87f1c40dcedeee9b549d03b4a16dcbaf2cd3dd74cc4f5f794f5cb96b`.
The [evidence](../../tools/ui-feedback/evidence/2026-10-17-palette-forms/)
retains source/binary identities, checks, initial failure, final reports,
inspected captures and a verified inventory. Final strict lint checks include
the last display and harness corrections.

Physical key delivery, native pointer hit-testing, VoiceOver speech and other
keyboard layouts are outside this replay's scope and remain owner verification.
No export, media timing or analysis behavior changes in this slice.
