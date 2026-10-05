# Model packs

Deadpan installs model weights as verified packs in one global directory,
`~/Library/Application Support/Deadpan/Models`, shared by every project. Section
14 of the [specification](spec/DEADPAN_SPEC.md) is normative; this records the
implemented manager in [`deadpan_models::packs`](../crates/deadpan-models/src/packs.rs).

## Approved manifests

Manifests live in [`models/packs`](../models/packs/) and are compiled into the
application, so code signing covers which weights Deadpan accepts. Schema 2
lists the pack identity and version, model family, runtime and compatible
runtime versions, supported operations and languages, every file's HTTPS URL,
SHA-256 and exact size, one or more license layers, memory and temporary-space
estimates, and its qualification report. A file `name` is a relative path of at
most four safe components (`mlx_ltx_q4_pack/<revision>/vocoder.safetensors`);
absolute paths, `..`, hidden components and duplicates are refused. Validation
also requires HTTPS on an approved host (currently `huggingface.co`), lowercase
SHA-256, bounded sizes and at most 64 files. Weights are never committed.

Each license layer (specification §14.4) has an `id`, `title`, SPDX identifier
or `LicenseRef-…`, attribution, HTTPS link, a plain `terms` summary shown before
installation, redistribution and access statements, `acceptance_required`, and
optionally `text`, the name of a full license text compiled into the build from
[`models/licenses`](../models/licenses/). A pack with several layers assigns
each file to one. The summary never replaces the full text; `models license`
and the Models panel show both.

| Pack | Files | Licenses | Use |
| --- | --- | --- | --- |
| `whisper-base-en` 2 | `ggml-base.en.bin`, 147,964,211 bytes; `ggml-silero-v6.2.0.bin`, 885,098 bytes | MIT (OpenAI Whisper weights and Silero VAD by snakers4/silero-vad, ggml conversions by whisper.cpp); no acceptance step | English [transcription](TRANSCRIPTION.md) and [speech activity](SPEECH_ACTIVITY.md) |
| `ltx-2.3-q4-bridge` 1 | 31 files, 36,152,862,913 bytes: `dgrauet/ltx-2.3-mlx-q4` at `56a5866d638ecfe37c54d348e88938235185c2d4` (17 files, 28.08 GB) and `mlx-community/gemma-3-12b-it-4bit` at `86cc6a8dedbc456dd0e4af01a9d09f396f77e558` (14 files, 8.07 GB) | LTX-2 Community License Agreement (weights) and Gemma Terms of Use (text encoder), both requiring acceptance | [AI pauses](AI_HOLDS.md) (`bridge_hold`) |

Version 2 of the whisper pack replaced version 1 (the recognizer alone). Its detector is pinned to
ggml-org/whisper-vad revision `9ffd54a1e1ee413ddf265af9913beaf518d1639b`, SHA-256
`2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`; Silero v6.2.0
loads and runs in whisper.cpp 1.8.3. A pack's operations name what it supports
(`transcribe`, `speech_activity`, `bridge_hold`); the recognizer is its first
file and the detector the file named `ggml-silero-*`.

The bridge pack's files, hashes, sizes and URLs are exactly the
[qualified receipt](../tools/model-qualification/evidence/2026-09-20-smoke/download-manifest.json)
the worker verifies on every attempt (a unit test compares them). Its install
directory is the worker's model cache: `<pack>/<version>/mlx_ltx_q4_pack/<rev>/…`
and `…/mlx_gemma_default_text_encoder/<rev>/…`. Both repositories are public and
ungated on Hugging Face (checked 2026-10-05 through the API); downloads need no
account. The terms that matter to a user:

- **LTX-2 Community License** (license date January 5, 2026; full text
  [`ltx-2-community-license.txt`](../models/licenses/ltx-2-community-license.txt),
  SHA-256 `e28ef68d…6a134`, identical to the pack's own `LICENSE`): free for
  individuals and entities with annual revenue under US$10,000,000; larger
  entities need a paid commercial license from Lightricks. Use restrictions in
  Attachment A apply to the model and its outputs. Redistribution must include
  the agreement and its use restrictions.
- **Gemma Terms of Use** (last modified April 1, 2026; text extracted from
  https://ai.google.dev/gemma/terms on 2026-10-05 into
  [`gemma-terms-of-use.txt`](../models/licenses/gemma-terms-of-use.txt), SHA-256
  `6d4742f8…1a3dc`): the Prohibited Use Policy applies; redistribution must
  pass on the use restrictions with the notice "Gemma is provided under and
  subject to the Gemma Terms of Use found at ai.google.dev/gemma/terms".

Redistributing either set of weights, for example in a full offline
distribution, carries those notice and use-restriction obligations. The
converters' licenses (mlx-forge, mlx-vlm) do not replace the weight licenses.

## Installation

1. Show the pack's size, free space and every license (summary, attribution,
   link and full text) before staging anything.
2. Refuse unless every license with `acceptance_required` was explicitly
   accepted (`PackError::LicenseNotAccepted`, CLI code `ModelPackLicense`).
   The check runs before the install lock or any file is created, and the
   receipt records the accepted license identifiers and the origin
   (`download` or `import`).
3. Check free space for the remaining bytes plus 256 MiB.
4. Take the pack version's exclusive lock (`.staging/<pack>-<version>.lock`,
   `flock`), so the app and the CLI never write the same staging files; a
   second installer gets `ModelPackBusy`.
5. Download each file into `.staging/<pack>-<version>/<name>.part`, resuming
   with an HTTP range request. A server that ignores the range restarts that
   file from zero; any other offset truncates the partial file so the next
   attempt restarts. The body is read on its own thread: cancellation is
   observed within 200 ms and a body that delivers nothing for 60 s is
   abandoned with its bytes kept for resume.
6. Before downloading a file, copy a same-named file of exact size from an
   installed version of the same pack (an APFS clone), then verify it like a
   download; a mismatch is deleted and the file is downloaded instead. Version
   2 therefore downloads only the detector over an installed version 1.
7. Verify exact size and SHA-256. A mismatched file is deleted, because it
   cannot be resumed into a valid one. A finished staged file is reused only
   after its hash matches.
8. Write a receipt and keep the pack staged until the host's smoke test passes.
   The whisper test recognizes one second of silence and detects speech in it.
   The bridge test runs the bundled (or development) runtime's
   `worker.py --check` against the staged directory: it verifies the 139 pinned
   LTX source files, imports the pipeline and text encoder modules from them,
   runs a Metal calculation, checks every receipt file's size and parses every
   safetensors header (no inference). A failed or cancelled smoke test keeps
   the verified staged copy, so a retry repeats only the test.
9. Activate by renaming the complete version directory to `<pack>/<version>`.
   Activation runs only when `installed` refused that version, so anything it
   replaces is already an incomplete copy.
   Other versions stay installed as known-good fallbacks until removed.

`installed` accepts a pack only when its receipt matches the approved manifest
and every file has its exact size; the consuming worker hashes the file again
before loading it. `state` reports Installed, Partial (bytes an install would
resume from) or Absent. Removing a pack, or discarding a partial download,
never touches project media.

The HTTPS transport uses `ureq` with rustls and the system trust store, refuses
plain HTTP including redirects, follows at most five redirects and sends the
user agent `OpenAI File Downloader, XaiImageApiFetch/1.0`. Hugging Face
redirects large files to its CDN (`us.aws.cdn.hf.co`), which honors range
requests; hash verification covers every byte regardless of the serving host.

### Offline installation

`PackStore::import` stages the same verified directory from a source the user
chose instead of the network, with the same consent, space, lock, smoke-test
and activation steps:

- **A folder** holding each file at its manifest path, either directly
  (`<folder>/<name>`) or below `<folder>/<pack>/<version>/`. Only regular files
  of exact size count; symbolic links are not followed. Files are copied into
  staging with `std::fs::copy`, which clones on APFS, so importing from the
  same volume needs no extra space (the space check counts only files on
  another device). Every file is then hashed.
- **An uncompressed tar archive** (ustar, pax or GNU long names, base-256
  sizes) with the same layout. The reader compares member names with manifest
  paths and writes only to staging paths derived from the manifest, never
  from archive names, so traversal entries are harmless. A matching member
  that is not a regular file (a link or directory), a duplicate, a wrong size
  or a bad header checksum is refused; unrelated members are skipped.

A source missing any file fails with `ImportIncomplete` (CLI
`ModelPackVerification`) naming the count and an example; a hash mismatch fails
verification. Neither activates anything. `PackStore::export` writes an
installed pack as `<pack>/<version>/<name>` pax entries, published by rename,
which `import` and the system `tar` both read; that is the form a full offline
distribution would carry (specification §14.1).

The developer qualification cache `~/Library/Caches/Deadpan/ltx-qualification`
already has the bridge pack's folder layout, so `models import
ltx-2.3-q4-bridge ~/Library/Caches/Deadpan/ltx-qualification --accept-license`
installs it by clone without downloading 36 GB.

## In the app

The **Models** panel (`:models`, or Models… in the Deadpan menu) lists every
approved pack with its title, state (Installed, Partly downloaded with the
bytes left, or Not installed), size, memory estimate and use, and shows the
models folder with its free space. Each license shows the bytes it covers, its
terms summary, attribution, access statement and link; Read the full … opens
the compiled text in a scrolling box. A license that requires acceptance has
an "I accept the …" checkbox, and Install (Resume for a partial download)
stays disabled with a stated reason until every such license is accepted,
while another pack job runs, or when free space is below the remaining bytes
plus 256 MiB (the refusal names both amounts). Install from folder… and
Install from archive… (`.tar`) take a native picker and run the same verified
import. A running job shows its phase, bytes and Cancel install; afterwards
the panel offers Remove or Discard partial download and states the outcome.
Nothing downloads without one of these explicit actions.

One job runs at a time on its own thread (`model_packs::Manager`) through the
same `install_pack` the CLI uses. It continues when the panel closes or the
project changes, Cancel keeps partial bytes for Resume, and quitting cancels
and waits up to three seconds. Acceptance is remembered per pack for the app
session and checked again by the installer.

The panel is a modal: Tab moves between its controls, Space or Enter
activates the focused one and Escape closes it without side effects. The
inspector's AI PICTURES section, when the bridge pack is missing, states the
download size, the two licenses and the memory estimate and offers Install AI
models… (`:models`), which opens the panel focused on the first license; during
an install it shows progress and Show in Models…. The Original rail's
TRANSCRIPT section keeps its Install model… and Update model… offers, with the
149 MB size and MIT license, and they open the panel on the whisper pack. Both
sections re-check when a job finishes.

The `model-packs` UI replay drives these paths with real keyboard input
through a harness-only scripted backend that reports progress, honors Cancel
and materializes a sparse installed pack; removal and discard use the real
store ([UI feedback](UI_FEEDBACK.md)).

## Commands

| Command | Effect |
| --- | --- |
| `models list` | JSON: root, free bytes and every approved pack with size, files, operations, licenses (with bytes each covers), memory estimate, installed directory, staged and remaining bytes |
| `models license <pack>` | Every license's title, summary, attribution, access, link and full compiled text |
| `models install <pack> [--accept-license]` | Download, verify, smoke-test, activate |
| `models import <pack> <folder-or-archive.tar> [--accept-license]` | The same from an offline source |
| `models export <pack> <archive.tar>` | Write an installed pack as an offline archive |
| `models remove <pack> [--partial]` | Remove the installed version, or with `--partial` discard staged bytes |

All take an optional `--root` (default `~/Library/Application Support/Deadpan/Models` on
macOS, `$XDG_DATA_HOME/deadpan/models` on Linux). `--accept-license` accepts
every license of the pack after the user has read them (`models license`);
without it, a pack whose licenses require acceptance refuses with
`ModelPackLicense` after announcing its size and licenses. Other failure codes:
`ModelPackCancelled`, `ModelPackBusy`, `ModelPackSpace`, `ModelPackVerification`
(missing, mismatched or hostile offline files) and `ModelPackFailed`. Install
and import report JSON lines (`installing` with size and licenses, bounded
`progress`, `smoke_test`, `smoke_test_passed` with the runtime report for the
bridge pack, `activating`) and end with the installed directory.
`transcribe` uses the installed transcription pack when no explicit model is
given, and its detector when no explicit `--vad-model` is given. AI pauses use
the installed bridge pack (see [AI pauses](AI_HOLDS.md#runtime)).

## Remaining

Signed update manifests and a rollback policy for packs, a built full offline
distribution that carries exported packs as data, gated-weight sign-in flows
(no approved pack is gated today), per-hardware pack qualification beyond the
reference M5 Max, a "Fast" label (no pack meets the §13.4 target), and the
clean-machine interrupted-install test of §26.6 on a second Mac.
