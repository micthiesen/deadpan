# Broad check and retained failures

`cargo xtask check --since 92907cb0 --filter 'not binary(preview_export)'` passed formatting and strict Clippy, then ran 2728 tests: 2726 passed, 2 failed, 9 ignored tests plus the deliberately excluded 45-test preview-export binary. Runtime 176.195 s; optimized test build 8m 45s. No new Repeat/core/audio test failed. All initial fixture corrections passed; no LEAK result occurred in this broad run. The earlier cancelling focused run's two passed-test LEAK labels are retained; their cause was not established.

Failures:
- youtube::helpers::tests::bundled_baseline_binds_resigned_bytes_to_the_compiled_pin: production code-signature verification timed out at 60 s. No signing/timeout code changed in this increment. An isolated serial rerun is owed; a passing rerun will not establish the cause.
- proxy_disk_full::a_full_cache_volume_fails_proxy_builds_truthfully_and_recovers: expected ConversionError::Worker, got host ConversionError::Io(StorageFull). The test filled APFS before staging's mkdir/open. The exact failing syscall is not reported by raw Io. Reviewed test-only repair precreates the first production staging pair, consumes it once, retains fresh output on retries, and preserves the Worker-only disk-full assertion and all cleanup/publication/recovery checks. Actual repaired run is owed.

The binary inventory command following this run rebuilt ring/rustls/CLI targets before release. Its inventory describes paths after that build, not a claim that every inventory executable was executed in the broad run. Core/audio sources and executables were not rebuilt by that inventory command. The separate old-code witness and final release export have individually pinned executed binaries.

Serial follow-up: both failures pass, code-signature baseline in10.170s with no source change; repaired worker disk-full fixture in3.284s. Cause of the original signature timeout remains unproven. Strict lint passes again.
