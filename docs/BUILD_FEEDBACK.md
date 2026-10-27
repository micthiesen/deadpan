# Fast local Rust feedback

Use Blaze selectively for repeated pure Core edits. Keep Cargo for the native
app, media tests, compiler diagnostics, qualification, packaging and CI. The
2026-10-26 trial found real Core acceleration but JIT panics on both the native
media test target and the app. A successful exit after Cargo fallback is not a
successful Blaze compilation.

## Routine loop

Pure logic feedback uses unoptimized code and incremental compilation:

```sh
env -u CARGO_PROFILE_DEV_OPT_LEVEL -u CARGO_PROFILE_TEST_OPT_LEVEL \
  CARGO_INCREMENTAL=1 cargo test -p deadpan-core --locked --lib <filter>
```

For repeated Core body edits, the installed experimental engine can shorten
that loop:

```sh
env -u CARGO_PROFILE_DEV_OPT_LEVEL -u CARGO_PROFILE_TEST_OPT_LEVEL \
  CARGO_INCREMENTAL=1 blaze test -p deadpan-core --locked --lib <filter> \
  --target-dir target/blaze
blaze status
```

Keep the package, profile, feature and flag selection stable between edits.
The first build includes Cargo bootstrap and daemon loading. Manifest, build
script and test-inventory changes can require Cargo again. Inspect the output
and status: `[ra-jit]` identifies the accelerated build, while a panic or
fallback must be reported. Use plain Cargo if the engine fails; do not change
correct Rust code or weaken assertions to satisfy it.

Keep the Blaze target separate from authoritative Cargo outputs. Do not enable
global Cargo interception with `blaze enable`. Use ordinary `cargo check` and
strict Clippy before accepting a change: Blaze's compiler and borrow checking
are not the correctness authority. Its `check` command is not a substitute.
Deadline-sensitive audio/media tests still require the optimized Cargo profile
described in [Development](DEVELOPMENT.md), and physical-Mac qualification stays
local as well as CI. Release builds, bundle checks and measurements use Cargo.

## Installation tested here

The installed source is indexable-inc/blaze commit
`2a84ef9c6fc0a56efe406838f2a4aadbf9bf0d96`, compiled with
`nightly-2026-10-25` (`rustc 1.101.0-nightly`, `12433c307`). The six binaries
are in `~/.cargo/bin`. Cargo qualification continues to use the repository's
Rust 1.97.1 pin; Blaze selects its own compiler. No global Rust override or
Cargo interception was installed.

The published October 20 prerelease predates the current Mac acceleration
support, so the trial built the pinned current source rather than treating
that archive as the current advertised engine. Installation took 136 seconds.
Reproduction starts by installing that nightly with `rust-src`, then building
the pinned source's `ra-jit` package in release mode with all its binaries.
Do not use the other-toolchain ABI escape hatch.

## Results and limits

Three equivalent body edits in the actual Core crate measured Cargo rebuilds
of 9.442, 9.327 and 5.890 seconds, against Blaze's 0.410, 0.318 and 0.146 seconds.
The medians are 9.327 and 0.318 seconds, about 29 times faster for this narrow
loop. Both workflows passed all 794 Core tests. The tested source was an
isolated copy of the shared development tree, including its existing sound
WIP; this is tooling evidence, not product completion. Cargo used Rust 1.97.1
and Blaze used its pinned nightly, so these compare complete workflows rather
than isolating the code generator alone.

The first Blaze Core bootstrap took 106 seconds. The comparable Cargo build
phase took 59 seconds. Full test execution was 89.5 versus 53.9 seconds in
these trial runs; this was not an isolated performance qualification. The
Core worker reported about 13 GiB of retained memory. Fast compilation does
not establish faster runtime or fulfillment of Deadpan's performance budgets.

The native path initially aborted because Blaze's injected arm64 sandbox
library could not load into Apple's arm64e compiler/archive launchers. Direct
Clang, C++, archive and SDK paths avoided those launchers with sandboxing still
enabled. The media JIT then panicked in a frozen definition map, and the app
JIT also panicked. Cargo fallback built the media target and its two exact
picture-range tests passed. A changed app body was also rebuilt by Cargo
fallback and its marker appeared in a headless launch. These passes do not
qualify native JIT acceleration or hot reload in Deadpan.

The optional ui-harness app also hit the JIT panic. Its Cargo-built fallback
passed the offscreen Metal transcript replay: 70 transcript assertions and
the live Kestrel audit, 71 checks total. This confirms the fallback works on
the actual native path; it grants no JIT performance or release credit.

The separate live-reload probe preserved PID and state across a body edit;
the build took 28 ms and patch application 351 microseconds. Changing the
existing state type requested a restart. One earlier edit requested a restart
because the executable had been linked again. A newly added test was discovered
and both tests passed; a basic invalid borrow was refused by both Blaze and
Cargo. That single refusal does not qualify Blaze's incomplete borrow checker.
This proves the small probe only. Do not
add production hot-reload dependencies or relax app signing/entitlements on
that evidence.

## Build-directory hygiene

Run `cargo xtask hygiene` before a long development session and after substantial
growth, with no compiler using the selected target. It resolves Cargo's actual
target through metadata, including environment and config overrides. Above
40 GiB it sweeps old artifacts toward 20 GiB, preserving evidence outside Cargo
profile directories. The trial reduced the old target's debug artifacts from
about 103 GiB to 17 GiB. Keep routine feedback out of the qualification shell's
`CARGO_INCREMENTAL=0` and optimization overrides.

Raw commands, output, executable/source pins, worker panic logs, expected
negative tests and the replay report are retained in
[the trial evidence](../tools/build-qualification/evidence/2026-10-26-blaze/inventory.json).
Two source-copy setup mistakes and three hygiene-test fixture corrections are
retained as such, separately from the compiler failures. No shipping gate,
performance target or product requirement closes from this tooling trial.
