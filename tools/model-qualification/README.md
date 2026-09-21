# Local model qualification

This directory contains a development adapter and measurement evidence. The app
does not install or invoke it yet. It requires an already assembled, pinned
private development environment and model data. It is not an end-user setup
procedure or a selected shipping runtime.

## Supervised bridge probe

`worker.py` accepts one protocol-2 `generate_bridge` request. The legacy
protocol-1 parser remains available for compatibility tests, but this
development worker emits only a `completed_bridge` event. Its strict manifest
declares the native media and provenance JSON; the sampled media remains a
developer output and is never declared as an accepted candidate. `worker_protocol.py`
checks the Rust wire contract without importing MLX. `mlx_backend.py` binds the
pinned LTX-2.3 q4 pipeline, keeps its CRF-33 conditioning preprocessing, polls
cancellation between denoising steps, and serializes native and sampled RGB
sequences. `worker_media.py` verifies the legal model grid, exact interior sample
positions, full decoded RGB, frame timestamps/durations, and explicit color tags.

The current adapter deliberately supports only two-endpoint bridges, Still
motion, 768×320, native 24 fps and `8*k+1` native frames in [9, 97]. These are
qualification bounds, not timeline UI limits or full provider capabilities.
It uses the existing developer GPL FFmpeg with lossless RGB intermediates.
Neither this codec choice nor the runtime is selected for distribution.

Build the Rust developer examples from the repository:

```sh
cargo build -p deadpan-jobs --examples --locked
cargo run -p deadpan-jobs --example plan_mlx_bridge --locked -- 24 24 1
```

Run `prepare_run.py --help` inside the already pinned private environment for
the required local paths. It prepares fresh, padded conditioning images, records
their original hashes and explicit color interpretation, obtains the plan from
Rust, and writes a host configuration. Its `--input-color-interpretation` field
is an explicit fixture assumption; preparation does not perform a color transform.
The `--run-directory` must not exist. No runtime or model is downloaded.

Then, from the repository:

```sh
cargo run -p deadpan-jobs --example qualify_model_worker --locked -- /absolute/run/host-config.json
python3 tools/model-qualification/verify_run.py /absolute/run
```

The Rust example supplies a cleared environment, launches the selected private
interpreter with `-I`, supervises the group, applies the lifecycle, and snapshots
native footage and provenance after clean exit. For protocol 2, the independent
verifier reads `host/native.snapshot.mp4` and the hash-verified provenance copy.
It checks native media only. Legacy protocol-1 runs still use
`host/candidate.snapshot.mp4`. The host stays in `Validating`; no project edit,
candidate acceptance, promotion, or job persistence happens here. Provenance and
the native sequence remain development outputs, not yet managed project assets.
Then use the Rust `deadpan-models` example `qualify_bridge_bundle` with the
original request and completed native declaration to derive and verify both
masters. [Bundle qualification](../../docs/GENERATION_BUNDLES.md) documents the
boundary, configuration fields are in the example, and measured runs retain
their exact configurations. Its output is still separate from project Ready
publication and authored acceptance.

Use a fresh run with `--cancel-after-millis` or `--cancel-at-stage inference`
to exercise actual cancellation.
The host enforces a 30-minute attempt deadline and a five-second cancellation
grace period. MLX's 64 GiB setting is a guideline; the worker checks 80 GiB process
RSS at cooperative boundaries. These are not OS memory or network isolation.
Blocking FFmpeg pipe I/O has no cooperative polling guarantee; the supervisor's
termination deadline is the bound there. All 139 LTX source files are verified
against a manifest bound to the upstream Git tree, and imported package/module
locations and hashes are checked before generation and again before completion.

Routine contract tests need only Python's standard library:

```sh
python3 -m unittest discover -s tools/model-qualification/tests -p 'test_*.py' -v
```

## Earlier standalone probe

[`evidence/2026-09-20-smoke`](evidence/2026-09-20-smoke/) captures one executed
LTX MLX keyframe probe. The Python files are the exact historical harnesses used
on the reference Mac, including their task-local paths. They are evidence, not
an installed worker, a portable benchmark CLI, or an end-user setup procedure.

The directory retains pinned file hashes and download receipts, the private
developer runtime inventory, original runtime and process reports, decoded-file
metadata, and normalized validation. `runtime.log.gz` contains the lossless
runtime log. `validation.json` hashes the captured files and explains why the
upstream end-stage Metal peak counter cannot stand for the entire generation.

Model weights, original footage, extracted input frames, and generated video
remain outside Git. The source-film URL, attribution, frame indices, and hashes
are recorded. Read [the qualification report](../../docs/qualification/model-smoke-2026-09-20.md)
for measured results and the substantial remaining gates.

This development probe uses a pinned `uv` environment and the developer's
FFmpeg executable. It does not establish the app-managed private runtime,
offline distribution, signing, license redistribution, or clean-Mac installation.
