# Self-review

The primary agent implemented and reviewed this increment alone. No independent reviewer, live GUI, device output or listening check ran.

- The split-owner exception accepts only the Repeat created by this command, with no preexisting external Repeat ancestor. Every alias still passes the full historical/current processing correspondence and exact recorded original-owner check. Existing ancestor and changed-subtree guards remain.
- Initial plays use the existing Introduced cohort map. Subsequent count growth has fresh iteration identities; shrinking and regrowing cannot inherit retired play clocks. No new clock format or DSP path was added.
- Repeat's additional capture allocator now skips the command base ordinal even if the physical recipe capture omitted it. Sound restoration owns that pre-edit identity. Later interior/suffix captures still skip every retained ordinal and use checked increment. Exhaustion refuses before committing.
- The core regression binds every physical recipe to reproduce the formerly colliding suffix layout. It verifies the command base table against the independently captured pre-edit tree, inverse/JSON behavior, initial applicability and later birth exclusion.
- Audio oracles render the old document separately. Tests cover dry Source, event plus owner EQ, dry nested Preserve, successive count/gap changes, and copy/recopy. A new play is compared with an independently unbound current clock; no PCM tolerance was widened. Gap comparisons preserve current silence/edge policies.
- The real-media test uses existing WAV/MP4 fixtures, pre-edit nonzero PCM, persisted commands and fresh-revision Undo/Redo. Its 13-picture export has zero measured audio offset but one failing AAC block. The test explicitly preserves that failure and checks the sole flag instead of claiming a passing export or weakening thresholds.
- Adding EQ to the nested Preserve fixture fails the pre-Repeat reference render with prepared stages per read. Production DSP did not change. The failing logs, backtrace and exact one-line reproduction patch are retained for the next preparation-cache fix; this combination is not qualified here.
