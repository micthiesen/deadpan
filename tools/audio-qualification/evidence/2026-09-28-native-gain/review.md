# Native gain review, 2026-09-28

Terminal release outcome: locked workspace build passed in 1,072.66 seconds;
complete performance replay passed 2,156 checks across 17 scenarios plus the
shortcut audit in 26.88 seconds on final source533439a4. Warm navigation,
Repeat and silent Hold picture p95 values were 6.356, 9.438 and 10.769 ms;
10,000-beat navigation CPU p95 was 1.792 ms. These are small-fixture offscreen
measurements, not physical display or listening acceptance.

Parent owns all compiler/test/GPU executions. Independent agents own scoped
implementation and source review only. No process was restarted on a polling
timeout. The initial base Clippy invocation exited 101 after 58.29 seconds with
six collapsible-if lints and one argument-count lint. All seven were corrected
without suppressions. The initial UI-feature Clippy invocation exited 101 after
692.71 seconds on a harness Window value/reference mismatch. The corrected
full workspace/all-target UI-feature Clippy invocation passed in 274.59 seconds.

Reviewed boundaries:

- Gain target captures session, project, revision, direct-child Sequence scope,
  node, retained Edit cursor and complete entry treatment, including absent
  target rejection before command text is entered.
- Writer previews use a fresh ordinary command revision and forward patch;
  proposals never become the authoritative workspace or a history entry.
- Media admission remains anchored to committed receipts, with a private
  proposal witness, separate content identity and exact cache/resume checks.
- UI keeps one pending proposal and coalesces subsequent recipe changes.
- Before/Draft uses one immutable Sequence window and admitted heard samples.
- Apply uses normal durable commit and retains the draft on rejection.

Accepted findings and disposition:

1. Obsolete proposal errors also reached the untagged global error slot. Fixed:
   PrepareGain returns success after publishing its identity-tagged result,
   keeping failures exclusively inside that result. Service tests assert this.
2. Native Tab could escape the bottom draft panel into normal editor controls.
   Fixed: disable the header and surrounding workspace while retaining picture
   opacity. Draft controls remain focusable. Popups retain shortcut ownership.
3. Pending/invalid field text disabled Pause during ongoing comparison playback.
   Fixed: Pause is always available while running; Restart requires applied,
   valid fields. The production replay now exercises invalid text while running.
4. Cancel unnecessarily reset the accepted picture. Fixed: schedule the restored
   entry frame without clearing the accepted GPU target. Replay holds preview
   delivery while checking same-frame retained picture identity.

Independent playback review found no additional concrete issue in Snapshot
proposal admission, complete asset equality, Sources cache matching, tagged Job
updates, Run/Resume identity, or real-media test permit ownership. Tests retain
the permit through both detached workers via their shared callback; Engine::drop
alone does not release it. No review agent executed a compiler, test or GUI.

Source review found no additional reachable duplicate-history, captured-target,
proposal-admission, coalescing or heard-clock defect. These are review findings,
not a substitute for compiler, test, native or visual evidence.

The first Metal replay exposed a 50-point-high picture while the gain draft was
open, an obsolete rejected-target error after valid draft entry, vertically
wrapped graph-key labels, and a replay selector that searched only labels while
the native curve ComboBox exposes its selected text as a value. The picture now
uses a compact owner row and omits inactive moment/sound controls while retaining
stable panel IDs. Successful entry clears the obsolete error. Graph keys retain
single-line labels; the replay resolves the actual ComboBox value and bounds its
diagnostics. Viewer minimum-height and clipped-paint checks cover both sizes.

Choosing the active Before/Draft tab now returns before stopping playback. The
replay checks the unchanged ticket, phase, generation, exact sample, window and
content identity. Restart remains explicit. The inspector reads the draft recipe
and labels it unsaved while the draft is active.

Independent review of these layout changes found no further concrete issue in
panel IDs, footer retry, focus containment, inspector state or comparison routing.
The failed visual runs remain retained. An intermediate layout measured a
149-point picture at 960×640. Reserving the complete graph later leaves a
qualified 145-point picture there and 270.1875 points at 1280×820, with the
heading at x=12. Explicit heading/Cancel wrapping removes an empty Tab stop.
Newly focused fields reveal their label/input union; four populated forward and
reverse circuits pass at both sizes without wheel assistance.

The complete visual suite retained two failures: strict picture containment
rejected a 0.00001526-point f32 residue, and new generic gain controls pushed
saved Hold audio facts below its minimum-window inspector clip. The former now
uses a finite checked 0.01-physical-pixel picture-only tolerance with four pure
regressions. The latter puts Hold/group/parameter actions before gain and reveals
gain buttons on actual AccessKit focus. Normal Tab remains pane cycling.
Independent production and regression review is clean. Corrected focused replays
pass workspace81, roomtone103, gain266 and Retime16 checks plus their audits.

Final source533439a4ece260aa1ee56751987ff765d8cd3601c52eafa0f323743a29b41dbe
passes format, strict lint in both configurations,298 feature-app tests and263
normal-app tests. The earlier complete workspace2029 run remains valid for its
unchanged backend; only ten app/UI source files differ, covered by final app
checks. Native command/focus/text/envelope/cancel review passes with a byte-equal
project dump afterward; native/review.md records the exact boundary.

One broader design finding remains outside this gain checkpoint: in the minimum
normal workspace, copied Original controls and an empty64pt Placed sounds strip
compress the picture substantially. Consolidating that empty focus entry into
the timeline heading is the next bounded layout increment. Measured waveforms,
physical keyboard/IME, VoiceOver, listening and full-scale media remain open.
