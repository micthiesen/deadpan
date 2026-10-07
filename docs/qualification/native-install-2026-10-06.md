# Native installation workflows, 2026-10-06

These runs exercise specification §26.6 on the development Mac through the
native Accessibility API, without screenshots. They supplement the scrubbed
bundle, model, helper and signing checks in [the release audit](../RELEASE_AUDIT.md).
They do not establish clean-machine acceptance.

## Artifact and environment

- Apple M5 Max, macOS 26.5.2, Darwin 25F84.
- Relocated packaged `Deadpan.app`, 721.9 MiB, hardened runtime and ad hoc
  signatures. Executed app SHA-256:
  `6c4926e00aab2aa22ae15d7fdd7bdf382d7a0d1b03af06b888a9dca0c34ae9e7`.
- Launch environment contained only a fresh `HOME`, `PATH=/usr/bin:/bin`
  and a fresh `TMPDIR`. No development runtime variables were set.
- Native Foundation Documents and recovery/settings locations still resolve
  to this Mac's user directories. `HOME` isolation does not emulate another
  macOS account. Model packs used the fresh HOME's Application Support root,
  confirmed in the Models panel.
- Evidence root: `/tmp/deadpan-resume-20261006`. Both native test windows
  were closed after their runs.

## Online workflow

1. Command-N and the native picker imported the repository's
   `cfr-bframes.mp4` fixture. Accessibility reported Saved, the complete
   120-frame baseline and 30000/1001 fps.
2. Models downloaded and smoke-tested the 149 MB whisper/Silero pack and
   the 36.15 GB LTX/Gemma pack. The test reused the owner's prior acceptance
   of the same pack licenses.
3. During the AI pack download, Command-Shift-N fetched the permitted
   Blender fixture `https://youtu.be/Z4C82eyhwgU`. Native confirmation showed
   Caminandes 2, 1920×1080, 24 fps and AAC audio before download. The new
   project retained 3,504 pictures and 3,507 edit frames, including the
   measured audio tail.
4. `800l` and `:hold 24f` created a silent one-second freeze at Edit
   boundaries 800–824. Generation ran through the native inspector.
5. Ready left the authored document byte-identical to the fallback dump.
   Clicking Accept committed revision
   `d4b71462-a5fe-44ec-8d85-5d6549983dd1`, changing the picture to Accepted AI.
6. The app quit normally, restarted with IP networking denied and reopened
   that exact revision. Command-E rendered all 3,531 frames to the default
   Documents/Deadpan/Exports location.

The movie passed emitted-file verification and was atomically saved. Final
filesystem confirmation reported `destination_changed`; native destination
recovery also refused confirmation. The report's BSD flags changed from
0 to 64 (`UF_TRACKED`) after its durable identity was captured. Its inode,
owner, mode, size and modification time stayed the same. The movie's
151,031,181 bytes match the recorded SHA-256:
`05eb3c98cfb982f92df76ecdafab0197eba2c8c78549a67bdb27f0fd4c37a2f8`.
There were no movie extended attributes. This initial run remains recorded
as a real publication failure. The [2026-10-07 follow-up](followup-2026-10-07.md)
then recovered this exact artifact through the native Destinations panel.
It passed full byte verification and now has durable `published` status,
with no recovery diagnostic and the same movie hash. Both earlier failures
remain in the operational journal. A fresh native Documents render also
published successfully after the fix.

Evidence: `native-online/{fallback,before-accept,accepted,reopened}.json`,
`render_publications.json`, `render_publication_operations.json`,
`movie-readback.json`, `render-report.json`, and both app logs.

## Full offline distribution

`offline-dist` copied the same signed app and exported both installed packs:

| Pack | Archive bytes | Fresh verification import |
| --- | ---: | ---: |
| whisper-base-en 2 | 148,853,760 | 6.503 s |
| ltx-2.3-q4-bridge 1 | 36,152,920,576 | 18.283 s |

Distribution construction took 871.958 seconds. `offline-dist-verify`
passed manifest coverage, every checksum, strict nested signature verification
and both fresh imports with smoke tests in 411.247 seconds. The AI archive
SHA-256 is
`2f672a6fee21c8dc7680b23e60beefd19ed88256edd24279b13fe3ededd5d721`.
Logs: `offline-dist-build.log` and `offline-dist-verify.log`.

The native distribution app started with another fresh HOME and IP networking
denied. Models showed neither pack installed. Install from archive installed
and tested whisper/Silero; the AI archive was fully extracted and verified,
but its runtime smoke test encountered the nested-sandbox limit below.

After restarting without the outer test sandbox, Install from archive
completed the same local AI archive's smoke test. The production AI worker
retained its own network-denying sandbox. No model download action was used.
Command-N imported a fresh copy of the local fixture. `60l` and `:hold 30f`
created a 30-frame freeze, then Generate produced a candidate. The Ready
document remained identical to the fallback. Explicit acceptance committed
`f13e3079-2d80-4b07-b8ea-caffe4c594dc`.

The app quit, restarted with IP networking denied, reopened the accepted
revision unchanged and rendered through Command-E into the scratch Exports
directory. Accessibility reported **Movie exported**. Durable publication
status is `published`, with no diagnostic. The movie contains 150 frames;
its 148,246 bytes match the emitted report and SHA-256
`79d02c3fa5223507b1f008b229d6dacb0807a0663f6bc1044f192e8f7caaed6c`.

Evidence: `native-offline/summary.json`, the four document dumps,
`render-publications.json`, `Exports/`, `offline-app.log` and `archive-app.log`.

## Network-denial limits

The whole-app test profile denies IP outbound, inbound and bind operations
while allowing local Unix sockets. A positive control reached local TCP and
UDP endpoints; the same calls under the profile failed with EPERM. Unix
socket traffic succeeded in both cases. Script and evidence:
`check-offline-network.py` and `offline-network-check.json`.

An initial profile denying all sockets also blocked Deadpan's local host
transport. This was a harness failure; no application restriction was relaxed.
The IP-only profile fixes that test setup.

macOS refuses a second `sandbox-exec` application inside the app's outer
profile: `sandbox_apply: Operation not permitted` (exit 71). Deadpan's AI
smoke test and inference intentionally launch their own stricter profile.
Consequently this run proves initial launch, whisper archive installation,
restart, reopen and rendering under whole-app IP denial, and AI archive
installation/generation under the AI worker's own denial. It does not prove
the entire workflow under one uninterrupted whole-app network restriction.

The physical-disconnection run belongs to **To verify (owner)** under §29.1:
disconnect this Mac's network interfaces, start the full offline distribution
with empty model storage, install both archives in Models, generate and accept
a pause, quit, reopen and render. No installed development tools or model
servers may supply the runtime. A true clean or second Mac remains a separate
owner check.
