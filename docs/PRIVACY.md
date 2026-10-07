# Privacy audit

Specification [Section 27.4](spec/DEADPAN_SPEC.md#274-privacy-and-observability)
keeps sources, transcripts, generated footage and face tracks local, limits
network access to user-requested imports, model/helper/app updates and explicit
links, requires no analytics, and forbids cookies or authentication headers in
logs or project metadata. This record is the DP-23 privacy check as of
2026-10-06. It covers the shipped `deadpan-app`, `deadpan-cli`, media,
transcription and tracking workers, and the AI runtime launch. The developer-only
`crates/xtask` and the `tools/` qualification harnesses download pinned build
inputs and are not shipped.

## Network paths

There is one HTTP client: `HttpsTransport` in
[`deadpan-models/src/packs.rs`](../crates/deadpan-models/src/packs.rs), using
`ureq` 3.4.2 with rustls and the system trust store. It is HTTPS only, follows
at most five redirects and hash-verifies every byte against a pinned or signed
manifest. Each of its constructions runs only after an explicit user action:

| Trigger | Code | Remote |
| --- | --- | --- |
| `models install <pack>`; Models panel install (license acceptance first) | [`cli/models.rs`](../crates/deadpan-cli/src/models.rs), [`app/model_packs.rs`](../crates/deadpan-app/src/model_packs.rs) | `huggingface.co` only (manifest host allowlist) |
| `downloader install`; New from URL after its install confirmation | [`youtube/helpers.rs`](../crates/deadpan-cli/src/youtube/helpers.rs), [`app/youtube.rs`](../crates/deadpan-app/src/youtube.rs) | pinned `github.com` release assets for yt-dlp and Deno |
| `downloader update` / `models update` with a signed manifest; Models panel **Apply signed update…** | [`update_signing.rs`](../crates/deadpan-cli/src/update_signing.rs), [`app/model_packs.rs`](../crates/deadpan-app/src/model_packs.rs) | the manifest's own URLs; an `https://` manifest argument is fetched only on the CLI, the app reads a chosen file |
| `project create-from-url`; native New from URL | [`youtube/acquire.rs`](../crates/deadpan-cli/src/youtube/acquire.rs) through [`youtube/runner.rs`](../crates/deadpan-cli/src/youtube/runner.rs) | YouTube, contacted by the pinned yt-dlp and Deno helpers, not by Deadpan |

The helpers run with a cleared environment (no inherited proxy, credentials or
user configuration), `DENO_NO_UPDATE_CHECK=1`, `--ignore-config`,
`--no-plugin-dirs`, `--no-remote-components`, `--no-cookies-from-browser` and a
private HOME ([YouTube import](YOUTUBE_IMPORT.md)). Nothing checks for updates on
its own: updates and installs start only from the commands and buttons above
([updates](UPDATES.md), [model packs](MODEL_PACKS.md)).

Everything else is local:

- The linked FFmpeg is configured `--disable-network`; the build scripts of
  `deadpan-source`, `deadpan-encode` and `deadpan-media-worker` refuse a prefix
  configured otherwise. Decoders read descriptors, never URLs.
- No shipped code opens an IP socket, resolves names or uses Apple networking
  APIs. The only sockets are Unix-domain: the authenticated
  [live-project endpoint](LIVE_PROJECT.md) and worker control socket pairs.
- The AI worker runs with `HF_HUB_OFFLINE`, `TRANSFORMERS_OFFLINE`,
  `HF_HUB_DISABLE_TELEMETRY` and `DO_NOT_TRACK` set; the worker refuses to
  generate without them and sets them itself for the model-pack `--check`.
  Both inference and model-pack checks also launch through the shared
  [`generation/runtime/launch.rs`](../crates/deadpan-cli/src/generation/runtime/launch.rs)
  boundary: `/usr/bin/sandbox-exec -p '(version 1)(allow default)(deny network*)'`
  executes the selected Python with its existing isolated arguments. This
  fixed macOS profile denies networking to Python and its descendants while
  retaining ordinary file access, standard I/O and Metal. An unavailable
  launcher or unsupported platform fails; there is no unrestricted fallback.
  `WorkerProcess` and `run_helper` retain their existing owned spawn,
  process-group supervision and checked teardown. The same boundary applies
  to bundled, development and explicit developer runtimes.
- Bookmark resolution passes `WithoutUI | WithoutMounting`, so relinking never
  mounts a network volume.
- Explicit links: the Models panel shows license source URLs with
  `hyperlink_to`. eframe's `links` feature dispatches a link only when the user
  activates it. `webbrowser` belongs exclusively to `egui-winit`; the CLI and
  local workers cannot reach it, and shipped application code has no direct
  browser-opener calls.

## Automated checks

- [`privacy_network_audit.rs`](../crates/deadpan-cli/tests/privacy_network_audit.rs)
  reads `Cargo.lock`, `cargo metadata` and every Rust source under `crates/`
  and `native/` (except `xtask`) and fails when:
  - a package other than `deadpan-models` depends on the HTTPS stack (`ureq`,
    rustls, webpki, `http`, `httparse`);
  - a network client, async runtime, unreviewed browser opener, crash reporter
    or telemetry/analytics crate enters the lockfile;
  - a crate other than `egui-winit` depends on `webbrowser`, or the CLI or a
    local worker reaches it;
  - the media, store, render, audio, playback, analysis, jobs, transcription,
    tracking or media-worker crates reach the HTTPS stack at runtime;
  - `ureq`, `HttpsTransport`, IP or Apple networking APIs, Unix sockets,
    subprocess launch sites or the yt-dlp runner appear outside their
    file allowlist (or the allowlist names files that no longer use them);
  - shipped code names `curl`, `wget`, `nscurl`, `open`, `osascript`, `nc`,
    `ssh`, `scp`, `sftp` or `git` as a program;
  - the FFmpeg `--disable-network` requirement, either AI launch boundary,
    the fixed network-denial profile, the AI worker's offline variables or
    the helpers' cleared environment and cookie arguments go
    missing;
  - a struct holding a `secret` or `cookie` field derives `Debug`.
- [`privacy_sandbox.rs`](../crates/deadpan-cli/tests/privacy_sandbox.rs) runs
  a private copy of `deadpan-cli` under `sandbox-exec` with `(deny network*)`, a cleared
  environment and an empty HOME. It first proves the profile denies a loopback
  TCP connection. Create-from-file of a committed fixture, an edit with dry
  run, Undo, Redo, validate, plan inspection, shot detection, transcript and
  pause reads, `doctor --project`, diagnostic export, backup, storage report, portable copy and
  its validation, Render (about 25 s in a debug build) and `verify-export` all
  succeed. `downloader install`, `models install`, both signed-update fetches
  and `create-from-url` fail within 30 s with `DownloaderNetworkFailed`, `ModelPackFailed`,
  `UpdateFetchFailed` and `DownloaderNotInstalled`, installing nothing. The
  render and diagnostic reports contain no package, source path, file name or label. Nothing
  is written to HOME. The test skips with a message only when
  `/usr/bin/sandbox-exec` is missing.
- The existing [`offline_portable.rs`](../crates/deadpan-cli/tests/offline_portable.rs)
  renders accepted AI pauses and a portable copy with outbound IP denied.
- [`generation/runtime/network_tests.rs`](../crates/deadpan-cli/src/generation/runtime/network_tests.rs)
  exercises the production model-check runner and, through
  `ai_network_inference_host_preserves_files_protocol_and_owned_group`, the
  inference supervisor. Reachable TCP, UDP and Unix-domain listeners provide
  positive controls. Python and a descendant that changes its session must
  receive permission errors on each transport, read and write local files,
  and exchange valid stdout or framed protocol responses. The tests check
  that Python retains the owned leader PID and process group and that both
  processes are reaped. Launcher unit tests cover unavailable isolation,
  invalid Python paths and unsupported platforms. All these checks passed
  in the follow-up 68-test run. A fresh packaged app then imported the real
  model pack, passed its model check, generated a candidate on Metal in
  78.166 seconds, explicitly accepted it and rendered a verified movie.
  Generation alone left the authored fallback unchanged. The run used an
  isolated HOME and the production launch boundary; see the
  [release audit](RELEASE_AUDIT.md) for identities and retained evidence.

## Secrets, cookies and reports

| Property | Evidence |
| --- | --- |
| Live endpoint secret never in errors or responses | `host/tests.rs` `strict_frames_and_authentication_never_reach_dispatch`, `adversarial_live_endpoint_frames`; discovery is mode 0600; secret-bearing types have no `Debug` (audit above); portable copies are built fresh without `.host.json`; backups contain only the database |
| Cookies come only from an explicit file, are copied owner-only and deleted | `youtube/acquire/tests.rs` `cookies_are_copied_owner_only_and_removed`, `arguments_disable_configuration_plugins_and_browser_sessions`, `environment_is_private_and_complete` |
| No cookie value or cookie path in the project, provenance or events | [`media-worker/tests/youtube_import.rs`](../native/deadpan-media-worker/tests/youtube_import.rs) scans every package file (database, WAL, media), the provenance and emitted events for a distinctive cookie value and the cookie file path |
| Helper error lines drop signed URL queries and credential header values | `youtube/acquire/tests.rs` `downloader_messages_redact_url_queries_and_credentials` (added with the fix below) |
| Provenance URL is canonical (tracking and playlist parameters removed) and stays out of history | `youtube_import.rs` and `original_provenance.rs` |
| Render reports omit source paths, labels and URLs | [publication](RENDER_PUBLICATION.md); `privacy_sandbox.rs` checks the published report |

Fixed in this audit: an unrecognized yt-dlp failure kept the helper's final
error line in the user-visible message. yt-dlp's transfer errors can include
`googlevideo.com` stream URLs whose query carries the client's IP address and
signed access parameters. `classify_failure` keeps each URL's scheme, host
and path, replaces user information with `[redacted]`, replaces its query or
fragment with `?[redacted]`, and replaces any `Cookie`, `Set-Cookie`,
`Authorization` or `Proxy-Authorization` value with `[redacted]`. It redacts
before truncating the diagnostic and also redacts URLs preceding a credential
header on the same line.

## Telemetry and logging

The lockfile has no analytics, telemetry or crash-reporting crate (enforced
above). No logger is installed: the `log` and `tracing` facades pulled in by
eframe and winit have no subscriber, and Deadpan writes no log files. The
store's failpoint log exists only in debug builds and only when
`DEADPAN_STORE_FAILPOINT_LOG` is set. The
`:diagnostics` panel shows process-local performance counters
([`deadpan-diagnostics`](../crates/deadpan-diagnostics/src/lib.rs)) and sends
nothing.

## Diagnostic export

`diagnostics export <new-report.json> [--project <project.deadpan>]` and
**Save diagnostic report…** in the native `:diagnostics` panel explicitly save
a local JSON report. The native action captures the current committed
document, then builds and writes the report on a worker thread. Cancellation
of the save dialog creates nothing. These entrypoints share
[`diagnostic_export.rs`](../crates/deadpan-cli/src/diagnostic_export.rs).

Schema 1 contains app/OS/architecture/SQLite/schema versions, compiled helper
and model-pack baseline versions (not a claim about installed updates), current
process counters, and optional project structural counts, raster, rate,
duration and stable plan/store error codes. A caller may add an operation and
failure from closed enums. An unreadable package still yields a report with
its stable error code. There is no raw error or log text, authored identity,
label, path, URL, transcript, credential, environment, media or attachment.
The report is constructed from allowed fields; it does not scrub a project
dump after collecting private content.

Serialization has a 256 KiB bound. Publication writes a complete owner-only
temporary file, syncs it, and installs it without overwriting an existing
file, symlink or directory. A directory-sync failure reports that the file
was saved but durability is unconfirmed. Unit tests cover private authored
text, ignored private package files, unreadable packages, the byte bound,
permissions and destination collisions. The native replay adds keyboard
save/cancel/collision checks; the network-denied CLI scenario includes export.
On 2026-10-06 the focused CLI/worker/privacy run passed all 68 selected tests
across 38 binaries in 36.462 seconds (523 tests were outside that selection).
This included all five diagnostic-export unit tests, all six static privacy
checks, and the network-denied workflow with diagnostic export and emitted-file
verification. The offline workflow took 30.714 seconds. Native replay evidence
is recorded with the current requirement qualification.

## Remaining gaps

- No physical network capture (for example a packet trace of a full GUI session)
  has been performed. The whole-process sandbox scenario covers the headless
  CLI; packaged `deadpan-app --headless` additionally exercises the production
  AI worker restriction with the installed model. The native app, live endpoint
  (which needs Unix-domain sockets) and transcription have not been exercised
  under the whole-process profile.
- yt-dlp has not been run under the sandbox; its offline failure mapping
  (`YouTubeNetworkFailed`) is covered by unit tests of its messages only.
- Third-party Python packages remain network-capable in isolation; production
  inference and model checks now apply the network-denial profile above.
  It permits filesystem access and does not contain a descendant's process
  group escape. Those are separate boundaries documented in
  [the adversarial suite](ADVERSARIAL.md#hostile-workers). Actual packaged
  inference, installed-model checks and Metal execution passed under the
  production restriction.
- The static audit matches source text. It cannot see network use inside
  vendored C/C++ code beyond the FFmpeg configuration check, or code reached
  through a new dependency that does not match the listed crate names.
- Portable project copies deliberately keep Original provenance, including the
  canonical source URL and title; diagnostic reports exclude both.
- Opening a license link in the owner's configured browser requires the native
  activation check; the dependency audit checks the dispatch boundary only.
