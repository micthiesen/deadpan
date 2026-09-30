# Independent coordinator and native lifecycle review

No actionable findings from source review of the current workflow owner,
worker, native ProjectService adapter and preview shutdown changes.

Reviewed: encoded_render/workflow.rs, workflow/{owner,worker,types,tests}.rs;
deadpan-app project/service/render.rs, project/service.rs, project.rs,
project/tests/render.rs; preview.rs shutdown changes; the render workflow lease
and exact writer binding in deadpan-store/render_media.rs.

Checked invariants:

- Cancellation persists against the exact current attempt/publication before
  the atomic worker signal. A journal failure still signals stop and records
  uncertainty.
- A committed movie reply is handled before cancellation, and its receipt and
  positive commit observation survive a failed terminal database write.
- Worker-owned candidates and recovered destination locks remain alive through
  their owner transitions, then are released on the worker before the service
  can replace or close its writer.
- Unknown process cleanup, worker disconnection and mismatched stage replies
  retain the execution lease. A mere Release reply cannot upgrade earlier
  unresolved teardown.
- Native start checks session, project and current revision. Retry/reconciliation
  retain historical intent. Cancellation requires the exact workflow identity.
- Open prepares a valid replacement before cancelling the previous render;
  installation waits for checked release. Failed Open leaves its render live.
- Native service shutdown completes an admitted user command and polls render
  completion before releasing the writer. Preview waits for shutdown completion.
- The media worker owns no writable SQLite connection. Commands and replies
  have capacity one; progress is a replaceable optional slot.

Limits: no build, tests, formatter or native execution ran in this reviewer.
The teardown implementation authored by this same agent was excluded from this
independent review. Its separate held-pump Drop correction was implemented at
the parent's explicit request and still requires parent execution and review.
