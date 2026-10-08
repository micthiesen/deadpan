# Supervised one-sided generation, 2026-10-07

The development worker now performs real forward and reverse video extension
through the supervised protocol. Both directions passed retained-input capture,
inference, clean process teardown, native conversion and an independent
full-pixel sampling oracle. **DP-12 remains Partial:** this does not yet provide
extension generation or acceptance in the application.

## Implemented contract

- Protocol 3 has distinct `GenerateExtension`/`CompletedExtension` messages and
  lifecycle checkpoints. A bridge completion cannot satisfy an extension, or
  vice versa. The store refuses V3 admission until its remaining integration
  is implemented; worker completion alone never authorizes Ready or acceptance.
- Context schema 1 retains chronological pictures at exact P/R spacing in one
  immutable definition clock. Each has a picture identity, input object and
  fitted rectangle. An opposite seam is explicitly absent or present and
  unconditioned. A selected region has one captured anchor.
- Capture uses private SHA-256-verified snapshots and BLAKE3 receipts, a shared
  deadline/cancellation flag, a 1 MiB manifest bound, at most 64 context entries
  and a 16 MiB aggregate frame-input bound. Aliases cannot contradict each other
  or refer to the manifest. The runtime admits exactly nine context pictures.
- Pinned `RetakePipeline.extend` receives chronological RGB tensors and E/8
  generated latent frames. Its source latent slice must remain exactly equal;
  nonfinite or unexpected tensors fail. Generated audio latents are computed
  and discarded. Source speech is not supplied to the model.
- The complete native movie is retained separately from the exact N-frame
  generated-only sequence. The Python sampler, native C converter and independent
  oracle agree on clamped frame-center sampling and half-up RGB quantization.
  Context handles never extend the usable output interval.
- A distinct development pack identity binds the operation and runtime. The
  approved bridge pack, installer capabilities and accepted bridge wire formats
  remain unchanged. Worker provenance has its own schema 3 with exact context,
  generated, complete-movie and authored durations.

## Actual model and media runs

Hardware: Apple M5 Max, 128 GiB, macOS 26.5.2. Runtime source is
`3392d75934120b7e69eefbe55893f7ef82be92a4`, with the retained approved LTX-2.3 q4
weights and Gemma text encoder. All model files and tensor schemas were checked.
The explicit development identity is `ltx-2.3-q4-extension-development`/`1`,
runtime `ltx-mlx`/`0.15.8+deadpan-extension-dev1`, operation `extension_hold`.

Each run used 768×320, K=9 real temporal input pictures, E=8 generated pictures,
17 total native pictures at 24 fps, seed 42107 and 30 development steps. The
authored output is nine frames at 30000/1001 fps, exactly 3003/10000 seconds.
The generated interval is 1/3 second; nominal speed conversion is 10000/9009.
The duration and count differ deliberately so interpolation and edge clamping
are exercised. The development envelope currently refuses authored durations
above 1/3 second, other raster/native-rate combinations and other K/E counts.

The fixture uses the previously measured synthetic moving square. Input image
bytes were retained before launch and matched after completion. Definition
clocks and qualification identifiers are explicit fixture assertions; this run
does not establish production source capture or same-shot qualification.
The private Python runtime ran under `sandbox-exec` with `deny network*`.

| Measurement | Extend from left | Extend from right |
| --- | ---: | ---: |
| Supervisor elapsed, seconds | 148.666 | 180.003 |
| Backend elapsed, seconds | 136.159 | 167.423 |
| Process peak RSS, bytes | 14,527,283,200 | 14,495,744,000 |
| MLX peak counter, bytes | 17,022,794,766 | 17,022,794,766 |
| Generated native ordinal interval | [9,17) | [0,8) |
| Preserved source latent start | 0 | 1 |
| Native / sampled pictures | 17 / 9 | 17 / 9 |
| Exact channel comparisons | 25,804,800 | 25,804,800 |
| Context-frame sampling fetches | 0 | 0 |

These are functional observations from one seed and synthetic fixture, not a
quality corpus or release performance claim. The Rust media worker independently
decoded native and sampled files and checked count, timing, color and hashes.
The separate Python oracle decoded both the worker and host outputs, compared
all samples against independent integer arithmetic, and checked the complete
native movie against its canonical copy. No audio stream was retained.

A separate real run cancelled on entering Inference. The worker acknowledged
within 1.129 seconds of that stage; clean process-group teardown completed at
14.682 seconds overall with no escalation, no faults, no candidate or acceptance.
This tests cancellation at inference entry, not interruption of every GPU kernel.

## Verification and retained failures

- All 843 affected jobs/models/store tests passed. After the final bounded-enum
  adjustment, all eight conditioning tests passed again; after adding fuzz
  coverage, all nine conditioning checks and the framed-protocol campaign test
  passed. The supervisor example's three tests also passed.
- All 95 Python tests passed without loading MLX. These include shared Rust/Python
  JSON fixtures, strict versions, cross-operation refusal, exact sampling,
  selected-region binding and exclusion of an unconditioned opposite image.
- Strict workspace and UI-harness Clippy passed. Final focused strict lint,
  formatting, affected doc tests and `git diff --check` passed.
- The regular, uninstrumented mutation campaign ran 265,506 cases with no
  failures across generation messages, pack manifests/archives and Bridge and
  Extension conditioning. The campaign now allocates time for all four actual
  model targets instead of treating that group as a single target.

Initial compilation caught unsupported `LowerHex` formatting in three new test
digest expressions. They now format bytes explicitly. Strict lint caught an
oversized enum variant and an equivalent integer comparison; the optional
opposite picture is boxed and the comparison simplified. These failures and
successful follow-ups are retained.

The first fixture capture rejected a human-readable qualification ID before
launch; it was replaced with the retained fixture movie's SHA-256. The first
worker preflight then rejected a missing baseline download manifest in the
frozen adapter copy. Copying that existing required resource fixed setup.
The failed worker exited cleanly, retained its failure report and produced no
candidate. The final preparation script includes both corrections.

## Evidence and remaining integration

[Retained evidence](../../tools/model-qualification/evidence/2026-10-07-extension-worker/manifest.json)
includes per-file hashes, source hashes against base `9d2a9447`, executed binary
hashes, input PNGs/receipts, requests, worker provenance, media reports, oracle
results, events, failures and reproduction scripts. Complete movies remain in
`/tmp/deadpan-resume-20261006/extension-worker`, with their hashes retained in
each direction's `retained-media.json`.

The next required product work is immutable project-picture capture with
same-shot checks, full temporal-input relevance and automatic replacements,
direction-aware quality admission, durable accepted sampling, native controls
and a measured longer-duration envelope. No GUI, release bundle, accepted-project
reopening, production context capture or extension export was exercised here.
The overall spec estimate stays about 88%.
