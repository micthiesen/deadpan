# YouTube network confinement, 2026-10-11

Every YouTube metadata and transfer helper now runs inside a macOS sandbox
that permits only the import's owned CONNECT gateway. The gateway admits
supported YouTube service names, HTTPS port 443 and public-unicast resolved
addresses. Helper probes have no network exception. There is no unrestricted
fallback when the boundary cannot start.

## Failure reproduced

Strict starting-URL normalization did not constrain the helper's later
connections. An owned HTTP server and a fabricated `--load-info-json` record
proved that the real pinned yt-dlp 2026.08.19 would fetch
`http://127.0.0.1:<owned-port>/owned` when the format claimed `protocol: https`.
It exited successfully and retained the server's response. This is a local
controlled witness, not an attack on YouTube or another user's service.

A separate owned-file witness confirms that the same pinned helper refuses a
`file://` stream URL even when its metadata claims `protocol: https`: it
reports that file URLs are disabled and creates no download. Deadpan never
passes the option that enables them. This existing local-file restriction is
independent of the new connection boundary.

The same host's network-denied sandbox prevented curl from contacting the
owned server. A localhost-port exception allowed it. Seatbelt rejects numeric
addresses in this filter with `host must be * or localhost in network address`;
the final gateway reserves both IPv4 and IPv6 loopback listeners at one port
before granting the localhost exception. The initial numeric-address test
failures are retained.

## Implementation and review

The helper receives one random per-run proxy credential in its argv and a
cleared environment. The gateway validates authentication before attributing
a policy violation to the import. Unsupported hosts, IP literals, non-443
ports, plaintext requests and ambiguous framing fail closed. Each new tunnel
is checked, including connections after redirects. A public address from the
bounded resolver result is passed directly to `connect_timeout`; the gateway
does not perform a second DNS lookup. TLS and certificate validation remain
in the helper, and the gateway never parses encrypted cookies or media URLs.

Resource limits are 16 connections, an 8 KiB header, two 32 KiB relay buffers
per connection, five seconds each for headers and DNS, a shared five-second
connection budget and 60 seconds of relay inactivity. Stop joins gateway
threads. A timed-out OS lookup retains its one global resolver slot until it
returns, preserving the existing thumbnail resolver's resource bound.

The complete diff was reviewed by the sole implementation agent. The generic
process supervisor remains available to the already sandboxed AI runtime;
only YouTube call sites gain the new wrapper. Thumbnail fetching shares the
existing address policy and bounded DNS worker. No new networking dependency
was added; `rustix` enables its existing event feature for bounded polling.

## Focused evidence

All 86 selected YouTube and privacy-audit tests pass. Real sockets verify
1.2 MB of byte-preserving bidirectional traffic, half-close replies, idle cancellation,
bounded shutdown, both reserved loopback endpoints, unauthenticated rejection
without poisoning an import, and an approved host resolving to private IPs.
Actual curl processes under the production wrapper cannot contact the owned
private HTTP/HTTPS service or bypass the proxy with `--noproxy '*'`.

The CONNECT mutation target starts with an exact 116-byte request captured
from the pinned helper using a public fixture credential. The owned capture
proxy returns 403 without making an upstream connection. The production
parser must either refuse or admit a supported destination that rejects a
different credential, within allocation and input bounds.

The first compilation exposed the shared AI supervisor's separate ownership;
the wrapper was separated before tests passed. Extending the static secret
audit initially mistook inline test initializers and an unrelated generation
authorization type for credential-bearing fields. The corrected audit checks
the actual secret/password declarations, including the proxy credential.
Those failed commands remain in the evidence and are not counted as passes.

The rebuilt debug CLI imports Blender's *Caminandes 2* from
`https://youtu.be/Z4C82eyhwgU` through the boundary in 12.802 seconds, then
validates the complete Ready project. The standalone helper probe also passes.
Executable hashes and raw reports are retained.

## Completion evidence

The repository gate passes formatting and strict workspace/UI Clippy, then
runs 5,682 workspace tests: 5,681 pass and one HDR inspection test fails.
Its encoder duration proof still budgeted one AAC priming packet although the
current encoder emits two. The [separate correction](mux-preroll-2026-10-11.md)
passes a deterministic old-code regression, all 64 native encoder tests and
172 encoded-output tests, including the original failure. All 1,105 UI tests,
both doctests and final strict workspace/UI Clippy also pass. One passing UI
gain test reported lingering output; its isolated follow-up passes without
that warning. The original warning is retained, with no established cause.

The real native app job checks *Caminandes 2*, requires a decoded 480×270
thumbnail before confirmation, verifies that no project exists yet, then
creates and validates a Ready Original in 13.8 seconds. This uses the current
gateway and shared thumbnail resolver, without cookies or an authenticated
account.

The 15-second stable CLI-boundary campaign performs 6,688,260 executions over
eight targets with no failures. The new CONNECT target performs 5,156,846
executions, including 332,291 accepted mutations and both refusal classes;
maximum measured per-case allocation is 256 bytes and time 0.460 ms. These are
parser observations, not network throughput measurements or exhaustive proof.
The fresh ad hoc signed bundle passes relocation, source/media smoke tests,
bundled helper probes, the offline AI runtime's GPU smoke and all helper,
runtime and notice tamper negatives. This confirms that the YouTube-only
wrapper preserves the AI runtime's separate offline launch boundary.

A second relocated copy of `deadpan-app --headless`, with fresh `HOME` and
`TMPDIR`, system-only `PATH` and no inherited developer configuration, imports
Blender Studio's *CHARGE* (`UXqq0ZvbOnk`) in 38.023 seconds. It retains
3840×1608/24 AV1 format 401 (111,440,722 bytes) and Opus format 251
(4,365,234 bytes), then reopens and validates the complete 6,306-frame baseline.
Signature verification and workspace/staging cleanup pass. The source credit
is retained with the reports; YouTube's absent license value stays absent.
The executed app SHA-256 is
`9aff891ab04aeec8d6592dc0c277f6bc2e425ce86e6a953553e5eb4f62ff3b64`.
This is observed import timing with uncontrolled caches, power and thermal
state, not a playback benchmark or a second-machine result.

## Scope

This addresses specification §15.4's network confinement and preserves the
existing import and thumbnail semantics. It does not qualify a new platform,
account-restricted YouTube content or an upstream service behavior change.
No native window, live browser or other agent was used. The
[final acceptance audit](youtube-acceptance-2026-10-11.md) closes DP-14 under
§29.1 while retaining exact owner checks. The full product goal remains open.

[Evidence](../../tools/media-qualification/evidence/2026-10-11-youtube-network/README.md)
retains commands, failures, source and binary hashes, source fixtures,
the real import reports and packaged provenance. Scratch imports live under
`/tmp/deadpan-youtube-network-20261011`.
