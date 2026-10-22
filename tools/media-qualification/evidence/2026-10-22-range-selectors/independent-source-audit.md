# Independent source audit

Audited directory: /Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-22-range-selectors
Target commit: 32598481ba9e66a0a37c79df3967d5e3fd170ebe

- Source binding passed: 2000 Rust paths, exact path set and blob hashes.
- Six input hashes passed. The retained release source tar matches all source identities.
- Broad suite: 4596 passed, 782 skipped; receipt and listed case inventory agree (5378 listed cases across 197 suites).
- UI suite: 1281 passed; log agrees (Summary [  68.818s] 1281 tests run: 1281 passed, 11 skipped); inventory lists 1292 cases across 2 suites.
- Native checks: visual: selector 85 checks, failed=False; Kestrel 1 check, failed=False; top-level failed=False. performance: selector 85 checks, failed=False; Kestrel 1 check, failed=False; top-level failed=False. Reported findings are warnings; no scenario failed.
- Media passed: 30 pictures; audio samples [0, 48048]; measured offset 0, status verified_zero, flags [], no failures.
- Owner fixture archive has 9 safe members; all member hashes match. Validate and doctor exited 0.
- Evidence inventory: 238 entries / 42105257 bytes; every listed size and SHA-256 matches. Inventory JSON SHA-256: be362b24fb93803b1f2b6dce4b45459677e3cfbdd90e78e9cafe1219d2512783.

The earlier directory mistake is retained as independent-wrong-directory-audit.json/.md and is not used as evidence here.
