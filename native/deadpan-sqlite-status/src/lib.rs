//! One SQLite connection's pager cache counters for process diagnostics.
//!
//! rusqlite exposes no safe `sqlite3_db_status`, so this adapter makes the
//! single documented call. It only reads (and resets) per-connection
//! counters: it performs no I/O, never retains the connection handle and
//! changes nothing a query can observe.

use rusqlite::{Connection, ffi};

/// Pager activity since the previous [`take_page_counts`] on a connection
/// (or since it opened), in database pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageCounts {
    /// Page requests satisfied by the connection's page cache.
    pub hits: u64,
    /// Page requests that missed the cache and read the database or WAL.
    pub misses: u64,
    /// Dirty pages written to the database file or WAL.
    pub writes: u64,
    /// Dirty pages written early because the cache was full (also in
    /// `writes`).
    pub spills: u64,
}

/// Read and reset this connection's cache hit, miss, write and spill counts.
/// Summed over the main and attached databases by SQLite.
pub fn take_page_counts(connection: &Connection) -> rusqlite::Result<PageCounts> {
    Ok(PageCounts {
        hits: take(connection, ffi::SQLITE_DBSTATUS_CACHE_HIT)?,
        misses: take(connection, ffi::SQLITE_DBSTATUS_CACHE_MISS)?,
        writes: take(connection, ffi::SQLITE_DBSTATUS_CACHE_WRITE)?,
        spills: take(connection, ffi::SQLITE_DBSTATUS_CACHE_SPILL)?,
    })
}

#[allow(unsafe_code)]
fn take(connection: &Connection, operation: i32) -> rusqlite::Result<u64> {
    let mut current = 0;
    let mut high = 0;
    // SAFETY: the borrowed Connection keeps its handle open and is used on
    // this thread only for this synchronous call (Connection is not Sync).
    // sqlite3_db_status takes the connection mutex, writes two ints into
    // valid local storage and retains no pointer. Reset affects only these
    // diagnostic counters.
    let code = unsafe {
        ffi::sqlite3_db_status(connection.handle(), operation, &mut current, &mut high, 1)
    };
    if code != ffi::SQLITE_OK {
        return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(code), None));
    }
    Ok(u64::try_from(current).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_cache_activity_and_resets_after_each_read() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.sqlite");
        let connection = Connection::open(&path).unwrap();
        take_page_counts(&connection).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE t(x BLOB);
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<200)
                 INSERT INTO t SELECT randomblob(2000) FROM n;",
            )
            .unwrap();
        let written = take_page_counts(&connection).unwrap();
        assert!(written.writes >= 50, "{written:?}");
        // A fresh connection reads the pages it needs from the file.
        drop(connection);
        let connection = Connection::open(&path).unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 200);
        let cold = take_page_counts(&connection).unwrap();
        assert!(cold.misses > 0, "{cold:?}");
        assert_eq!(cold.writes, 0);
        // Repeating the scan is served from the page cache.
        let _: i64 = connection
            .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
            .unwrap();
        let warm = take_page_counts(&connection).unwrap();
        assert!(warm.hits > 0, "{warm:?}");
        assert_eq!(warm.misses, 0, "{warm:?}");
        // Reading reset every counter.
        assert_eq!(
            take_page_counts(&connection).unwrap(),
            PageCounts::default()
        );
    }
}
