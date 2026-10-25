# Native worker launch admission, 2026-10-25

The packaged transcription, tracking and media workers now have independent
compiled image pins and expected-signer admission before launch. Their complete
six-library FFmpeg closure is included. The alignment path shares the bounded
signature, path and image checks. A separate private-AI-runtime gap remains;
**DP-22 and DP-22.3 stay Partial**.

[Permanent evidence](../../tools/model-qualification/evidence/2026-10-25-native-launch-trust/)
retains source/binary identities, invocations, outputs, failures, continuations
and a complete hash inventory. Primary machine: Apple M5 Max, macOS 26.5.2,
Rust 1.97.1. The builds use the recorded dirty source tree based at 8b418a82;
per-run hashes identify the executed inputs independently of that base label.

## Contract

The bundle builds workers first. Private copies receive the exact relocation
and clean-signing operations used by final assembly. Their signature-independent
hashes, including all six FFmpeg libraries, become the canonical
`DEADPAN_NATIVE_CONTENT_PINS` map in the final App and CLI. Inherited pin values
are removed. Host rebuilding must preserve the original worker bytes, and final
images must match the independently prepared pins. Both signed hosts actually
run `native-runtime-qualify` before publication, including with `--no-build`.

Admission checks the selected worker's exact role/path, all library pins,
private regular package paths, app resource seal, current/main host signatures
and image signatures. Explicit ad hoc signatures require validity and content
pins. A local certificate binds to the verified host's exact leaf certificate;
an Apple team binds to its expected team. Unknown or ambiguous signer forms
refuse. No Developer ID, notarization or broad entitlement is needed.

The actual `DeadpanPackaging` key must occur once in the top-level XML plist
dictionary with the expected string. Parsing is bounded to 256 KiB and depth
64. Comments, nested/duplicate markers, wrong types, malformed XML and internal
entity definitions refuse. A compiled pin from any runtime family makes a lost
marker or escaped host an error. Cheap discovery and retained downloader/AI
launch values cannot turn that error into development fallback.

Heavy admission runs at six preparation/CLI boundaries: transcription
supervision; tracking; exact-picture proposals; the common explicit/resumable
proxy builder; YouTube assembly; and AI output qualification. Early selectors
on UI/project-writer threads stay cheap. Each admission has a shared deadline
of at most 60 seconds, honors cancellation between stages and read/hash chunks,
and polls system commands cooperatively. Checked group teardown and a sole
reap path apply on every post-spawn result; cleanup failures remain errors.

Proxy runtime damage is an environmental failure, preserving the Original's
resumable ranges. AI helper admission failures retain host I/O/interruption/
deadline classifications instead of blaming candidate output.

## Verification

| Check | Actual result |
| --- | --- |
| Original changed-package check | Strict lint passed; 962 tests ran: 961 passed, one privacy allowlist failure, 23 skipped |
| Lifecycle/host continuation | Strict lint and 203 selected tests passed after interruption, cleanup, error classification and fallback changes |
| Final XML/signing continuation | Strict lint and 147 selected tests passed, including structural-marker, signer/process, native policy, privacy, helper and xtask cases |
| Local certificate build | Full hardened bundle passed both hosts' native/alignment probes, 77-image audit, notices/SBOM and actual Python/MLX/Metal runtime check; signature reports `local` |
| Ad hoc build | `--no-build` assembled and probed the same independently pinned images under ad hoc signing; no signing identity needed |
| Relocated local-certificate workflow | Scrubbed environment, isolated home and path containing spaces; actual Whisper Metal transcription returned five words; Vision tracked three pictures and completed the requested range; face/saliency proposals ran |
| Re-signed substitutions | All three workers and all six libraries refused in both hosts; library tests mutate only LC_UUID, preserving ABI |
| Package identity | Missing/comment-only markers and a host outside `.app` refused; default and explicit managed downloader roots could not bypass the damaged-host check; doctor reported unavailable AI with the package error |
| Signer binding | Validly sealed app with an ad hoc worker refused the local host's certificate requirement in both hosts; restoring the original bytes passed |
| Final negative-case lane | 169 recorded operations, 34 expected refusals, zero timed-out operations; complete authored document bytes remained identical |
| General bundle verification | Relocated startup, doctor, downloader probe, local import, actual render, AI runtime/Metal check, damaged helpers/resources and notice negatives passed |

The fixture is the existing synthesized-speech Original from
[word refinement](word-refinement-2026-10-25.md). These runs establish launch
and workflow behavior, not general speech/tracking accuracy. No private AI
generation is rerun here, and no fresh full-product gate is claimed.

## Preserved failures

The original broad run failed because the new system-tool launch file was not
yet in the privacy allowlist. Its review and privacy record now cover the fixed
`/usr/bin/codesign` path, arguments and cleared environment. The original run
remains failed. An XML-parser lint error is retained before its corrected final
continuation.

The first certificate-name bundle attempt was stopped after signing did not
complete. Exact-fingerprint signing succeeded. Four earlier negative-case
driver attempts remain: missing fixture shot analysis; signing identity lookup
under the isolated home; explicit-keychain lookup under that home; and a signing
timeout. The final driver refreshes the existing identity through the canonical
dotfiles signer and uses the owner's home only for fixture signing. Every tested
host retains the scrubbed environment and isolated home. Timeouts cannot count
as expected refusals. No product admission check was relaxed for these setup
failures.

## Remaining private runtime work

The private AI interpreter, worker scripts, imported code/bytecode and native
extensions still lack a host-compiled tree inventory check before Python runs.
Build-time pinned downloads and worker self-reported hashes happen too early
or too late to establish that launch authority. The new cheap package guard
prevents fallback after marker loss; it does not bind those executable bytes.

The actual runtime tree has 10,722 files, 1,536 directories and six confined
leaf aliases, totaling 493,623,996 bytes. Its 62 Mach-O images, Python/LTX code,
precompiled bytecode, `.pth`, Metal library and runtime metadata need a closed
inventory bound into the hosts, with expected signer/resource-seal admission at
the shared Check/Inference launch boundary. Model weights keep their separate
pack verification. This remains the next DP-22.3 implementation work.

Admission does not claim containment against a concurrent same-user writer who
can replace the host itself. Clean/second-Mac and physical owner checks retain
their existing specification 29.1 status.
