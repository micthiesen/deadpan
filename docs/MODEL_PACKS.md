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
| `whisper-base-en` 1 | `ggml-base.en.bin`, 147,964,211 bytes | MIT (OpenAI Whisper weights, ggml conversion by whisper.cpp) | English [transcription](TRANSCRIPTION.md) |

## Installation

1. Show the pack's size and license before downloading.
2. Check free space for the remaining bytes plus 256 MiB.
3. Download each file into `.staging/<pack>-<version>/<file>.part`, resuming
   with an HTTP range request; a server that ignores the range restarts that
   file from zero, and any other offset fails.
4. Verify exact size and SHA-256. A mismatched file is deleted, because it
   cannot be resumed into a valid one.
5. Write a receipt and keep the pack staged until the host's smoke test passes
   (transcription packs recognize one second of silence in the real worker).
6. Activate by renaming the complete version directory to `<pack>/<version>`.
   Other versions stay installed as known-good fallbacks until removed.

`installed` accepts a pack only when its receipt matches the approved manifest
and every file has its exact size; the consuming worker hashes the file again
before loading it. Removing a pack never touches project media.

The HTTPS transport uses `ureq` with rustls and the system trust store, refuses
plain HTTP including redirects, follows at most five redirects and identifies as
`Deadpan/<version>`. Hash verification covers every byte regardless of the
serving host.

## Commands

`models list`, `models install <pack>` and `models remove <pack>` take an
optional `--root`. Install reports JSON lines (`installing`, bounded
`progress`, `smoke_test`) and ends with the installed directory.
`transcribe` uses the installed transcription pack when no explicit model is
given.

## Remaining

The in-app manager, resumable downloads across application restarts with
user-visible cancel, the offline full distribution with packs as data, gated
weights, signed update manifests and pack qualification beyond transcription.
