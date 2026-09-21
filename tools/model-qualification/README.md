# Local model qualification evidence

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
