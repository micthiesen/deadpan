# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the setup foundation, not a completed product or reduced release scope.

**Open** means required behavior has no qualifying implementation. **Partial** identifies concrete groundwork while acceptance remains unmet. **Complete** requires linked code, passing relevant tests, and a demonstrable acceptance result. No requirement or delivery gate is complete at setup. Baseline checks are described in [Development](DEVELOPMENT.md); they are not substitutes for full acceptance evidence.

## Product requirements

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Project creation, reopen, autosave, undo/redo, migration, recovery. | Open | None. | Persistence/crash/migration suite. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | [`time.rs`](../crates/deadpan-core/src/time.rs): typed timing, rational conversion, ranges, unit/property tests, and a 10,000-boundary fractional-rate drift test. No VFR/source index or media synchronization implementation. | Source-mapping property tests and encoded sync fixtures, including VFR and 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | [`time.rs`](../crates/deadpan-core/src/time.rs): repeat-duration equation and tests for total plays, absent trailing gap, and overflow only. No structural nodes or render plans. | Golden render-plan and duration tests for all primitives and nested structures. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Open | None. | Structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Open | None. | Binding matrix and keyboard-only session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Open | None. | Parser/transaction/replay tests. |
| DP-07 | All time/delivery operations in Section 8. | Open | None. | Recipe fixture renders and editable inspector demos. |
| DP-08 | All framing/picture operations and keyboard target selection. | Open | None. | Tracking/geometry/interaction tests. |
| DP-09 | All audio operations with preserved intentional dynamics. | Open | None. | PCM/gain/tail/stretch fixtures. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Open | None. | Analysis accuracy and correction tests. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Open | None. | Occlusion/shot-change fixtures. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Open | None; no inference backend or model pack. | Actual qualified model corpus, not mocks. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Open | None. | Clean-machine and interrupted-install tests. |
| DP-14 | YouTube URL import with bundled JavaScript support. | Open | None. | Clean-machine permitted-source import. |
| DP-15 | Local media import, managed/linked assets, relinking. | Open | None. | Ownership/relink/failure tests. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Open | None; a native UI window does not qualify a media renderer. | Preview/export comparison and stress benchmarks. |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Open | None. | Encoded-file metadata/pixel/sync verification. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Open | None. | Worker chaos and concurrency tests. |
| DP-19 | Cache integrity and accepted-media portability. | Open | None. | Eviction/reference/offline-project tests. |
| DP-20 | Accessible, native-behaving, simple UI. | Partial | [`deadpan-app`](../crates/deadpan-app/): native development welcome shell only. | Accessibility inspection and keyboard acceptance for the full workflow. |
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [`deadpan-cli`](../crates/deadpan-cli/src/main.rs): headless `doctor` JSON diagnostics; [integration tests](../crates/deadpan-cli/tests/doctor.rs) cover real timing probe, missing-capability reporting, and invalid-command failure. No project commands, revision checks, or dry-run API. | Headless/GUI parity and conflict tests. |
| DP-22 | Signed/notarized zero-manual-setup distribution. | Open | None; source development builds are not an application distribution. | Clean-machine online and offline acceptance. |
| DP-23 | License/SBOM/privacy/security requirements. | Open | Initial repository licensing/dependency documentation is groundwork only. | Release audit and malicious-input tests for the exact shipped application, helpers, and packs. |
| DP-24 | Measured performance budgets and diagnostics. | Open | `doctor` scaffolding does not yet report an active media/model pipeline or measure product budgets. | Published reproducible hardware benchmark report. |

## Delivery gates

[Specification Section 30](spec/DEADPAN_SPEC.md#30-implementation-workstreams-and-delivery-gates) defines the complete ordered build plan. Gates may have parallel work behind their interfaces; none permits calling an earlier subset the completed product.

| Gate | Status | Required work and exit evidence |
| --- | --- | --- |
| A: Qualify risky dependencies | Open | Actual macOS media decode/seek/encode, GPU viewport, audio DSP/output, model candidates, private-runtime packaging, and Cutlass extraction comparison. Deliver pinned dependency/license inventory, isolated harnesses, hardware benchmarks, and model-pack qualification matrix. UI dependency setup alone does not pass. |
| B: Establish the pure editing foundation | Partial | Exact timing groundwork exists. Still required: nodes, anchors, instance paths, selectors, commands, inverse transactions, project schema, render-plan compiler, headless validator, deterministic dump, and generated/property fixtures. Representative nested edits must render through a test backend with exact frame/sample selection; inverse transactions and serialization must preserve meaning. |
| C: Build the interactive media workspace | Open | Actual decode/index/proxy paths, audio, GPU preview, panes, keyboard grammar, selection, inspector previews, durable history, focus/IME, and accessibility. Edit and audition real footage through keyboard commands with measured latency and no drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Open | Bundled yt-dlp/EJS/Deno, provenance, safe updates, automatic output, HDR/SDR and codec/mux verification, notices, signed runtimes, notarization, recovery/migration, and disk/permission failures. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
