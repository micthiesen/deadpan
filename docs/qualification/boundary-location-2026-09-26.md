# Exact boundary location qualification, 2026-09-26

`AnchorIndex::locate_boundary` and the shared headless `locate-boundary` command
report an exact project boundary's complete structural owner path. This supplies
information needed for nested editing; it performs no edit and does not complete
the arbitrary splice requirement. Core schema 25/database schema 31 are unchanged.

The path retains every Sequence, Retime and Repeat owner, exact local positions,
full durations, original Sequence slots and stable iteration identities. It
distinguishes play entries, owned gap branches and implicit gaps. Boundary bias
selects the adjoining content, with explicit outward project endpoints. Fractional
Retime coordinates never round. One scope/comparison budget spans the path, and
billion-play fixtures remain compact. Existing mark transforms reuse the same
positive-duration Sequence prefix index without changing their relocation rules.

## Environment and review

macOS 26.5.2 build 25F84, arm64; Rust 1.97.1; pinned developer FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. Cargo jobs were serialized in the shared checkout.
Git HEAD was `c03a5edde5f28d27074745eb15711cb28b1f2e50`; the increment is measured
against the preceding interior-insertion source checkpoint. Git metadata remains
read-only, so no commit or push was possible.

Two independent read-only reviews covered general correctness and exact boundary
timing. Neither reported a concrete finding. They inspected persistent-mark
prefix equivalence, zero-duration slots, cropped and partitioned clocks,
Repeat/gap identities, strict CLI requests and shared work bounds. The parent
reviewed the source diff and command tests. Review is separate from test evidence.

## Verification

[Retained evidence](../../tools/media-qualification/evidence/2026-09-26-boundary-location/README.md)
contains the incremental patch, source manifests, raw logs, review results and
public-command records. All 501 source/config paths matched before and after the
required gate. No source changed between the reviewed tests and the final checks.

| Check | Result |
| --- | --- |
| Anchor suite | 35 passed, including nine locator tests: fractional owner clocks, inverse-coordinate checks, empty slots, cropped/partitioned domains, nested gaps, stable billion-play lookup, maximum depth, work limits and an independent expanded half-frame reference for both biases. |
| CLI boundary suite | 5 passed: strict envelopes, exact roundtrip, writer coexistence, gap/play entry distinctions, endpoint bias, budgets, stale revisions and unchanged history. |
| Required formatting and strict workspace lint | Passed. |
| Exact workspace tests | 910 passed, 1 failed, 0 ignored. The run stopped at the sandbox-denied Unix socket test; later targets/doctests were not reached by this invocation. |
| Complete store/plan suites | 375 passed, 0 failed, 0 ignored across 32 result groups, including all 88 migration tests. |
| Workspace build and doctor | Passed; doctor reports core 25/database 31. |
| Optional app/harness feature | Strict all-target lint passed; 194 unit and 2 integration tests passed. |
| Native-app/CLI headless parity | 55 invocations across seven boundary scenarios and refusal checks passed. Every emitted owner mapped back to the same exact project coordinate. SQLite bytes, two revisions, one fixture history entry and the empty redo stack remained unchanged. |
| Design assets | All 10 ImageGen boards and exact prompts matched their retained hashes. |

The workspace failure remains
`directories_fifos_and_sockets_are_rejected_without_blocking` at
`deadpan-jobs/tests/artifact.rs:200`: `UnixListener::bind` returns OS error 1,
`PermissionDenied`/`Operation not permitted`, before socket-admission behavior
can run. It is the previously recorded sandbox restriction. The test was not
ignored, bypassed or reported as passing.

The headless parity run used native-app SHA-256
`8b2d6978c3f5b9cf48b92015f6d60525996fbaa996afe30e842cf988e718abd9`
and CLI SHA-256
`9ea6449a99c1a32dab2b0eba20bfeca9f3e1dbcde2615efff769a219948ec361`.
Its fixtures use synthetic Background/Silence structure and make no new decoded
media or GUI claim. Initial exploratory compilation caught a test module-path
typo and a test using `ProjectFrame` where Split expects `FrameDuration`; both
were corrected before the frozen required gate.

## Remaining work

The [splice design](../STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent) records
the next mutation obligations: outside-in occurrence isolation, exact fractional
selections, live enclosing effects during inserted time, mark ownership,
affected-suffix audio resume and one atomic reversible command. Integer output
partitions and static captured framing cannot represent every nested case.

No GUI code or lifecycle behavior changed. Visual replay, native startup smoke,
VoiceOver, IME, device output and release latency were not rerun for this query
increment. The earlier Metal-adapter limitation and existing UI/performance
findings retain their status. All 10 ImageGen target boards and exact prompts
remain intact. No product requirement or release gate changes status.
