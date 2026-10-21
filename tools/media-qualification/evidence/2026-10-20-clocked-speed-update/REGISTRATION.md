# Scratch old-code witness

Prepared only. No compile or runtime claim. Parent owns Cargo and integration.

Copy `speed_update.rs` from this scratch directory to:

`crates/deadpan-core/tests/insert_time/retimed/speed_update.rs`

In `crates/deadpan-core/tests/insert_time/retimed.rs`, register:

```rust
#[path = "retimed/speed_update.rs"]
mod speed_update;
```

The child module imports the existing parent `fixture`, `pause`, `retime`,
`ratio`, and the parent integration test's public-command helpers through
`use super::*`. No fixture duplication or production patch is required.

Run only this test on the retained old implementation, with an explicit evidence
directory and a retained log. The parent may use Cargo test or its equivalent
nextest filter:

```sh
export DEADPAN_SPEED_UPDATE_WITNESS_DIR=/tmp/deadpan-speed-update-clocks-2026-10-20/old-witness
cargo test --locked -p deadpan-core --test insert_time retimed::speed_update::set_retime_updates_a_stage_with_retained_exact_pause_clocks -- --exact --nocapture
```

Expected old outcome, derived from source and not yet executed: all baseline
checks and public InsertTime pass, then the intended `SetRetime` success
assertion fails with `InvalidCommand` and:

`this edit would change content inside a speed change with exact pause clocks; edit outside it or undo those pauses first`

`before.json`, `request.json` and `observed.json` are written to the explicit
evidence directory immediately before the success assertion. The files are
bounded by this tiny fixture; no application writes or project package are
involved. Unset the environment variable to disable this optional evidence.
Use a fresh directory per binary/source identity so later runs do not replace
old evidence. Record the executed test binary hash alongside the source hash
manifest, using separate target directories for archived-source comparisons.

The intended future result is output15 with exact selection `[0,27/2)` and
root19. It retains descendant clocks/bindings and has exact pause start11/3,
right fragment start16/3 and outer suffix start17. It does not assert16-frame
recipe-based speed resolution or accept the current refusal as success.

`source-sha256.txt` records source files read while writing this proposal.
`scratch-sha256.txt` records the prepared scratch module/instructions. Neither
manifest proves a compiler or runtime outcome.
