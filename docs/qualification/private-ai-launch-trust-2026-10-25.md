# Private AI prelaunch admission, 2026-10-25

The packaged Python runtime now has a closed inventory bound into both hosts,
with content, path, ownership, signer and app-seal admission before Check or
Inference executes. Together with [native-worker admission](native-launch-trust-2026-10-25.md),
this closes the audited DP-22.3 gap. **DP-22 returns to Complete** under the
primary-Mac verification scope of §29.1. No delivery gate closes.

[Permanent evidence](../../tools/model-qualification/evidence/2026-10-25-ai-launch-trust/)
retains every invocation, failure and continuation, source/binary identities,
compiled inventory, notices/SBOM, expected refusals and emitted-file receipts.
Primary Mac: Apple M5 Max, macOS 26.5.2, Rust 1.97.1. Runs span October 25–26
UTC, October 25 in America/Vancouver. Source is the recorded dirty tree based
at 8ad64f85; per-file hashes identify its implemented inputs.

## Contract

The builder freezes the entire runtime, notices and in-memory report into a
private snapshot before rebuilding the hosts. It removes old signatures and
signs all 62 Mach-O images once. The canonical inventory is stored separately
at Contents/Resources/ai-runtime-inventory.json; its exact SHA-256 is compiled
into App and CLI as DEADPAN_AI_RUNTIME_INVENTORY_SHA256. Every build clears
inherited pin values. Final assembly copies the frozen signed bytes without
re-signing. Source snapshots, final bytes, notices and inventory are rechecked
across host rebuilding and after direct Python/MLX/Metal smoke execution.

Both final hosts actually execute ai-runtime-qualify before and after that
smoke check. Stale or absent compiled pins refuse publication even with
--no-build. --without-ai-runtime instead requires absent runtime/inventory and
an explicit null pin report. Resource provenance cannot supply authority.

The shared launch gate hashes the pinned manifest before parsing it, compares
the entire descriptor-relative tree, checks five fixed executable/import paths,
and verifies every Mach-O image plus the app/current/main host against the
expected signer. Ordinary files include Python source, bytecode, .pth, worker/LTX
sources, runtime metadata and Metal data. Missing or added nodes also refuse.
Only confined leaf-file aliases are allowed. Duplicate/unknown/noncanonical
JSON, escaping/cyclic/dangling/directory aliases, foreign/shared-write entries,
special files and unsupported Mach-O layouts refuse.

Bounds are 32,768 nodes, depth 64, 1,024-byte relative paths, an 8 MiB manifest,
512 MiB per file and 2 GiB total. Reads/hashes/scans/signature commands observe
cancellation and one admission deadline of at most 60 seconds. Check's 300 s
and Inference's 30-minute budgets include admission rather than restarting
after it. Failures remain host I/O/interruption/deadline errors. Heavy work
stays off the UI/project writer. Cheap runtime identity discovery separately
uses a 64 KiB, nonblocking, non-following regular-file read.

The developer opt-in permits inventory bypass only when all five selected
paths resolve outside the app, after verifying the host seal/signer. Bundled,
mixed or aliased paths retain the full fixed-path inventory requirement.
Compiled pins from any runtime family prevent damaged-package downgrade.
Installed model weights retain their separate pack verification.

## Measured inventory and builds

The inventory has 10,660 ordinary files, 62 Mach-O files, 1,536 directories and
six leaf aliases. Its 2,314,724 canonical bytes hash to
08ed482a9f5009c1941d4d7aa2651a0fc643529fa087c50f25aeedfc0e04e9c0
in both local-certificate and ad hoc bundles. Their regular bytes total
493,629,228 and 493,623,996 respectively; signature-independent image identity
permits that signing difference while preserving content authority.

| Check | Actual result |
| --- | --- |
| Broad changed-package check | Strict lint passed; 1,010 tests ran: 1,009 passed, one stale privacy-source assertion failed, 23 skipped; one nextest pipe-leak warning |
| Corrected continuation | Strict lint and 192 selected tests passed; two pipe-leak warnings retained |
| Serial warning follow-up | All thirteen relevant inventory/admission/protocol tests passed without a pipe-leak warning |
| Independent review | General, security and lifecycle reviews; stale audit assertion fixed; bounded-discovery follow-up re-reviewed |
| Local certificate bundle | Both hosts' probes before/after Python/Metal, 77-image audit, notices/SBOM passed; 792.1 MiB |
| Ad hoc --no-build bundle | Same compiled canonical inventory, all host/runtime probes and audit passed |
| Disabled-AI bundle | Fifteen-image audit and both explicit absent-runtime reports passed; 291.3 MiB |
| Stale --no-build attempt | Actual publication refused the absent AI pin before direct Python smoke execution |
| General relocated verification | Local and disabled-AI bundles passed startup, doctor, helpers, import, actual render and applicable runtime/negative checks |
| Real model Check | Both relocated host entrypoints rehashed the installed pack and ran the real Check mode successfully |
| Runtime tampering | 150 operations, 61 expected refusals, zero timeouts; restored hosts pass |

The negative lane covers scripts, bytecode, import paths, Metal data, altered
inventory, added import hooks, missing files, modes, alias escapes/cycles,
directory aliases, FIFO nodes, FIFO identity metadata and marker removal.
Python, libmlx and FFmpeg UUID changes preserve ABI and are validly re-signed
and sealed before both hosts refuse them. Structural unsealed shapes are
reported separately, without claiming a valid re-sign. A tampered-worker real
Inference attempt fails as host I/O before execution and preserves the exact
authored document.

## Real generation, acceptance and recovery

The established [October 8 definition-edge fixture](extension-native-2026-10-08.md)
uses K9/E8, twelve authored frames at 24 fps, Still motion and seed 42107. It
reaches Ready; that alone leaves the document unchanged. Explicit acceptance
then changes the provider. The emitted 55,047-byte MP4 contains 21 pictures and
42,000 canonical audio samples, and passes full emitted-file verification.
Export runs with an empty HOME and IP networking denied, without installed
models. Undo restores the exact fallback document except its revision identity;
full history validation passes.

Two earlier invented interior-Hold cases completed all thirty inference steps
and correctly failed production quality validation. Still entry motion measured
1.253080/s against 0.500000/s; Moderate's unconditioned exit had luma shift
37.185 against 32.000. Both failures remain, with no acceptance. Choosing the
established edge fixture neither changes thresholds nor substitutes a failed
candidate. This is launch/workflow verification, not a general quality corpus.

## Preserved verification failures and limits

The original formatter/shared-module resolution and unsupported mkfifo test
compilation failures remain. The broad test's stale source assertion is not
relabeled a full-tree pass. Parallel nextest warnings remain alongside the
clean serial follow-up; no unrelated harness repair is claimed.

The first fixture request used the wrong command discriminator. The first
canonical export lacked its destination directory; its continuation exported
and undid acceptance successfully, then used an unsupported --full flag for
history validation. The correctly formed project validate invocation passes.
All failed commands and frozen driver versions remain; these corrections
neither regenerate nor replace the accepted candidate.

The 31-file 36 GB model pack was privately cloned and fully hashed from its
already accepted installation. No download or license bypass occurred. Model
weights, executables, private keys/keychains and large mutation backups do not
enter the evidence collection. Generation times are observations under
uncontrolled cache/load, not latency qualification. Real-person perception,
other/clean Macs and physical owner checks keep their existing §29.1 scope.

Admission does not claim containment against a concurrent same-user writer who
can replace the host itself. Full product performance, composability, effects,
analysis scheduling and delivery-gate work remain open.
