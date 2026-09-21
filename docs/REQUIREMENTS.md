# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

**Open** means required behavior has no qualifying implementation. **Partial** identifies concrete groundwork while acceptance remains unmet. **Complete** requires linked code, passing relevant tests, and a demonstrable acceptance result. No requirement or delivery gate is complete. Baseline checks are described in [Development](DEVELOPMENT.md); they are not substitutes for full acceptance evidence.

Current measured evidence: [editing foundation verification](FOUNDATION_VERIFICATION.md) and [native media qualification](qualification/media-2026-09-20.md).

## Product requirements

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Project creation, reopen, autosave, undo/redo, migration, recovery. | Partial | [`deadpan-store`](../crates/deadpan-store/): SQLite packages, immediate durable commands, retained revision/history branches, fresh-revision undo/redo, single writer, and live database checkpoints. [Tests](../crates/deadpan-store/tests/persistence.rs) exercise reopen, rollback, crash, and WAL backup. | Managed-media lifecycle, migration/restore/recovery, history limits, full failure/chaos suite, and native document workflow. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | [`time.rs`](../crates/deadpan-core/src/time.rs): validated typed timing, rational conversion, ranges, unit/property tests, and a 10,000-boundary fractional-rate drift test. [Native media harness](qualification/media-2026-09-20.md) measures VFR PTS, seeks, AAC priming/padding, and exact impulse positions. No application source index yet. | Source mapping through nested edits and actual shared playback/export, including 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | [`document.rs`](../crates/deadpan-core/src/document.rs), [`command.rs`](../crates/deadpan-core/src/command.rs), and [tests](../crates/deadpan-core/tests/document.rs): flat validated tree, exact nested durations, immutable asset records, structural commands, reversible patches, bounded JSON, and serialization/inverse properties. | Indexed render-plan compiler, semantic range selectors, and golden picture/audio selection for all primitives and nested structures. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Open | None. | Structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Open | None. | Binding matrix and keyboard-only session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Open | None. | Parser/transaction/replay tests. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats, and change Hold duration/provider. No rendered creative operation or interactive inspector yet. | Remaining operations, semantic targeting, recipe fixture renders, and editable inspector demos. |
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
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [Headless API](HEADLESS.md), [CLI integration tests](../crates/deadpan-cli/tests/project_commands.rs), and [native headless tests](../crates/deadpan-app/tests/headless.rs): project creation/validation/dump, typed commands, revision checks, read-only dry runs, undo/redo, checkpoints, and structured errors. Both executables share one entrypoint. | Complete command/selector surface, host socket routing, plan/render operations, and headless/GUI parity. |
| DP-22 | Signed/notarized zero-manual-setup distribution. | Open | None; source development builds are not an application distribution. | Clean-machine online and offline acceptance. |
| DP-23 | License/SBOM/privacy/security requirements. | Partial | [Dependency inventory](DEPENDENCIES.md), native harness build/license hashes, strict bounded domain JSON, schema checks, and initial package-path protections. Qualification explicitly excludes the developer GPL FFmpeg build from distribution. | Release audit, complete hostile-project/worker/pack tests, SBOM/notices, privacy checks, and exact shipped component licenses. |
| DP-24 | Measured performance budgets and diagnostics. | Partial | `doctor` reports actual core/SQLite probes. [Native harness](qualification/media-2026-09-20.md) measures tiny fixture decode/seek/encode on recorded hardware; these are qualification observations, not product budgets. | Full-size playback/edit/export/inference benchmarks, latency distributions, diagnostics, memory pressure, and published reproducible product measurements. |

## Delivery gates

[Specification Section 30](spec/DEADPAN_SPEC.md#30-implementation-workstreams-and-delivery-gates) defines the complete ordered build plan. Gates may have parallel work behind their interfaces; none permits calling an earlier subset the completed product.

| Gate | Status | Required work and exit evidence |
| --- | --- | --- |
| A: Qualify risky dependencies | Partial | [Media qualification](qualification/media-2026-09-20.md) includes real encode/decode/seek/audio fixtures, sanitizers, and pinned rsmpeg/Cutlass comparisons. Hardware B-frame muxing, rsmpeg/FFmpeg 9, and Cutlass source-offset contracts failed. Still required: shipping media build/bindings and format/color matrix, GPU viewport, audio DSP/output, model candidates, private-runtime packaging, full-size hardware benchmarks, and model-pack qualification. |
| B: Establish the pure editing foundation | Partial | Typed time/nodes, structural commands/inverse transactions, schema, SQLite history, headless validator/dump, and generated/property fixtures exist. Still required: anchors, nested instance paths/overrides, semantic selectors, and indexed render-plan compiler. Representative nested edits must render through a test backend with exact frame/sample selection; serialization and inverse transactions must preserve all authored meaning. |
| C: Build the interactive media workspace | Open | Actual decode/index/proxy paths, audio, GPU preview, panes, keyboard grammar, selection, inspector previews, durable history, focus/IME, and accessibility. Edit and audition real footage through keyboard commands with measured latency and no drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Open | Bundled yt-dlp/EJS/Deno, provenance, safe updates, automatic output, HDR/SDR and codec/mux verification, notices, signed runtimes, notarization, recovery/migration, and disk/permission failures. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
