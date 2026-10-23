# Scratch runner drafts

Drafted only. None of these files has been executed, compiled, formatted or tested. No source copy or overlay exists yet. The final qualified source commit and immutable baseline remain pending from Root.

- `check_profile.py`: four-row/six-line whitelist parser plus gate-disabled absence check. Input work is bounded to 64 MiB and 65,536 bytes per stderr line. Decimal conversions enforce u64 counts/u128 elapsed values, sums are checked, and the only subtraction is the per-trial outer-minus-two-disjoint-leaves residual.
- `test_profile_parser.py`: synthetic success, gate-off, row/order/block/syntax/count/marker/containment/overflow and bounded-input cases. These are drafted tests, not passing test evidence.
- `memory_observer.rs`: external macOS runner using the exact existing `deadpan_native_process` rlib. It accepts an executable and arguments, with no PID-sampling interface, and directly owns the app's Child/process group.

Observer interface:

    memory_observer --receipt <terminal.json> --samples <raw.json> --stdout <app.stdout> --stderr <app.stderr> -- <app> <app arguments>

The observer inherits the caller's cwd and environment, including whether the profile gate is set. Root must pass the same cwd/environment as the original native producer and pin the observer, app and linked adapter hashes. The observer does not run a shell or wrap the app in another process. Root must compile it with the locked copied-source rlib and its exact dependency search path; no new Cargo manifest/dependency is proposed.

Memory sampling starts immediately after guarded spawn, requests a 5 ms pause between queries, retains at most 60,000 rows and checks a 180-second cooperative run bound. Actual query durations and sample gaps are recorded. Existing libproc calls are synchronous, so these checks cannot preempt a stuck system call. Cleanup has a separate checked five-second group deadline and, if needed, five-second leader deadline.

Every post-spawn terminal path, including sample refusal/error and time/sample bounds, attempts existing checked group teardown, then checked leader fallback if needed. A confirmed exited leader is reaped once, with reap-attempt state set before wait and no retry after wait error. If both cleanup paths fail and exit is unconfirmed, the observer skips blocking wait, writes a failed/unreaped terminal receipt and exits. It cannot truthfully promise successful cleanup when the existing checked adapters fail. Such a receipt invalidates the trial and explicitly preserves possible remaining-child/descendant risk for Root to resolve.

Setup/spawn failures produce failed terminal receipts when the already-open receipt destination works. Raw sample output failure is recorded in the terminal receipt after cleanup. A receipt-output device failure produces stderr plus nonzero exit; missing/nonterminal/failed receipts are rejected. Zero samples produce null maxima, never fabricated zero memory. Nonzero app exit, any sample error, unconfirmed group cleanup, reap error or output error invalidates memory evidence while retaining available observations.

Reported maxima are sampled lower bounds for the main app over its whole replay lifetime. They include startup/opening and later app work, exclude workers and external GPU/services, and cannot qualify per-leaf memory, an exact peak or a memory ceiling. Timing and memory lanes remain separate. No timing, memory or overhead result is claimed.

Review before use: compile/parser validation; exact observer rlib/dependency linkage; setup/spawn failure receipt; real owned-child natural-exit cleanup; timeout cleanup; sample-refusal/error cleanup; group-failure/leader-fallback and reap-error handling; output-failure receipts. No runner failure authorizes changing runtime adapters or sampling arbitrary PIDs.
