# UI feedback evidence

Read the [qualification](../../../../docs/qualification/ui-feedback-2026-09-26.md)
for the results and boundaries. [The run guide](../../../../docs/UI_FEEDBACK.md)
defines the ongoing agent workflow.

`summary.json` is derived from three unmodified scratch reports. It retains their
SHA-256 digests, build/environment identity, every check outcome and expectation,
failure values, selected diagnostic values, raw samples for the named performance
budgets, and named checkpoint state. It omits full per-frame widget/stage dumps.
The complete contact sheets and traces remain at the source paths recorded in it.

| Run | Result |
| --- | --- |
| `visual-06` | Nine UI scenarios plus shortcut audit passed; 358 assertions. |
| `performance-02` | Navigation budgets passed; Repeat and Hold picture feedback exceeded their p95 budgets. |
| `playback-08` | Actual error paint and baseline comparison passed; first-frame clipping and visual differences remain warnings. |

Representative captures:

- `workspace.png`: real submitted video and selected inserted beat after pointer monitor adjustment.
- `large-project-minimum.png`: selected final beat in the 10,000-beat fixture at 960×640.
- `playback-error-entry.png`: the injected device error first appears partly below the viewport.
- `playback-error-visible.png`: the next frame shows the whole notice. Playback service updates are simulated.

Verification logs retain failed attempts as well as successes. The workspace test
run and its focused playback retry timed out. The unchanged workspace playback
binary later passed that exact test; the cause of the earlier timeouts is not
established. Tests not reached after the workspace failure passed as their actual
compiled test binaries. This is not a clean full-workspace run.

Final app feature tests passed 180 unit and two integration tests. Formatting,
default and feature Clippy, workspace build, CLI doctor and native startup/shutdown
smoke passed. The smoke test does not establish native key delivery, IME,
VoiceOver, audio output or physical presentation latency.
