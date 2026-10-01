# Native Slip replay review

Reviewed the unexecuted `/tmp/deadpan-native-slip-20261001/harness/harness.patch` against the final service/UI scratch sources and the current replay driver APIs. The patch checksum at review was `4d56edac59a1aafdbbdf3bb5cd23bc15f12ec091fd8f5a2d2a53d054ecc52de5`.

## Findings

No actionable harness correctness findings.

The picture expectations are independent of the Slip result: the replay selects Original ordinals `[10,24)`, expects Edit frame 3 to show ordinal 13, then checks +5 -> 18, a -100 request clamped to -10 -> ordinal 3, reverse nudge -> 4, and the `l/l/h/Shift-l` batch -> +2 -> ordinal 15. First/last inspection at +2 expects ordinals 12 and 25. After splitting at Edit frame 5, the right Partition begins at Original ordinal 15, so +2 at its first frame expects 17. `exact_picture` checks the decoded `SourceFrameId`, the independent measured-index PTS and time base, decoded/displayed identity and geometry revision, plus the current proposed or committed presentation identity.

The replay waits for a real prepared proposal and current submitted Proposed picture before Apply, withholds/releases actual worker replies and service updates, and compares the committed document against the candidate snapshot. It then checks one Undo/Redo with fresh revisions and exact authored nodes/sample bindings. The held service update is retrieved directly only while normal feedback delivery is disabled, then delivered after cancellation. Native text, IME batches, focused controls, and held Enter repeats are sent through the production event router. Assertions return errors immediately, and the replay can’t pass its picture or commit checks without the intended presentation/transaction state.

The compact-size capture occurs immediately after changing viewport size and before `wait_ready`; the subsequent handle, text-visibility and picture checks wait for the stable proposal. This is acceptable for the transition screenshot, but the saved frame may show the prior retained picture if resize submission has not completed in that one step. The root’s inspected captures and final replay should remain the evidence for the stable clamp state.

## Verification

Read-only review; I did not run Cargo, native UI, or the replay. The root reports the focused tests and real visual replay passed. No further execution was needed for this review.
