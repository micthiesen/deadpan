# YouTube import

DP-14 creates a new [single-Original](SINGLE_ORIGINAL.md) project from one
YouTube video. This page covers URL rules, the pinned downloader helpers,
acquisition, assembly, provenance and failures, headless and in the native app.
Signed helper bundling remains open.

```sh
deadpan-cli downloader install                 # pinned yt-dlp + Deno, verified
deadpan-cli downloader status --probe          # hashes and versions actually loaded
deadpan-cli project create-from-url /abs/clip.deadpan 'https://youtu.be/VIDEO_ID'
deadpan-cli project create-from-url /abs/clip.deadpan URL --cookies /abs/cookies.txt
deadpan-cli project original-provenance /abs/clip.deadpan BLAKE3_DIGEST
```

You are responsible for having the rights to use an imported video. Remixing
or parody does not resolve copyright by itself. Deadpan does not bypass DRM:
formats YouTube marks as DRM-protected are never selected.

## In the app

The empty start surface follows the product board's "Choose one Original"
panel: Choose video (`⌘N`), a YouTube URL field with Start project, and Open
project (`⌘O`). Over an open project the same step opens as a sheet (`⌘⇧N`,
`:youtube`, File > New from YouTube URL…); that project stays open until the
new one is ready.

1. Type or paste a URL. Text, paste, selection and IME are native. The line
   below the field shows the normalized video ID or the exact refusal (for
   example "This is a playlist; choose a specific video from it."), and Start
   project is disabled until the URL is supported. Enter on a refused URL
   starts nothing.
2. Enter (or Start project) inspects the video. If the helpers are missing, an
   explanation offers Install downloader (75.6 MB) with the pinned versions,
   the 118.1 MB installed size and SHA-256 verification. Nothing is installed
   until that button or Enter is chosen; Esc declines. A finished install
   continues the same import.
3. The details show title, uploader, length, upload date, license, the selected
   picture (`1920 × 1080 · 24 fps · H.264 (avc1.640028)`) and sound
   (`AAC (mp4a.40.2) · 44.1 kHz · 130 kb/s`), the download size and the
   destination, `Documents/Deadpan/<title>.deadpan` (or `<title> 2.deadpan`),
   with the rights notice. Nothing has been transferred and no package exists.
   Details wait at most 10 minutes for confirmation: the private directory
   holds the cookie copy and YouTube's stream URLs expire, so an unconfirmed
   inspection is dropped with `YouTubeConfirmationExpired` and Enter checks the
   video again.
4. Download and create (Enter) shows the stages: video details, downloading
   (percent and bytes), assembling, checking and retaining the Original, and
   opening. Esc cancels any stage until the Ready package is published; the job
   tears down its helper group and private directory and no package is left.
   Publication is one atomic rename, so an Esc that arrives after it cannot
   remove the project: the step says "Esc can no longer cancel; opening…", and
   notes when the project was already complete as Esc was pressed.
5. If another project took the confirmed name meanwhile, even at the final
   rename, the finished project moves to the next free name (`<title> 2`); the
   download is never discarded for a name collision.
6. The Ready package opens through the ordinary Open path, like a local new
   project, and the status line reads `Created “<title>” from YouTube as <name>`.
   If Open is refused or the project service stays busy for 60 s,
   `ProjectOpenFailed` names the complete package and offers Open project…;
   the URL is cleared so Enter cannot download it again. Closing the window
   cancels running work and never opens a package that finishes meanwhile; a
   complete one stays in the library.

An optional cookies file is chosen explicitly with Choose cookies file… (a
native picker); it is never read from a browser. Age-restricted and sign-in
refusals point at that row. A picker result that arrives while an import runs
is not applied, and Enter waits while the picker is open. Failures keep the
library's stable code with a plain title, its message and next-step guidance in
app terms; a finished failure or cancellation is cleared when the project
session changes.

Only plain Enter and Escape act on the step. Command chords beside the field
keep their ordinary meaning (`⌘N`, `⌘O`, `⌘I`, `⌘⇧N`), including as native menu
key equivalents, while running work keeps the whole keyboard. The footer shows
only the start or step keys on the empty start surface and while the step owns
the keyboard. Starting from YouTube is refused, with the same reasons as a local
New, while a macro records, an import prepares, a render workflow is unfinished
or an AI pause generation runs.

[`youtube.rs`](../crates/deadpan-app/src/youtube.rs) owns one job thread at a
time, outside the UI thread and the project service. The library is split in
two phases for it: [`acquire::inspect`](../crates/deadpan-cli/src/youtube/acquire.rs)
verifies helpers, fetches and validates metadata and keeps the locked private
workspace, inspected JSON and cookie copy in an `Inspected` value;
`acquire::download_and_create` consumes it after confirmation. Declining drops
it, removing all of them. `create_from_url` composes both for the CLI. The
job's first destination comes from the sanitized title as one bounded file-name
component in the library (separators become single spaces; an empty result
falls back to `YouTube <ID>`), and the library's no-replace rename still
refuses a name taken in the meantime. Window close cancels the job and waits
for its teardown; app exit waits up to 10 s. The [preview layer](../crates/deadpan-app/src/preview/youtube.rs)
routes only plain Enter and Escape to the step; a focused button keeps its own
activation and composition owns its keys.

No thumbnail is fetched or shown: the app has no JPEG/WebP decoder, and the
title, length and streams identify the video before transfer.

## URLs

[`youtube/url.rs`](../crates/deadpan-cli/src/youtube/url.rs) is pure. It
accepts only `https://` URLs on `youtube.com`, `www.`, `m.` and
`music.youtube.com`, `youtu.be` and `youtube-nocookie.com`, without user
information or a port. Supported forms:

| Form | Example |
| --- | --- |
| Watch | `https://www.youtube.com/watch?v=ID`, also on `m.` and `music.` |
| Share | `https://youtu.be/ID?si=...` |
| Shorts | `https://www.youtube.com/shorts/ID` |
| Embed | `https://www.youtube.com/embed/ID`, `https://www.youtube-nocookie.com/embed/ID` |
| Live recording / legacy | `https://www.youtube.com/live/ID`, `/v/ID` |
| Playlist with an indicated video | `watch?v=ID&list=...`, `playlist?list=...&v=ID` |

YouTube Music watch URLs use the same video ID space, so they are accepted;
the downloaded streams are the ordinary YouTube renditions of that ID. The ID
must be exactly 11 characters from `A-Z a-z 0-9 - _`. A playlist without
`v=` fails with `YouTubePlaylistNeedsVideo` ("choose a specific video"); other
hosts, schemes, IDs and paths fail with `YouTubeUrlInvalid`. Only the canonical
`https://www.youtube.com/watch?v=ID` reaches the downloader and provenance;
the user's tracking parameters, timestamps and playlist context never do.

## Helper bundle

Specification §15.2 requires yt-dlp, its EJS JavaScript components and a
supported runtime, initially Deno. [`youtube/helpers.rs`](../crates/deadpan-cli/src/youtube/helpers.rs)
compiles exact pins into the build; see
[dependency decisions](DEPENDENCIES.md#downloader-helpers) for files, sizes,
hashes and licenses:

- yt-dlp 2026.08.19, the official `yt-dlp_macos` standalone executable. Its
  release notes and `--help` state that official executables already include
  the matching `yt-dlp-ejs` package; `--verbose` reports `yt_dlp_ejs-0.8.0`.
- Deno 2.9.7 for Apple Silicon, the release ZIP's single `deno` executable.

`downloader install` downloads each file over HTTPS (bounded redirects, user
agent `OpenAI File Downloader, XaiImageApiFetch/1.0`) into a private
`.staging` directory, rejects any size or SHA-256 mismatch (for Deno, of both
the archive and the extracted executable), sets `0755` and publishes one
complete `<root>/<name>/<version>/` directory with a no-replace rename
(`RENAME_EXCL`), so even a racing empty directory is never replaced. A
published version is never overwritten; a damaged one is reported, not
replaced. A concurrent installer that published first wins. Body reads run on
their own thread, so cancellation and a 60 s stall are noticed even while a
socket read blocks. The default root is
`~/Library/Application Support/Deadpan/helpers`; `--root` (or `--helpers` for
`create-from-url`) names another absolute directory.

Both executables' exact size and SHA-256 are verified when an import resolves
them and again immediately before every yt-dlp launch (yt-dlp starts Deno
within that run). Symbolic links are refused, and so is an executable, or any
directory above it, that another user owns or that is group- or
world-writable (a sticky directory such as `/tmp` is allowed). A change
between that check and exec therefore requires this user's own access. `downloader status` reports presence and verification;
`--probe` runs both helpers and reports the yt-dlp, yt-dlp-ejs, JavaScript
runtime and Deno versions they actually load, with `matches_pins`. `doctor`
lists the pinned versions and their presence without hashing.

A packaged `Deadpan.app` (`cargo xtask bundle`) ships the same pinned files as
a read-only baseline in `Contents/Resources/helpers`, and the running bundle
prefers it over the managed root. Deno keeps its upstream signature and pinned
bytes and its signer requirement. yt-dlp is re-signed with the hardened
runtime and must match a compiled signature-independent content hash. Bundled
files are verified before every launch. Inside a packaged app the baseline is
always used, so a missing or damaged one reports `DownloaderHelperInvalid`
instead of falling back
([packaging](PACKAGING.md#downloader-baseline)). An explicit `--root` or
`--helpers` names only a managed root. The managed install serves development
builds and is the future update location. Updates through app-verified signed
manifests with compatibility checks and rollback, and notarization, remain open
(DP-22). Updating a pin is a source change with new hashes and a re-run of the
tests, the bundle check and a real import.

## Acquisition

[`youtube/acquire.rs`](../crates/deadpan-cli/src/youtube/acquire.rs) runs
yt-dlp twice, each time through `deadpan_native_process::spawn` as a process
group leader with a cleared environment:

- `HOME`, `TMPDIR`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME` and
  `DENO_DIR` point into one private temporary directory, `PATH` is
  `/usr/bin:/bin`, and nothing else is inherited (including proxy variables).
- Arguments always include `--ignore-config`, `--no-plugin-dirs`,
  `--no-remote-components`, `--no-js-runtimes --js-runtimes deno:<pinned path>`,
  `--no-cache-dir`, `--no-cookies-from-browser`, `--no-playlist` and either
  `--no-cookies` or `--cookies <private copy>`. The URL follows `--`.
- Output is bounded (32 MiB metadata, 64 KiB diagnostics), each run has a
  deadline, and SIGINT/SIGTERM cancel it. On exit, cancellation or timeout the
  whole group is torn down before the leader is reaped, and the output pipes get
  five seconds to close: a descendant that escaped the group and keeps them open
  fails the run instead of hanging it.
- Every signal only requests cancellation; there is no immediate exit, so each
  stage tears down its helper group and removes the private directory,
  including any cookie copy, before the command returns. The directory holds an
  exclusive lock for its owner's lifetime. Each import first sweeps this user's
  `deadpan-youtube-*` directories whose lock is free (or that have no lock and
  are over an hour old), so the files of a process killed outright do not
  persist. A yt-dlp group orphaned by such a kill is not stopped.

The first run is `--dump-single-json`. Before any transfer or package creation
Deadpan refuses playlists and multi-video results, metadata for a different ID
or extractor, live, upcoming and still-processing streams, videos longer than
six hours, videos without admissible streams and estimates above 16 GiB. The
`metadata` event reports title, author, duration, thumbnail URL, license and
the selected streams, with the rights notice.

Deadpan, not yt-dlp's format selector, chooses streams. The qualified source
decoder admits MP4/H.264 picture and AAC sound, so the candidates are direct
(`https`/DASH) DRM-free H.264 MP4 picture and AAC sound formats. Picture prefers
resolution, then frame rate, then bitrate. Sound prefers the original-language
track, then YouTube's normal dynamic range over its `-drc` compressed variant,
then bitrate. YouTube serves H.264 up to 1080p; larger VP9/AV1 renditions are
skipped rather than transcoded.

The second run downloads exactly those two format IDs with
`--load-info-json` from the inspected metadata, so it fetches the same video and
formats. It runs in the private download directory with the relative template
`%(format_id)s.%(ext)s`, so no path is ever parsed as a template, and with
`--quiet`. Free space for the downloads plus the assembled copy (and, beside the
package, for retention) is checked before transfer when sizes are known, and
again before assembly. A watchdog bounds the download directory to 16 GiB and
reports progress every 0.5 s. The inspected metadata file and the cookie copy are deleted as
soon as the transfer ends. Each file must exist, be a regular nonempty file and match a declared
size exactly. Package creation has not happened yet, so a failed or cancelled
transfer leaves nothing behind.

### Assembly

YouTube delivers its best picture and sound as separate single-stream
fragmented MP4 files, and the pinned developer FFmpeg prefix deliberately has
no `ffmpeg` program. The isolated media worker gained a `remux` mode
([`deadpan_media::remux_av`](../crates/deadpan-media/src/conversion/remux.rs),
C in [`converter.c`](../native/deadpan-media-worker/src/converter.c)). The host
appends the sound download to the disposable picture download, so the picture
is not copied, and passes that one descriptor; the worker opens each as
exactly one H.264 or AAC stream through descriptor-only AVIO with empty
protocol allowlists, copies packets in decode-time order into one progressive
MP4 without opening a codec, and reports packet counts, geometry and sound
format. Output is capped at the input size plus 2% and 16 MiB, never above what
retention accepts; deadline and cancellation are enforced on both sides.

Edit lists are written in the picture track's own clock (`movie_timescale`).
The default millisecond movie clock rounded a 1001/30000 s initial composition
delay to 990/30000 and moved every picture; the remux test caught this. The
worker then re-demuxes its own output and requires, per stream, an identical
SHA-256 over the time base, priming (`initial_padding`), trailing padding,
seek preroll and decoder configuration, and over every packet's PTS, DTS,
duration, key/discard flags, bytes and skip-samples side data, in its own
clock. Any difference fails with `verification_failed`. The stream language tag
is kept; `creation_time` and handler names are not. The output descriptor must
therefore be readable as well as writable. This adds no quality loss; the result
is a candidate original, not admitted media.

In the real run below, YouTube's own files account for the 146.000 s and
146.100 s lengths: format 137 holds 3,504 pictures of 512/12288 s (exactly
146.000 s, with an edit list starting media at 512 for B-frame reordering) and
format 140 holds 6,292 AAC frames of 1,024 samples at 44.1 kHz (6,443,008
samples, 146.0999 s) with no edit list and no declared priming. The assembled
file reports the same values, with identical packet tables.

### Project creation

The assembled file, named from the sanitized title and video ID inside the
private directory, goes through the same path as
`project create-original` ([`single_original.rs`](../crates/deadpan-cli/src/single_original.rs)):
`create_single_source`, managed retention (APFS clone or verified copy),
provenance, qualification of the picture and first audio track from a verified
snapshot, and `initialize_prepared_source`, which commits the receipt, asset,
full-source beat, presentation basis, history baseline and Ready profile in one
transaction. The package is built at a hidden sibling path
(`.<uuid>.creating.deadpan`) and moved to the requested path with a no-replace
rename only once it is Ready; any failure removes the partial package, so the
path holds a complete project or nothing and a retry can reuse it. A process
killed during creation can leave that hidden sibling behind. The private
directory, including both downloads, is removed at the end.

## Provenance

Database schema 62 adds `original_provenance`
([`original_provenance.rs`](../crates/deadpan-store/src/original_provenance.rs)),
keyed by the retained original's content identity, which must already exist.
Like transcripts and shot analysis it is operational metadata outside document
history: saving it creates no revision or Undo step, and the authored document
never contains the URL. Each record holds the service, video ID, canonical URL,
title, author name/ID/URL, license, upload date, duration, thumbnail URL,
retrieval time, helper versions (yt-dlp, yt-dlp-ejs, Deno), the selected format
IDs with codec, geometry, frame rate, bitrate and exact downloaded byte length,
and the assembly method. Text is bounded and free of control characters, URLs
must be HTTPS, and each record is at most 64 KiB; validation rechecks stored
rows. Credentials, cookies and request headers are never recorded. Remote titles
are untrusted display text: they label the asset and file but never form a path;
the app derives one sanitized library file name from them.

## Cookies

Authenticated import accepts only an explicit `--cookies` file in Netscape
format, at most 1 MiB. Deadpan copies it to an owner-only (`0600`) file in the
private directory, passes only the copy (yt-dlp may rewrite its jar), and
deletes the copy as soon as the transfer ends. The user's file is never
modified. Browser sessions are never read: every run passes
`--no-cookies-from-browser`.

## Errors

Codes are stable and messages say what to do next:

| Code | Meaning |
| --- | --- |
| `YouTubeUrlInvalid`, `YouTubePlaylistNeedsVideo` | Unsupported URL, or a playlist without a chosen video. |
| `DownloaderNotInstalled`, `DownloaderHelperInvalid`, `DownloaderUnsupportedPlatform` | Run `downloader install`; a damaged helper is never replaced automatically. |
| `YouTubeVideoUnavailable`, `YouTubeVideoPrivate` | Deleted, removed or private video. |
| `YouTubeRegionRestricted` | Not available in this region. |
| `YouTubeAgeRestricted`, `YouTubeSignInRequired` | Retry with an explicit `--cookies` file. |
| `YouTubeRateLimited` | Bot check or HTTP 429; wait, or use cookies. |
| `YouTubeLiveUnsupported` | Live, upcoming or still-processing stream. |
| `YouTubeFormatUnavailable` | No direct H.264 picture or AAC sound offered. |
| `YouTubeExtractorFailed` | YouTube changed; the pinned helpers need an update. |
| `YouTubeNetworkFailed`, `YouTubeDownloadFailed`, `YouTubeDownloadIncomplete` | Retry the import. |
| `YouTubeTooLong`, `YouTubeTooLarge`, `YouTubeMetadataTooLarge`, `YouTubeMetadataInvalid` | Bounds or malformed metadata. |
| `YouTubeAssemblyFailed`, `DownloaderAssemblyUnavailable` | The media worker refused or is missing beside the CLI. |
| `YouTubeInsufficientSpace`, `DownloaderOutputTooLarge` | Not enough free space; a helper printed more than allowed. |
| `YouTubeCookiesInvalid`, `DownloaderTimeout`, `ImportCancelled`, `DownloaderFailed` | Cookie file, deadline, cancellation, unclassified failure. |

Not every YouTube URL is downloadable, and classification of yt-dlp's English
diagnostics is best effort; unrecognized failures keep the helper's final error
line in the message.

## Evidence

- URL unit and generated-ID property tests; helper install, never-overwrite,
  tamper, cancellation and ZIP extraction tests with an in-memory transport.
- Acquisition tests for argument vectors, private environment, stream selection,
  metadata refusals, error mapping, cookie handling and file naming, plus
  stand-in yt-dlp scripts proving that failures before and during transfer
  create no package and pass only the canonical URL.
- [`remux_real_media.rs`](../native/deadpan-media-worker/tests/remux_real_media.rs)
  runs the real worker on split fragmented fixtures (progressive output, swapped
  and duplicate streams, truncation, output budget, cancellation) and compares
  the pinned `ffprobe` packet tables, padding and side data of input and output.
- [`youtube_import.rs`](../native/deadpan-media-worker/tests/youtube_import.rs)
  runs `create_from_url` with a stand-in yt-dlp and the real worker, store and
  decoders, and checks the Ready profile, managed original, asset label, cookie
  handling and provenance.
- [`youtube/tests.rs`](../crates/deadpan-app/src/youtube/tests.rs) drives the
  app job with a scripted downloader that creates the project from a fixture
  through the real single-Original path: explicit confirmation before any
  package, decline and mid-transfer cancellation leaving nothing, the explicit
  install continuing the import, codes and guidance, one job at a time,
  drained shutdown, expiry of unconfirmed details, a destination taken during
  the build moving to `<title> 2`, and a package published after Escape still
  opening. `single_original` tests the same collision at the final rename. Router, validation, library naming and command tests
  cover the keys and text.
- The `youtube` [UI replay](UI_FEEDBACK.md#implemented-scenarios) uses the same
  scripted downloader with production input, widgets, project service and Metal:
  start card at both sizes, `⌘⇧N`, live refusal, paste, install offer, details,
  decline, 40% progress and cancel, an age-restricted refusal, a chosen cookies
  file, success opening the project, and the sheet over an open project.
- Real app-job run, 2026-10-04, debug build:
  `DEADPAN_REAL_YOUTUBE_URL='https://youtu.be/Z4C82eyhwgU?si=share' cargo test -p deadpan-app real_url_import`
  ran the pinned helpers and media worker through `Jobs`: details after 2.7 s,
  confirmation, transfer, assembly at 5.7 s and a valid Ready project after
  16.7 s in a temporary library. The test returns early without the variable.
- Real run, 2026-10-04, Apple Silicon macOS, debug build: after `downloader
  install` (4.5 s) and `downloader status --probe` (`matches_pins: true`),
  `create-from-url 'https://youtu.be/Z4C82eyhwgU?si=share'` ("Caminandes 2:
  Gran Dillama", Blender, CC-BY) fetched metadata, selected format 137
  (`avc1.640028`, 1920x1080, 24 fps, 52,740,343 bytes) and 140 (`mp4a.40.2`,
  44.1 kHz stereo, 2,365,262 bytes), assembled a 55,131,001-byte MP4 and
  created a Ready project in 15.4 s. The qualified asset has 3,504 pictures over
  146.000 s and 146.100 s of audio; the edit is 3,507 frames at 1920x1080,
  24 fps. `project validate` passed, and provenance reads back. A rerun after
  the review fixes (picture-clock edit lists, self-verified assembly, staged
  package, in-place append) took 16.7 s, passed the worker's timing check,
  produced a valid Ready project of the same 3,507 frames and left no private
  directory or staging package behind.

## Remaining work

- A thumbnail in the confirmation step (needs an image decoder for the app).
- Helpers shipped and signed in the application bundle, signed update
  manifests, compatibility checks and rollback; Linux and Intel builds.
- VP9/AV1 and above-1080p originals need decoder qualification first.
- Resumable transfers, clean-machine acceptance and a broader failure corpus.
