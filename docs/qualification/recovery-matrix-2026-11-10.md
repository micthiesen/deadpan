# Recovery, conflict and failure matrix (2026-11-10)

This is the normative acceptance matrix for DP-21.6 ("complete conflicts,
recovery and failure acceptance with private diagnostic output"). It lists
every failure or conflict class that the specification names
([18.3](../spec/DEADPAN_SPEC.md), [20.4 and 20.5](../spec/DEADPAN_SPEC.md#20-projects-history-and-recovery),
[27.3 and 27.4](../spec/DEADPAN_SPEC.md#274-privacy-and-observability),
[28](../spec/DEADPAN_SPEC.md#28-failure-behavior-and-non-negotiable-invariants))
against the three surfaces that can meet it: the native app, the closed-project
headless CLI and the authenticated live endpoint an open app publishes
([live project](../LIVE_PROJECT.md)).

## Expected behaviour in every cell

Every cell must show all four properties; a cell is covered only when a test
asserts the ones that apply to it.

1. **Truthful message.** A stable code from the shared vocabulary
   (`StoreError::code`, `CliError::code`, `HostError`) and a readable
   explanation of what failed and what to do. Never "Saved" after a failed
   transaction.
2. **No write, or an atomic outcome.** The committed head, history, register
   bank and media are either unchanged or completely at the new state.
3. **Recovery offer** where one exists: the same edit saves once the cause
   is gone, a verified backup is named (`recovery_backup`), interrupted
   attempts are listed, or a relink/restore is offered.
4. **Private diagnostics.** No endpoint secret, cookie, authentication header
   or worker log in any output; no absolute path beyond the one the caller
   supplied. The shareable [diagnostic export](../../crates/deadpan-cli/src/diagnostic_export.rs)
   is allowlisted and contains no paths at all.

## Matrix

Legend: test paths are relative to `crates/`; **new** marks a cell this
review added; *owner* marks a physical check on the
[To verify (owner)](../REQUIREMENTS.md#to-verify-owner) list; *n/a* means the
surface cannot meet the class, with the reason.

| Class | Native app | Headless CLI | Live endpoint |
|---|---|---|---|
| Stale revision (`RevisionConflict`) | `deadpan-app/src/project/tests/semantic.rs`, `generation.rs`: captured revision refuses, selection preserved | `deadpan-cli/tests/project_commands.rs`, `headless_parity.rs`, `source_registration/*`: refusal, `current_revision`, no write | `deadpan-cli/tests/live_project.rs::invalid_identity_revision_and_target_preserve_the_document_and_journal`; `deadpan-app/src/project/tests/headless.rs` |
| Concurrent writer | `deadpan-app/src/project/tests/backups.rs::a_second_window_inspects_without_replacing_the_owner_and_reopens_explicitly` | **new** `deadpan-cli/tests/failure_matrix.rs::a_second_writer_is_refused_without_writing_and_the_edit_saves_after_it_closes` (`HostOwnerUnavailable`, no write, saves after the owner closes); `render.rs::public_render_rejects_an_open_writer_without_creating_an_attempt`; store `backups.rs` (`ProjectAlreadyOpen`) | `deadpan-store/tests/host_owner.rs` (replaced/moved owner, unsafe discovery); `deadpan-cli/src/host/tests.rs::lost_reply_after_delivery_is_uncertain_and_not_retried`; `host/server_tests.rs::a_retired_endpoint_answers_only_its_admitted_request_after_the_owner_closes` |
| Read-only open / newer schema | `deadpan-app/src/project/tests/backups.rs::a_package_from_a_newer_deadpan_opens_read_only_and_never_saves`; `readonly.rs` (every package byte unchanged) | `deadpan-cli/tests/backups.rs::a_newer_package_is_viewable_but_never_written`; store `backups.rs::a_newer_schema_opens_read_only_for_viewing_and_is_never_written` | `deadpan-store/tests/host_owner.rs::read_only_store_cannot_obtain_or_use_writer_capabilities` |
| Obsolete schema / migration failure | `backups.rs::opening_a_schema66_package_refuses_without_rewriting_or_backing_it_up` | `deadpan-cli/tests/live_project.rs::migration_of_a_live_current_schema_preserves_the_writer_and_history`; store `chaos_kills.rs::random_process_kills_during_a_migration_leave_the_old_or_the_new_database`, `failpoints.rs` | n/a: migration needs the writer lock, which the live owner holds; refused as concurrent writer |
| Damaged database | `deadpan-app/src/project/tests/damaged.rs` (verified recovery offer, new session) | `project_commands.rs` (`recovery_backup`); store `adversarial.rs::adversarial_corrupted_database_files_fail_cleanly`, `backups.rs::a_database_that_no_longer_opens_is_replaced_by_a_verified_backup` | n/a: an owner with a damaged database cannot publish an endpoint |
| Missing, moved or changed media | `recovery.rs::a_missing_original_opens_degraded_and_relinks_only_identical_bytes`, `backups.rs::a_moved_file_with_different_bytes_is_reported_missing_not_relinked` | `relink_moved.rs`, `original_commands.rs`, `relink_cross_volume.rs`; store `recovery.rs`, `bookmarks.rs` | `deadpan-app/src/project/tests/headless/preparation.rs` (relink through the owner's import worker) |
| Disk full (real `hdiutil` image) | `recovery.rs::a_full_disk_never_reports_saved_and_its_alert_clears_after_a_real_save` | **new** `failure_matrix.rs::a_full_volume_refuses_a_headless_edit_as_disk_full_and_saves_it_once_space_returns` (edit and read both `DiskFull`, head unchanged, same edit saves); `model_pack_import_disk_full.rs`, `proxy_disk_full.rs`; store `storage_failures*.rs` | Same store transaction as the native app; `storage_code` maps the live reply's typed code (`deadpan-app/src/recovery.rs` tests) |
| Permission denied / read-only volume | `storage_errors.rs::denied_open_does_not_mark_current_project_unsaved_or_hide_a_later_save_failure` | **new** `failure_matrix.rs::an_unreadable_database_is_reported_as_permission_denied_without_its_path`; store `storage_failures.rs::permission_denied_refuses_writing_without_changing_the_project`, `a_read_only_volume_refuses_writing_without_changing_the_project` | Same store errors through `LiveError` codes |
| Crash during commit | `recovery.rs::opening_after_an_unclean_exit_reports_it_once_and_keeps_every_saved_edit` | store `recovery.rs::a_killed_writer_is_detected_and_its_committed_edits_and_jobs_are_recovered`, `chaos_kills.rs` (random kills) | Live commits use the same store transaction; `live_project.rs::a_remote_commit_survives_refresh_failure_with_its_exact_revision_receipt` |
| Crash during backup or restore | `damaged.rs::replacement_receipt_survives_a_failure_to_open_the_recovered_project` | store `failpoints.rs` (after publish, inside copy, after copy), `chaos_kills.rs::random_process_kills_during_restores_leave_one_whole_state` | `headless/operations.rs::restores_in_one_batch_or_while_draining_answer_every_client` |
| Crash during render publication | `render.rs::shutdown_finishes_admitted_edit_and_pumps_render_before_releasing_writer`; reopen offer in `recovery.rs` | store `render_jobs/publication.rs::readonly_preserves_active_phase_reopen_interrupts_without_touching_destination`, `reconciliation_preserves_positive_commit_through_unavailable_report_and_reopen` | `live_project.rs::live_render_envelope_preserves_public_render_schema_and_size_checks` |
| Crash during generation | `jobs.rs::interrupted_attempts_are_listed_after_reopen_and_can_be_discarded` | store `generation_attempts.rs::writable_reopen_interrupts_every_nonterminal_state_but_read_only_does_not`, `chaos_kills.rs` | `deadpan-cli/tests/live_generation.rs` |
| Crash during import | `project/tests.rs::shutdown_does_not_wait_for_import_and_rejects_new_commands` | store `original_preparation.rs::database_rollback_keeps_published_bytes_and_the_prepared_token_can_retry`, `prepared_checkpoint.rs` | `deadpan-cli/tests/live_preparation.rs` |
| Worker failure / hostile output | `jobs.rs`, `generation.rs::shutdown_drains_a_running_job` | `hostile_analysis_workers.rs` (frames, stderr, stalls, descendants, escapees); `deadpan-jobs/tests/protocol.rs`, `supervisor.rs`; `render_worker.rs` | `deadpan-app/src/worker/endpoint_tests.rs::endpoint_replacement_cancels_active_work_and_rejects_stale_success_and_failure` |
| Interrupted sleep | [activity and sleep](activity-sleep-2026-10-17.md); store `generation_bundles.rs::sleep_at_final_bundle_commit_rolls_back_ready_and_retains_interrupted`; output device sleep/wake never resumes silently ([audio output](../AUDIO_OUTPUT.md)) | Same store paths as native | n/a: the endpoint carries short requests only |
| Quit during jobs | `jobs.rs::requested_jobs_and_tags_are_listed_for_the_quit_question`, `jobs.rs::shutdown_releases_every_waiter_and_refuses_new_admission`, `project/tests.rs::shutdown_finishes_an_admitted_command_before_releasing_the_store` | `render.rs` (SIGINT/SIGTERM request cancellation and drain) | `host/server_tests.rs` (retired endpoint drained on shutdown) |
| Corrupted caches | `deadpan-audio/src/persistent_stage_tests.rs::persistent_stage_corruption_rebuilds_exact_pcm_and_reports_why` | `proxy_resume.rs::damaged_or_foreign_partial_state_is_encoded_again_never_trusted`, `proxy/cache_tests.rs`; `deadpan-models/src/packs/tests.rs::corrupt_space_starved_and_cancelled_installs_never_activate` | n/a: caches are never reached through the endpoint |
| Bad keymaps / user files | `keymap.rs::a_late_invalid_override_cannot_partly_install_an_earlier_valid_one`; `keymap_file/tests.rs` (size, FIFO, devices, symlinks, partial reads) | n/a: headless paths never read personal keymaps (AGENTS.md); `backup-policy` refuses invalid values | n/a |
| Secrets and paths in diagnostics | Diagnostic export harness (`preview/harness/diagnostics.rs`); launch journal `0600` (`recovery.rs`) | `privacy_sandbox.rs` (export has no paths, names or transcript); `privacy_network_audit.rs::secret_bearing_types_cannot_be_debug_formatted`; `youtube/acquire/tests.rs::downloader_messages_redact_url_queries_and_credentials`; **new** `failure_matrix.rs` asserts no caller directory in any refusal message | `HostError` carries fixed text only (`host.rs`); `host_owner.rs::explicit_registration_is_private_bounded_and_removed_before_unlock` |

## Coverage

Of 57 cells, 6 are *n/a* for the stated reasons. Before this review 48 of the
51 applicable cells had a test; the gaps were the headless concurrent writer
with an owner that publishes no endpoint, headless disk full and headless
permission denied, and no test checked that a refusal message omits the
caller's directory. All 51 applicable cells now have tests.

The physical cells that cannot be exercised here are owner checks: a real
power loss during commit (process kills do not establish power-loss
behaviour) and physical sleep while a render or AI pause runs. Both are on the
To verify (owner) list.

## Defects found and fixed

- A read or write of a project on a full volume reported `ProjectFailure` with
  "unable to open database file" or "disk I/O error": SQLite cannot create its
  shared-memory and write-ahead files even to read. Opening now reports
  `DiskFull` with what to do and that nothing changed (`unopenable_database`
  in `deadpan-store/src/lib.rs`). The native app's storage alert classifies it
  through the same code.
- A project database without read permission reported `ProjectFailure` and
  SQLite's message with its absolute path. It now reports `PermissionDenied`,
  names only `project.sqlite` inside the package and says nothing changed.

## Privacy review

- `diagnostics export` is the shareable artifact: allowlisted structural counts
  and fixed categories, written `0600`, no paths, identifiers, URLs, transcript
  or logs (proved by `privacy_sandbox.rs`).
- `doctor` is local status output, not an export. It prints the absolute
  locations of helpers, models and the running executable because those are
  what the user needs to find, and `cargo xtask bundle-verify` checks them
  against the bundle. It prints no endpoint secret, cookie or header.
- Live endpoint errors are fixed strings; secret-bearing types cannot be
  `Debug`-formatted. Cookies are copied `0600`, deleted after use and never
  stored; downloader messages redact URL queries and credentials.
- Structured CLI errors name only the path the caller passed (for example a
  read-only volume names the package) or a file inside the package.
