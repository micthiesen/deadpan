# Self-review

The primary agent implemented and reviewed this increment alone.

- Split identity maps are recorded directly from the existing structural
  reducer. No equality search, label matching or inferred lineage supplies
  mutation authority. Each event also checks its exact owner in the paired
  processing proof, including when another event populated the scope cache.
- Successive copies map back to the pre-command owner. Restoring only surviving
  aliases handles both retained-left and deleted-original cases. Old journals
  are borrowed per original owner, so restoring one alias cannot consume the
  history needed by another. Imported sound maps are retained and collisions
  reject. All mutation stays in a private candidate until atomic publication.
- The alias expansion is bounded by the existing 64-event limit. Processing
  and crop comparisons retain the existing work budget. The bound also covers
  temporary aliases that a later deletion might remove.
- Narrowed scopes still include non-Partition Retime processors. External
  Repeat ancestry refuses explicitly; unchanged processing correspondence and
  exact owner checks remain required. Unrecorded crop changes still refuse.
- A split left fragment records its old full allocation even without movement.
  Delivery crops cannot erase the complete input required by existing EQ.
  Tests cover dry, event/shared EQ, Original plus attachment, silence, deletion,
  moves, arbitrary reads, retained earlier clocks and fresh-revision Undo/Redo.
- The first PCM oracle incorrectly replaced two integral sample shifts with a
  single frame-boundary difference. The correction derives both shifts from
  the authored insertion points, with no event alignment or tolerance change.
  The first real-media oracle selected a silent window after the short WAV;
  its cuts were moved earlier so every equality witness contains real signal.
- A forced SQLite history failure leaves state and row counts unchanged.
  Preview/commit equality, reopen and Undo/Redo exercise the persisted result.
  The old/new binary comparison independently demonstrates the former refusal.
- No schema or DSP processing changes are introduced. Broader Repeat/root-owner
  and enclosing-subtree transformations remain guarded. The export is timing
  and decoded-file evidence; it does not establish acoustic quality.
