# Model packs

Deadpan installs model weights as verified packs in one global directory,
`~/Library/Application Support/Deadpan/Models`, shared by every project. Section
14 of the [specification](spec/DEADPAN_SPEC.md) is normative; this records the
implemented manager in [`deadpan_models::packs`](../crates/deadpan-models/src/packs.rs).

## Approved manifests

Manifests live in [`models/packs`](../models/packs/) and are compiled into the
application, so code signing covers which weights Deadpan accepts. Each lists
the pack identity and version, model family, runtime and compatible runtime
versions, supported operations and languages, every file's HTTPS URL, SHA-256
and exact size, the weight license with attribution, redistribution and access
terms, memory and temporary-space estimates, and its qualification report.
Validation requires HTTPS on an approved host (currently `huggingface.co`),
safe file names, lowercase SHA-256 and bounded sizes. Weights are never
committed.

| Pack | Files | License | Use |
| --- | --- | --- | --- |
| `whisper-base-en` 2 | `ggml-base.en.bin`, 147,964,211 bytes; `ggml-silero-v6.2.0.bin`, 885,098 bytes | MIT (OpenAI Whisper weights and Silero VAD by snakers4/silero-vad, ggml conversions by whisper.cpp) | English [transcription](TRANSCRIPTION.md) and [speech activity](SPEECH_ACTIVITY.md) |

Version 2 replaced version 1 (the recognizer alone). Its detector is pinned to
ggml-org/whisper-vad revision `9ffd54a1e1ee413ddf265af9913beaf518d1639b`, SHA-256
`2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`; Silero v6.2.0
loads and runs in whisper.cpp 1.8.3. A pack's operations name what it supports
(`transcribe`, `speech_activity`); the recognizer is its first file and the
detector the file named `ggml-silero-*`.

## Installation

1. Show the pack's size and license before downloading.
2. Check free space for the remaining bytes plus 256 MiB.
3. Take the pack version's exclusive lock (`.staging/<pack>-<version>.lock`,
   `flock`), so the app and the CLI never write the same staging files; a
   second installer gets `ModelPackBusy`.
4. Download each file into `.staging/<pack>-<version>/<file>.part`, resuming
   with an HTTP range request. A server that ignores the range restarts that
   file from zero; any other offset truncates the partial file so the next
   attempt restarts. The body is read on its own thread: cancellation is
   observed within 200 ms and a body that delivers nothing for 60 s is
   abandoned with its bytes kept for resume.
5. Before downloading a file, copy a same-named file of exact size from an
   installed version of the same pack (an APFS clone), then verify it like a
   download; a mismatch is deleted and the file is downloaded instead. Version
   2 therefore downloads only the detector over an installed version 1.
6. Verify exact size and SHA-256. A mismatched file is deleted, because it
   cannot be resumed into a valid one. A finished staged file is reused only
   after its hash matches.
7. Write a receipt and keep the pack staged until the host's smoke test passes
   (the real worker recognizes one second of silence and detects speech in it).
   A failed or cancelled smoke test keeps the verified staged copy, so a retry
   repeats only the test.
8. Activate by renaming the complete version directory to `<pack>/<version>`.
   Activation runs only when `installed` refused that version, so anything it
   replaces is already an incomplete copy.
   Other versions stay installed as known-good fallbacks until removed.

`installed` accepts a pack only when its receipt matches the approved manifest
and every file has its exact size; the consuming worker hashes the file again
before loading it. Removing a pack never touches project media.

The HTTPS transport uses `ureq` with rustls and the system trust store, refuses
plain HTTP including redirects, follows at most five redirects and identifies as
`Deadpan/<version>`. Hash verification covers every byte regardless of the
serving host.

## In the app

The Original rail's TRANSCRIPT section offers Install model… with the pack's
size and license when no transcription pack is installed, and Update model…
when only version 1 is installed and the Original lacks speech activity. Installation runs on
a background thread with progress and Cancel install; cancelling returns to the
offer, and a model install continues when the project changes because packs
belong to every project. Quitting cancels and waits up to three seconds for the
job thread.

## Commands

`models list`, `models install <pack>` and `models remove <pack>` take an
optional `--root` (default `~/Library/Application Support/Deadpan/Models` on
macOS, `$XDG_DATA_HOME/deadpan/models` on Linux). Failures use the codes
`ModelPackCancelled`, `ModelPackBusy`, `ModelPackSpace` and `ModelPackFailed`. Install reports JSON lines (`installing`, bounded
`progress`, `smoke_test`) and ends with the installed directory.
`transcribe` uses the installed transcription pack when no explicit model is
given, and its detector when no explicit `--vad-model` is given.

## Remaining

The offline full distribution with packs as data, gated
weights, signed update manifests, per-file operations in the manifest schema
and pack qualification beyond transcription and speech activity.
