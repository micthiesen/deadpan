# Independent review disposition

Base: `origin/main` at `097f7358574c4da42cc2e607555978abf9d757a9`, plus this
increment's tracked and untracked changes. Three independent reviewers covered
general correctness, recovery/byte ownership, and migration/test validity.

## Applied

- General review: routine render reads/transitions/checkpoints reparsed every
  historical attempt report. They now inspect selected bounded rows and scalar
  index/capacity metadata. Full semantic audits remain on open/explicit validate.
  A regression inserts an unrelated invalid report and proves targeted
  operations skip it while full validation and reopen reject it.
- General review: each job rehashed its historical document under one arbitrary
  aggregate 30-second open cutoff. Validation now caches project/duration/hash by
  revision and uses the same bounded canonical writer without that wall-time
  validity cutoff. Worker hashing retains its caller's cancellation/deadline.
- Recovery review: closing the owner could leave an active child running until
  its deadline. Both supervision loops now poll ownership, request cancellation
  and retain the failure through teardown. Deterministic children confirm receipt
  of that cancellation for encoder and verifier, with no user cancel flag.

All three final reviews report no remaining findings.

## Dismissed or clarified

- Runtime scalar counts/head checks still inspect bounded key indexes. They do
  not deserialize old reports and have fixed 4,096-job/65,536-attempt caps. The
  reviewer rechecked and accepted this correction; constant-time query behavior
  is not claimed.
- Synchronous historical SQLite preparation checks ownership before and after
  bounded calls. It has no live child or admitted result during the call. The
  existing cooperative control boundary does not preempt SQLite itself.

Parent triage retained the synthetic legacy fixture fixes: only empty new render
tables are removed before explicitly synthetic modern history is labeled with an
old schema. Genuine schema-39 SQL remains unchanged, and real vocabulary
collisions still reject migration before promotion.

After native qualification, Clippy requested a named tuple type in
`read_attempt_body`. The correction changes only a local type alias; SQL, tuple
fields and executable operations remain identical. Final checks retain exact
source differences rather than attributing earlier native execution to later
source bytes.

## Full-gate corrections

The full workspace run completed with three failed targets and retained all
passing results. The CLI doctor test still expected database 39; its assertion
and migration capability string now name database 40 and migration through 39.
The store corruption fixture tried to write a sealed 0444 movie directly; it now
sets 0600, writes the corrupt bytes and restores 0444 before testing rejection.
The playback fixture accepted an older Playing update before its injected report
was consumed. A source-only independent investigation identified that race; the
fixture now waits for Playing at the exact asserted sample, retaining its original
bounded deadline, failure checks and subsequent terminal-clock assertions.
No production playback code changed. Exact source differences and complete
failed-target continuations are retained in the qualification summary.
