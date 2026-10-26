# AI runtime trust verification scratch

`prepare.py` has run. It APFS-cloned the existing approved extension pack into
`home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-extension/1` and
SHA-256 verified all 31 receipt entries (36,152,862,913 bytes) in 12.143 s.
Every clone has a distinct inode. The owner's model files and receipt were read
only. Existing LTX/Gemma license acceptance is retained. `setup.json` is the
per-file receipt. The 39,157-byte CFR fixture is a clone of the repository's
approved `native/deadpan-source/tests/fixtures/cfr-bframes.mp4`.

No AI commands or inference have run in this scratch task. Fixture authoring was
subsequently exercised against the already-qualified old bundle: create-original,
raw JSON dump, InsertTime dry-run and commit, then quick validation all pass in
`schema-validation-v2`. The dry-run leaves the authored dump identical. The first
attempt used the wrong command discriminator; `schema-validation-v1` retains its
`InvalidInput: missing field command` error, and `verify.v1.py` retains the original
driver. Current `verify.py` uses nested `command: insert_time`.
Python syntax compilation passed for both scripts. The IP-only outer sandbox profile parses
and `/usr/bin/true` exits 0 under it; this is not a network-denial positive control.

## After Root builds final hosts

Use the exact local identity already retained in the native-trust run. The driver
prepares that identity through the canonical dotfiles signer before codesign,
uses its keychain explicitly, and records no private key material.

```
python /tmp/deadpan-ai-runtime-trust-2026-10-25/verify.py \
  --bundle /path/to/final/Deadpan.app \
  --output '/tmp/deadpan-ai-runtime-trust-2026-10-25/verified with spaces' \
  --identity 7D8CA64BD99CD51A4C1A9BFC0CB8D8BCA76A057E \
  --workflow --negative
```

Every tested host gets only isolated HOME, system PATH and TMPDIR. The app is
APFS-cloned to a path containing spaces. Positive checks call both final CLI and
App `--headless` with `ai-runtime-qualify`, then `models check
ltx-2.3-q4-extension`. The latter fully revalidates the already cloned weights and
executes real WorkerMode::Check; it downloads nothing and creates no pack copies.

The workflow creates a scratch one-Original project, inserts a deterministic
30-frame silent Background Hold at Edit frame 60, and calls `generate-hold
--hold ai-trust-hold --seed 9 --mode extend-left`. It requires Ready while the
authored document stays identical, explicitly accepts the reported request and
attempt, renders the accepted output under IP denial, then uses durable Undo
and compares the restored fallback document (ignoring only new revision id).
The request envelope uses 16 fresh split IDs; unused IDs are allowed by the
typed insertion command. If this exact tiny fixture's extension context is
refused by product policy, retain that refusal and use a longer approved fixture;
do not relax validation or substitute a stand-in model.

The historical reference-Mac bridge run reached Ready in 76.883 s; this is an
estimate, not an extension measurement. Allow 20 minutes for the workflow and
600 s per model Check. The weights require about 36.2 GB logical space but the
APFS clone shares their initial physical blocks. Runtime admission hashes the
493 MB private tree and verifies 62 Mach-O images, so each Check/negative may
take longer than past 12-second smoke tests.

The AI worker owns its production deny-network sandbox. Do not wrap AI Check or
Inference in a second `sandbox-exec`: macOS rejects nested application of the
profile with exit 71. Only non-AI render/readback uses the outer IP-only sandbox.
This proves neither physical disconnection nor a second Mac.

## Negative coverage

The driver mutates and restores one copied-bundle entry at a time. Script,
bytecode, `.pth`, Metal resource, inventory, missing worker, changed mode,
added `sitecustomize.py`, valid re-signed Python, valid re-signed libmlx and valid
re-signed FFmpeg cases reseal the app and require external deep strict codesign
success before both hosts refuse admission and real Check. This distinguishes
host runtime trust from external resource-seal failure.

Existing alias escape and cycle, added directory alias and FIFO cases call both
host qualifiers without invoking Python, then restore the shape. Their exact
host diagnostics are retained. They are not claimed to have valid external
signatures. The marker-removal case reseals successfully, then requires both
qualifier and real Check refusal. A final positive qualifier confirms restoration.

Root owns these Cargo/build gates; this subtask did not run Cargo:

- Build without AI runtime: both hosts' native/alignment policies still pass,
  `ai-runtime-qualify` and ordinary AI use refuse without developer fallback.
- `bundle --no-build` with exact matching AI inventory and host pins succeeds.
- `--no-build` with stale, missing or changed inventory/host pins refuses; retain
  the actual packaging diagnostic, rather than treating arbitrary build failure
  as the intended negative.

Source review confirms closed-project `generate-hold` writes one JSON stdout
report (progress uses stderr), `accept-hold` accepts the reported request and
attempt flags, and durable Undo requires `project undo PATH --expected REVISION`.
Dump output is the document itself, with the tested Hold recipe at
`nodes["ai-trust-hold"].kind.recipe`. All specified pyc/pth/Metal paths exist in the
already-qualified bundle. The added-hook case credits host rejection only after
resealing and external deep strict verification succeed; any sealing failure is
retained as a harness failure, not counted as a host trust result.

The driver still needs review against the final AI qualifier output contract before execution.
No outputs from the previous native-trust verification count as these new checks.

## Frozen driver strengthening

`verify.v2.py` preserves the structurally validated driver before stricter
negative assertions. Current `verify.py` requires each negative's final structured
host error to name inventory/content-pin/runtime-trust refusal; marker removal
requires its packaging-marker error. Real Check negatives require the prelaunch
admission or compiled-inventory error (the deliberate missing-worker case accepts
the precise bundled-worker-missing/reinstall package error). A Python/Metal/model
failure with exit 1 cannot count. Positive qualifiers require a 64-hex compiled
inventory pin. Runtime/script/host/source fingerprints are retained in results.

During the re-sealed worker-script mutation, one additional `generate-hold` runs
against the workflow's restored Hold after Undo. It must exit 1 with host `Io`
admission failure, state Failed, no Ready result or Ready variants, and an
authored dump exactly identical before and after. `--negative` therefore requires
`--workflow`. Each real Check is capped at 600 seconds; refused inference is
capped at 240 seconds. No strengthened cases have executed yet.
