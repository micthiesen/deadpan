# Publication recovery review disposition

The parent reviewed the integrated shared staging, verifier attempt binding,
store phases/barriers, recovery ownership and native harness. A fresh independent
review covered durability ordering, crash knowledge, revocation, migration and
filesystem identity. It returned no findings after checking one candidate concern.

The proposed concern was that a trusted host can declare Failed/Cancelled after
MovieCommitting. This remains necessary for a known failure before rename, such as
NOREPLACE refusing an occupied destination. The production host returns a failure
only before a successful movie rename and preserves every later failure as
PublishedUnconfirmed. Lost results remain active and become Interrupted on reopen;
reconciliation cannot declare MovieCommitting unpublished. No in-repo caller was
found violating this contract. No source change was warranted for that concern.

The parent corrected build/test issues found during verification: checked SQLite
bound conversions; foreign-key disabling solely while importing an authentic
SQLite dump in table order; changed-mode recovery error classification; preserving
nested error causes through io::Error; and canonical-path test comparison.
Standalone staging retains its previous random-name retry loop. Old unguarded
filesystem commit helpers are now test-only; production always uses the guard.

A separate review of the user's visual slice spec found ambiguous ripple wording
for picture/audio-only scope. The committed spec now applies ripple duration to
linked insertion and points role-only placement to existing attachment policies.

No GUI changes or physical power-loss guarantees are inferred. Native evidence
qualifies the tested APFS/process-death boundary, and all release requirements
remain open or partial.
