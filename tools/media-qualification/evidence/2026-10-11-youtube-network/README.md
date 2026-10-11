# YouTube network confinement

See the [qualification](../../../../docs/qualification/youtube-network-2026-10-11.md)
for implementation, retained failures, verification outcomes and limits.

- `checks/` retains the pre-fix pinned-helper local HTTP witness, initial
  sandbox/profile and audit failures, focused and repository checks, real CLI
  import reports and command timings. A compile failure exposed the shared AI
  supervisor's separate sandbox ownership; the final wrapper is YouTube-only.
- `connect-fixture/` contains the exact pinned helper's CONNECT request and
  its capture command. `deadpan:fixture` is an intentionally public test
  credential. The owned proxy returned 403 without contacting YouTube.
- `file-url-probe/` records the pinned helper's refusal to download an owned
  local file, despite metadata claiming an HTTPS protocol.
- `chaos/` contains the stable adversarial campaign and executed binary hash.
- `packaged-4k/` records the actual relocated app's helper probe, URL import,
  validation, reopened document/view, diagnostics, signature check and timings.
- `bundle/` retains release checksums, software inventory and build provenance.
  `scripts/` contains the executed local harnesses, with their original paths.
- `source-identity.json` binds the reviewed source and packaged executables.
  `compression.json` records original lengths/hashes for deterministic gzip
  files. `SHA256SUMS` covers every retained file except itself.

Public-video checks use *Caminandes 2* (`Z4C82eyhwgU`) and *CHARGE*
(`UXqq0ZvbOnk`), Blender Studio. The packaged source credit is retained in
`packaged-4k/identity.json`. No cookies or authenticated account were used.
No movie is duplicated here. Full apps and scratch projects remain under
`/tmp/deadpan-youtube-network-20261011` and
`/tmp/deadpan-youtube-network-bundle-20261011`.
