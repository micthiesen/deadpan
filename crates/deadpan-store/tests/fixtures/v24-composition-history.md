# Database-24 composition history

`v24-composition-history.sql` was produced on 2026-09-24 by the actual pre-change
Deadpan executable built from commit `c03a5edde5f28d27074745eb15711cb28b1f2e50`.
The producer used its headless create, edit, undo, redo and validation commands;
the fixture was not created by relabeling current-schema JSON.

- Executable SHA-256: `4df29f2d65ba76d2cdcf88f4c361a38d416a224270aeae26b3d297052896036e`
- SQL SHA-256: `7429704f56c886610decc265d100fec07cbfb973455807084534b0fa0a47f3e2`
- Core schema: 18. Database schema: 24.
- Eight immutable revisions and four history entries, including a framed Hold,
  an authored Camera curve, InsertTime, undo/redo and pending redo.
- Final old revision: `5700ab26-76a7-4c9f-ae02-d6728f058aff`.

The migration test compares every snapshot and transaction through the frozen
core-18 adapter, the exact backup, operational metadata and pending redo. It then
persists a new captured context, undoes, redoes and reopens the migrated store.
