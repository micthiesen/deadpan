# Database 23 / core 17 fixture

`v23-framing-history.sql` was produced on 2026-09-24 by the previously verified native application binary, before the framing implementation. It was not created by downgrading new document version tags.

- Binary: `/tmp/deadpan-master-20260924/native-gui/Deadpan Limited Review.app/Contents/MacOS/deadpan-app`
- Binary SHA-256 verified before execution: `f68cb731edbc6ae1c3e7e4ce416dac7aa158156d61beba80b12d059bcbc3e5b9`
- SQL SHA-256: `1ce0a3dc3bfd03e7d916bafe55c941d907259315c6a9579ccccc4b88871d9251`
- Retained producer, requests, command output and original package: `/tmp/deadpan-framing-20260924/old-binary/`

The binary's `--headless` API created an explicit 16×16, 30000/1001 project, inserted an eight-frame silent Hold, inserted two frames at frame three, renamed the inserted Pause, then performed undo, redo and undo. Its own `project validate` admitted the final package. The final chronology has seven revisions, three edit entries and pending redo. The second insertion retains authored timing bindings.

The SQL is Python sqlite3's dump of the closed package. `application_id` and `user_version` were read from that database and retained as explicit pragmas because `iterdump` omits them. No document, request, edit or operational row was changed. There are no external media dependencies or generation attempts in this fixture; those migration cases remain covered by their separate retained fixtures.

The regression replays every old snapshot and transaction, checks the preserved backup, state/redo and operational metadata, requires only the core version tag to differ in migrated documents, then consumes the pending redo through the current store.
