# Signed updates

Specification §15.2 requires downloader updates through "app-verified signed
manifests, compatibility checks, and rollback", installed in "a controlled
versioned Application Support location" while the bundled baseline stays; §14.3
asks for signed model-pack manifests, and §14.3/§27.1 require a known-good pack
to stay until its replacement passes a smoke test, with separate helper and
model version identities and rollback rules. This page records the trust root,
the manifest formats, the on-disk layout and the release procedure.

## Trust root

Deadpan is a personal app without a Developer ID (§27.1), so code signing cannot
anchor update provenance. Updates are signed with a project-owned Ed25519 key:

- The public key is compiled into every build from
  [`models/update-keys.json`](../models/update-keys.json) (schema 1, a list of
  `{id, ed25519}` entries). The current key is `deadpan-2026-10`,
  `c88c3ea03061671854a45dca08646e88f2e680bb1a8bc1055e3521b0c0329254`.
- The private key never enters the repository. It was generated on the owner's
  M5 Max with `deadpan-cli update-signing keygen --out
  ~/.local/state/deadpan/update-signing/deadpan-2026-10.pk8` (PKCS#8 v2,
  mode 0600 in a 0700 directory, outside the dotfiles tree). Release signing
  names it with `--key <file>` or `DEADPAN_UPDATE_SIGNING_KEY`. Back it up
  privately; losing it means rotating the key in a new app build.
- Rotation: add the new public key, ship a build, then remove the old one. A
  manifest signed by a key a build no longer lists is treated as incompatible
  (the baseline is used and the reason reported), not as tampering.

Verification uses `ring` 0.17.14 (Apache-2.0 AND ISC), already linked by the
HTTPS transport, so no new cryptography crate was added. The SHA-256 pinning
alternative (hashes of each future manifest compiled into the app) was rejected:
it cannot deliver an update without rebuilding the app, which would make the
§15.2 updater pointless.

## Envelope

A signed manifest is JSON (at most 256 KiB):

```json
{ "schema": 1, "kind": "downloader" | "model-pack", "key_id": "deadpan-2026-10",
  "payload": "<the manifest JSON text>", "signature": "<128 hex digits>" }
```

The signature covers `deadpan-signed-update-v1\0`, the kind, `\0` and the exact
payload bytes, so a downloader manifest can never verify as a pack manifest.
The signature is checked before the payload is parsed; unknown fields are
refused everywhere. Code: [`deadpan_models::updates`](../crates/deadpan-models/src/updates.rs).

## Downloader updates

Payload ([`DownloaderManifest`](../crates/deadpan-cli/src/youtube/updates.rs)):
`schema` 1, a positive `serial`, `issued` (YYYY-MM-DD), `min_app_version`,
`platform` (`macos-aarch64`), the embedded `ejs_version`, and exactly two
releases, yt-dlp then Deno, each with name, dotted numeric version, license,
an `https://github.com/` URL, download and executable SHA-256 and sizes (at
most 512 MiB), packaging (`executable` or a single `zip_entry`), executable
name, and for yt-dlp the Mach-O `content_sha256` pin, for Deno its `signer`
code requirement.

Layout under the managed helper root (`~/Library/Application
Support/Deadpan/helpers`, never inside the application bundle):

```text
<root>/<name>/<version>/<executable>     one directory per version, never overwritten
<root>/updates/manifests/<serial>.json   the exact signed envelope
<root>/updates/state.json                {active, previous, highest_serial, below_baseline_accepted}
<root>/updates/.lock                     one updater or rollback at a time
```

`downloader update --manifest <file|https-url> [--allow-downgrade] [--root]`:

1. Verify the signature against the compiled keys, then the structure, then
   compatibility (minimum app version, platform). Refusals:
   `UpdateUntrusted`, `UpdateSignatureInvalid`, `UpdateManifestInvalid`,
   `UpdateIncompatible`.
2. Refuse helpers older than this build's baseline (`UpdateDowngrade`) unless
   `--allow-downgrade`; refuse a serial lower than the highest installed here
   (a replay, even of an identical retained envelope; `rollback` is the way
   back) unless explicit; refuse a different envelope reusing an installed
   serial.
3. Install each release with the same verified, never-overwriting installer as
   `downloader install`, plus the yt-dlp content pin. An already verified
   version directory is reused.
4. Run the real probe (`--verbose` diagnostics and
   `deno --version`) on the new set. It must report the manifest's yt-dlp,
   yt-dlp-ejs and Deno versions, or `UpdateProbeFailed` leaves the previous
   selection active and retains nothing, so a corrected manifest may reuse
   the serial.
5. Retain the envelope (temporary file and no-replace rename; a torn or
   unverifiable file left by an interrupted write is replaced), then replace
   `state.json` atomically (write, fsync, rename, fsync directory), recording
   the previous selection. Nothing is deleted.

The replay floor is the higher of `state.json`'s `highest_serial` and the
highest serial among retained envelopes that verify, so deleting or editing
`state.json` cannot reselect an older signed envelope: an active serial below
that floor is used only when the owner chose it explicitly
(`--allow-downgrade` or `rollback`, recorded as `older_serial_accepted`), and
is otherwise passed over as a replay with a reported reason.

`downloader rollback [--baseline]` verifies the target completely (signature,
files, code signatures; for the baseline, the bundled or managed pins) and
swaps active and previous. `--baseline` also recovers from an unreadable
`state.json`: it reports the problem (`recovered` in its JSON), verifies the
baseline and writes a fresh state whose replay floor comes from the retained
envelopes. The `updates` and `updates/manifests` directories must be owned by
this user and not group- or world-writable, and links are refused.

Threat model: these checks keep other local users, damaged files and replayed
envelopes out. A process running as the same user can still change
`state.json` and the managed helpers; signatures, the replay floor and
per-launch hashes make that visible or refused, but cannot prevent it.

Selection on every use: a packaged app, the CLI and doctor read `state.json`
under the managed root. An active update is used only when this build trusts
its key, satisfies its minimum app version and platform, and none of its
helpers is older than this build's baseline, unless the owner explicitly
accepted that against the same baseline. Otherwise the bundled baseline (or, in
development, the managed root's compiled pins) is used and `downloader status`
and `doctor` report why; an import shows the same reason (`downloader_note`
in headless events and output, a warning on the app's confirmation step) and
records it in the Original's provenance. A changed executable, an edited or missing envelope
under a trusted key, or a failed code signature is an integrity failure
(`DownloaderHelperInvalid`) that refuses imports instead of switching copies;
the message names `downloader rollback`. Every launch re-verifies exact size,
SHA-256, private permissions and `codesign --verify --strict` (against the
release's signer requirement when it has one).

An app update that ships a newer baseline therefore supersedes older helper
updates automatically, and a helper update never modifies the signed bundle.

## Model-pack updates

Payload ([`PackUpdate`](../crates/deadpan-models/src/packs/updates.rs)):
`schema` 1, `serial`, `issued`, `min_app_version` and one complete schema-2
pack manifest. It must name a pack family this build compiles, keep its
`runtime_id`, list a runtime version this build ships and add no operation.
Bridge (AI pause) packs refuse: the AI worker verifies its own compiled weight
receipt, so they change only with an app update.

`models update <file|https-url> [--from <folder|archive.tar>] [--accept-license]
[--allow-downgrade]` verifies the envelope, prints its size and every license
(with `licenses_to_accept`: layers that require acceptance, plus any that are
new to this build or whose terms differ from the compiled pack's, compared by
hash) and refuses with `ModelPackLicense` until they are accepted, installs the version beside the
others through the ordinary stage, smoke test and activation path (download, or
`--from` an offline folder or archive), and only then retains the envelope as
`<models>/.updates/<pack>/<version>.json` and records the version in
`<models>/.active/<pack>.json` with the version previously in effect. `models rollback
<pack>` selects the previous version again after checking that it is installed
and still admissible. Older versions than the selected one and serials lower than the
highest activated one need `--allow-downgrade`; a version already known with different files
is refused. `models remove` refuses the active version until it is rolled back
and names `--version` for removing another installed version. Retained
envelopes are written by temporary file and rename, a torn one is replaced,
pointer changes hold an exclusive lock on `.active/.lock` across processes, and
`.updates` and `.active` must be private to this user.

Consumers (transcription, pause detection, `doctor`, `models list`) use the
selected version: the pointer's when it is in the verified catalog and
installed, otherwise the compiled approved version. `installed` checks the
receipt and each file's exact size, not its hash (the consuming worker hashes
before loading), so a missing or resized file, a missing envelope or a key this
build no longer trusts falls back to the compiled version. `models list`,
`doctor` and the Models panel then show a note saying why.

## In the app

The Models panel (`:models`) lists each pack's selected version and the
version kept for rollback, and adds UPDATES: the downloader's versions and
origin (bundled, pinned baseline or signed update N), any reason an installed
update is not used, **Apply signed update…**, **Activate previous downloader:
…** when a previous selection exists and **Use the baseline downloader** when
an update is active or the state is unreadable (it rewrites the state after
reporting the problem). A chosen file is verified at once (signature, kind,
target) and held for review: what changes, the pack's size and licenses, with
an acceptance checkbox for each license that needs one, then **Apply update**
or **Cancel update**. The verified bytes travel with the job; the file is not
read again. A pack with a kept version offers **Activate version N** and
**Remove version N**, and shows a note when its recorded version is not in
use. Updates and rollbacks share the panel's single background job; a busy
refusal names the running job's kind (Model update, Downloader update,
Rollback…). The `model-packs` UI replay drives a refused untrusted file, the
review, an applied signed version 3 of the transcription pack (scripted
installer, real verification against a replay key) and activating version 2
again.

## Release procedure

1. Describe each helper: `deadpan-cli update-signing describe <file>` prints
   size, SHA-256 and the Mach-O content pin. Cross-check the upstream
   checksums (yt-dlp `SHA2-256SUMS`, Deno `.sha256sum`).
2. Write the payload JSON with the next serial.
3. `deadpan-cli update-signing sign --kind downloader|model-pack --key <key>
   payload.json signed.json`. Signing refuses a payload the app cannot parse
   and a key the build does not trust, and re-verifies the result.
4. Apply it on this Mac with `downloader update --manifest signed.json` (or
   `models update signed.json`) and keep the record below current.

## Qualification record (2026-10-06, M5 Max)

Real signed manifests under the `deadpan-2026-10` key, a fresh scratch root,
debug CLI:

- Serial 1 (yt-dlp 2026.08.19, Deno 2.9.7): both downloaded from GitHub,
  verified, content-pinned and probed (`stable@2026.08.19`, `yt_dlp_ejs-0.8.0`,
  `deno 2.9.7`, `matches_pins: true`) and activated in 6.6 s.
- Serial 2 (yt-dlp 2026.07.04, older than the baseline): refused with
  `UpdateDowngrade`; with `--allow-downgrade` installed beside 2026.08.19,
  probed `stable@2026.07.04` and activated in 3.9 s, previous = update 1.
- `rollback` restored update 1 (probe `stable@2026.08.19`); `rollback
  --baseline` restored the compiled pins. (A later change makes re-applying
  an older retained serial a replay that needs `--allow-downgrade`; unit
  tests cover it.)

Packaged app (ad hoc `cargo xtask bundle --without-ai-runtime` from this
working tree, its own `Contents/MacOS/deadpan-cli` with only a fresh `HOME`,
`PATH=/usr/bin:/bin` and `TMPDIR`): status and probe used the bundled baseline;
serial 2 was refused without `--allow-downgrade`, then installed under the
fresh home's `Library/Application Support/Deadpan/helpers`, probed and
activated in 3.3 s. Status, probe (`stable@2026.07.04`) and `doctor` then
reported the update ahead of the bundled baseline, `codesign --verify --deep
--strict` still passed on the bundle, and `rollback` returned to the bundled
baseline (probe `stable@2026.08.19`).

To verify (owner): applying a signed downloader update to a packaged
`Deadpan.app` on a second Mac under quarantine, and a published update URL
(no hosting is configured; `--manifest` accepts an `https://` URL).
