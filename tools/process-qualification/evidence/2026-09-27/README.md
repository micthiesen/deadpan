# macOS subprocess pipe inheritance

[Qualification](../../../../docs/qualification/process-launch-2026-09-27.md)
explains the defect, owned launch inventory and cooperative scope.

`proof-result.json` records the real raw/guarded witness, source hashes, tool
versions, command output and confirmed cleanup. `verification.json` records
formatting, strict affected and app-harness lint, 146 affected worker/media tests
and 246 app-harness tests. Compressed logs retain exact output and the failed
first proof driver. That initial attempt stopped during cleanup of `rustc
--version`, before the race fixture ran; it is not a product-test failure.

`failed-proof-source-identity.json` and `cargo-source-identity.json` distinguish
the initial driver from the unchanged production state exercised by Cargo.
The corrected witness hashes its own inputs. Four pure mocked cleanup tests
were reported passing by the implementing agent; their source is retained, but
no separate raw log was retained.

The original workspace supervisor failures and the one diagnostic retry remain
in [root-sound evidence](../../../media-qualification/evidence/2026-09-27-root-sounds/README.md).
No timeout, pipe-EOF assertion or worker concurrency was relaxed to pass.
