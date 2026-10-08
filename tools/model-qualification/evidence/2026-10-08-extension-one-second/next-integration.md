# Next integration: normal Extension generation

Read-only map of the current checkout, 2026-10-08. No builds, inference or repository edits were performed for this map. Spec §12.2 requires one-sided conditioning with an absent or explicitly unconditioned opposite seam; §12.3 forbids inferring that capability from image-to-video or Bridge support.

## Recommended coherent slice

Carry one captured operation through the existing normal generation path, from CLI/native intent through runtime selection, immutable input preparation, request allocation, supervision, full qualification, publication, Ready, Preview and explicit Accept. Enable a separately qualified K9/E24 Extension pack only after the normal path and its packaged smoke check pass. Reuse the current writer/job split, inference slot and store APIs. No new scheduler or provider framework is needed.

Use three small operation enums at existing boundaries:

1. `PreparedInputs::{Bridge(BridgeInputs), Extension(ExtensionInputs)}`. Common accessors expose the existing `GenerationPlan`, constraints, manifest/hash and retained input descriptors. The sum owns its operation; callers cannot pair Bridge PNGs with an Extension plan.
2. An immutable selected runtime carries its manifest plus `SelectedGenerationProvider::{Bridge(SelectedBridgeProvider), Extension(SelectedExtensionProvider)}`. Construct the Extension capability from `manifest.constraints.extension.capability(plan.project_frame_rate())`, never from a worker declaration or the compiled Bridge capability. Rebuild it for each project grid; its output limit is grid-specific. Keep the manifest snapshot fixed through variants and rollback.
3. `QualifiedBundle::{Bridge(QualifiedBridgeBundle), Extension(QualifiedExtensionBundle)}` and matching retained-conditioning variants. Dispatch to existing qualifiers, then expose an owned iterator of all retained objects for publication. Do not flatten the different provenance or conditioning contracts into optional fields.

The existing `GenerationPlan`, `GeneratedSamplingMap`, store receipt and accepted-provenance sums already cover both operations. Existing store request/preparation/lifecycle/acceptance tables do not need another schema change for this slice.

## Exact Bridge assumptions to replace

| Boundary | Current assumption | Required change |
| --- | --- | --- |
| `generation/attempt.rs:117–344` | AllocateInput/Allocated own BridgeInputs; current lookup filters bridge_plan; allocation calls record_scoped_bridge_generation_request; message/provider extraction assumes GenerateBridge/V2 | Use operation sum, `record_scoped_generation_request(..., GenerationPlan)`, exact variant input equality and GenerateExtension/V3. Preserve immutable origin revision/target for retries. |
| `attempt.rs:533–584` | Workspace writes left.png/right.png and captures RetainedConditioning | Extension writes chronological PNGs, optional opposite, manifest and continuity.bin; pin first and call capture_extension_conditioning before launch. |
| `attempt.rs:591–940` | Lifecycle V2, CompletedBridge-only contract check, SelectedBridgeProvider built from declaration plus compiled development capability, qualify_bridge, Bridge receipt | Derive protocol from allocated request; reject cross-operation completion; retain host-selected provider/capability and dispatch to qualify_extension and new_extension receipt. Preserve cancellation, record acknowledgements and confirmed teardown. |
| `attempt.rs:954` | Publication destructures exactly manifest/left/right | Publish every retained context PNG, optional opposite and signatures before Ready. `native/deadpan-media-worker/tests/extension_quality/saved.rs:316–343,474` already demonstrates the real Extension receipt/publication flow. |
| `generation/runtime.rs:103,225–552` | installed_pack/default fallback/install guidance always BRIDGE_PACK; with_model_manifest accepts one shared runtime path | Select the pack by captured operation. Share physical Python/source/codec paths but bind pack ID, operation and runtime compatibility independently. No packaged developer fallback; no Extension selection through installed Bridge. |
| `runtime.rs:553–647`, `runtime/launch.rs` | --check uses Bridge assumptions; worker configuration omits constraints; arbitrary JSON report accepted | Add operation-aware check configuration/report admission with exact pack/runtime/manifest hash and Extension envelope. Keep the same network-denied launcher. |
| `models.rs:517` | Only BridgeHold triggers the AI smoke check; Extension-only pack would skip it | Require an Extension smoke check before activation. Cover both operations if a future manifest declares both. |
| `generation/command.rs:92–148`, `command/options.rs`, `preparations/command.rs:118` | Bridge runtime and preparation; no --mode option | Resolve Automatic from exact scoped endpoint presence, expose explicit mode, select matching runtime, prepare operation-tagged inputs. `--another` retains operation/provider/input identity. Apply the same selection to automatic replacements. |
| app `project/service/generation.rs:67,667–740,1344–1454,2147–2240`; `generation/preparations.rs:309` | Prepared event/worker are Bridge types; validates every option against Bridge; recapture and request matching assume Bridge | Use the shared CLI preparation/selection path on the existing bounded job thread. Writer still allocates and publishes; update controls/status only for the captured session/revision. Automatic preparations must use their captured intent and resolved options, not force Bridge before those options are read. |
| app `project/generation.rs:255`, `preview/ai_pause/timing.rs:29` | Running-job timing holds BridgeGenerationPlan | Use GenerationPlan; candidate/accepted Extension timing and quality readers already work. |
| app `navigation/command/generate.rs`, `preview/ai_pause.rs:1900,2018,2368`, `preview/model_packs.rs:73` | Grammar hardcodes Automatic; tooltip says both sides; installer offer always Bridge | Expose Automatic/Bridge/Extend left/Extend right in existing command controls, show actual resolved mode and unconditioned/absent seam, offer the corresponding pack, preserve key discoverability. Existing Preview/Accept/variants UI can be reused. |

`GenerationModePreference::resolve` already implements the correct policy: both endpoints => Bridge; only left => ExtendFromLeft; only right => ExtendFromRight. It refuses unsupported selected operations rather than silently choosing another. `GenerationInputCapture::for_preference` already persists K9/24fps temporal intent independent of installation. `fulfil_generation_preparation` already accepts `Into<GenerationPlan>` and checks its captured operation/input binding. Do not rewrite those mechanisms or use a saved freeze as proof that a missing endpoint exists.

## Runtime and pack approval

Current Python inference dispatches GenerateExtension, but `mlx_backend._model_manifest` admits only the development Extension pack/runtime identities. `worker_media.EXTENSION_RUNTIME_GENERATED_FRAMES` separately pins dev1 E8 and dev2 E24. `mlx_backend.check_runtime` defaults runtime_paths to Bridge and imports KeyframeInterpolationPipeline; it does not establish Retake availability. Add a distinct production identity and explicit Extension check importing the pinned Retake path, checking the exact tensor layout/operations/envelope and executing Metal. Preserve the existing Bridge identity and its checks.

Create the Extension approved manifest independently, with its own pack/runtime identity and qualification report. Reuse exact verified model files/licenses where appropriate; shared bytes do not share operation approval. `tools/ai-runtime/pins.json` currently names only `0.15.8+deadpan5`; the bundled runtime compatibility report and assembly need to record the added Extension adapter identity. `tools/ai-runtime/build.py` already includes worker_extension_context.py. Run relocated packaged checks with all developer environment variables absent and retain executable/adapter/manifest hashes.

Minimum evidence before approving this K9/E24 pack:

- Both actual model directions through the new normal path and exact one-second envelope; retain generated-only full-pixel oracle, immutable input checks and complete quality disclosures. The existing real runs establish dev2 at N30/P30 only; do not relabel them as the new normal/packaged identity.
- Sampling/canonical output checks for N1, fractional project rates and maximum supported output count using the retained native movie. Reject N beyond the exact ceiling before model loading. This need not rerun expensive inference for every sampled count.
- Explicit Ready/Preview/Accept, undo/redo, copied/shortened Hold, cold reopen and verified encoded export of a real result, with model unavailable after acceptance.
- Runtime smoke, pack import/install/activation/rollback, absent/damaged pack and mismatched provider cases; independent operation refusal before launch. Extension updates retain their new strict data guard.
- Cancellation, stale edit while queued/running, automatic replacement/retry and one-play Repeat targeting; network restriction and owned worker cleanup unchanged.
- Measure every advertised motion setting. Initial K9/E24 evidence is Still with synthetic footage; do not advertise Subtle/Moderate as measured from that run. Unavailable motion/face/mouth observations remain explicit. Real-person quality/corpus judgement stays on the owner verification list under §29.1. Neither existing latency nor this slice warrants Fast labeling.

## Focused tests and division of work

- CLI attempt/runtime/command tests: both operation variants, wrong completion/protocol, provider/version/manifest mismatch, missing signature/PNG, retained input tampering, current-request reuse and rollback separation. Extend synthetic attempt fixture to emit valid V3 so persistence/cancel/failure tests exercise the production path.
- Models smoke/Python tests: Bridge check cannot satisfy Extension, exact production identity/counts/duration, Retake import/source pins, staged pack check and invalid report identity.
- App service/replay tests: edge Automatic selects the right direction, interior explicit Extension reports the opposite seam, retained command target and scope, jobs/variants/acceptance and nonblocking edits. Run the existing native AI flow with a real packaged candidate; no new shortcut is necessary, but command help/visible controls/replays must agree.
- Reuse the already-passing store Extension tests and saved-media fixture. Broaden only where normal dispatch introduces a new failure mode.

Suggested independent ownership after fixing the shared enum/API contract: (A) CLI attempt sums/workspace/qualification/publication; (B) runtime/pack/Python smoke and bundle contract; (C) app service/grammar/timing/controls. Root owns integration, shared preparation resolver, real model qualification, builds and final evidence.
