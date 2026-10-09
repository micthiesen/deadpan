use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_store::{ProjectStore, render_jobs::StoredRenderCheckpoint};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Result, media_limits};

const MAX_JSON: u64 = 32 * 1024 * 1024;
const MAX_EXECUTABLE: u64 = 1024 * 1024 * 1024;
const MAX_AUTHORED_BYTES: u64 = 16 * 1024 * 1024;
const MAX_AUTHORED_ROWS: u64 = 4096;

pub(super) fn save(path: &Path, value: &impl Serialize) -> Result {
    let bytes = serde_json::to_vec_pretty(value)?;
    if u64::try_from(bytes.len())? > MAX_JSON {
        return Err("qualification JSON bound exceeded".into());
    }
    // Readers see either the previous complete receipt or the next complete
    // receipt. A partially written witness can never authorize the kill.
    let mut temporary = tempfile::NamedTempFile::new_in(path.parent().ok_or("no parent")?)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    File::open(path.parent().ok_or("no parent")?)?.sync_all()?;
    Ok(())
}

pub(super) fn read(path: &Path) -> Result<Value> {
    read_bounded(path, MAX_JSON)
}

pub(super) fn read_bounded(path: &Path, maximum_bytes: u64) -> Result<Value> {
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?);
    if !file.metadata()?.is_file() || file.metadata()?.len() > maximum_bytes {
        return Err("invalid qualification receipt extent".into());
    }
    let mut bytes = Vec::new();
    file.take(
        maximum_bytes
            .checked_add(1)
            .ok_or("receipt bound overflow")?,
    )
    .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())? > maximum_bytes {
        return Err("qualification receipt grew past its bound".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn hash(path: &Path) -> Result<Value> {
    use std::os::unix::fs::MetadataExt;
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > MAX_EXECUTABLE {
        return Err("qualification hash input exceeds its bound".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(u64::try_from(count)?)
            .ok_or("hash overflow")?;
        if bytes > MAX_EXECUTABLE {
            return Err("hash input grew past its bound".into());
        }
        digest.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if bytes != before.len()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err("hash input changed while being read".into());
    }
    Ok(json!({"path": path, "bytes": bytes, "sha256": hex(digest.finalize())}))
}

pub(super) fn authored(package: &Path) -> Result<Value> {
    let mut database = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let transaction = database.transaction()?;
    authored_snapshot(&transaction)
}

fn authored_snapshot(transaction: &rusqlite::Transaction<'_>) -> Result<Value> {
    const TABLES: [(&str, &str, &str); 4] = [
        ("revisions", "id,parent_id,kind,document", "id"),
        ("history", "id,parent_id,revision_id,request,edit", "id"),
        (
            "state",
            "singleton,head_revision,cursor,workflow",
            "singleton",
        ),
        ("redo", "position,history_id", "position"),
    ];
    // The first read establishes the transaction's SQLite snapshot. Preflight
    // and collection see those same cells even if another writer commits.
    // Check raw fields before constructing JSON, which may expand escaped text.
    let mut rows_total = 0_u64;
    let mut raw_bytes = 0_u64;
    for (name, columns, _) in TABLES {
        let lengths = columns
            .split(',')
            .map(|column| format!("COALESCE(length(CAST({column} AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join("+");
        let sql = format!(
            "SELECT COUNT(*),COALESCE(SUM({lengths}),0) FROM \
             (SELECT {columns} FROM {name} LIMIT {})",
            MAX_AUTHORED_ROWS + 1
        );
        let (rows, bytes): (i64, i64) =
            transaction.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let rows = u64::try_from(rows)?;
        let bytes = u64::try_from(bytes)?;
        rows_total = rows_total
            .checked_add(rows)
            .ok_or("authoring row overflow")?;
        raw_bytes = raw_bytes
            .checked_add(bytes)
            .ok_or("authoring size overflow")?;
        if rows_total > MAX_AUTHORED_ROWS || raw_bytes > MAX_AUTHORED_BYTES {
            return Err("qualification authoring fixture bound exceeded".into());
        }
    }
    let mut json_bytes = 0_u64;
    for (name, columns, _) in TABLES {
        let bytes: i64 = transaction.query_row(
            &format!(
                "SELECT COALESCE(SUM(length(CAST(json_array({columns}) AS BLOB))),0) FROM {name}"
            ),
            [],
            |row| row.get(0),
        )?;
        let bytes = u64::try_from(bytes)?;
        json_bytes = json_bytes
            .checked_add(bytes)
            .ok_or("authoring size overflow")?;
        if json_bytes > MAX_AUTHORED_BYTES {
            return Err("qualification authoring fixture bound exceeded".into());
        }
    }
    let mut tables = BTreeMap::new();
    for (name, columns, order) in TABLES {
        let mut statement = transaction.prepare(&format!(
            "SELECT json_array({columns}) FROM {name} ORDER BY {order}"
        ))?;
        let mut rows = statement.query([])?;
        let mut values = Vec::new();
        while let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            values.push(value);
        }
        tables.insert(name, values);
    }
    Ok(json!(tables))
}

pub(super) fn checkpoint(
    store: &ProjectStore,
    checkpoint: &StoredRenderCheckpoint,
) -> Result<Value> {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(60);
    let snapshot = store.render_read_handle().snapshot(
        &checkpoint.media,
        media_limits()?,
        &cancelled,
        deadline,
    )?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut offset = 0_u64;
    while offset < checkpoint.media.movie().byte_length() {
        let count = snapshot.read_at(offset, &mut buffer, &cancelled, deadline)?;
        if count == 0 {
            return Err("checkpoint ended early".into());
        }
        digest.update(&buffer[..count]);
        offset = offset
            .checked_add(u64::try_from(count)?)
            .ok_or("checkpoint size overflow")?;
    }
    let movie_hash = hex(digest.finalize());
    let manifest_hash = hex(Sha256::digest(snapshot.manifest_bytes()));
    if movie_hash != checkpoint.media.movie_sha256().as_str()
        || manifest_hash != checkpoint.media.manifest_sha256().as_str()
    {
        return Err("independent checkpoint hash differs from stored identity".into());
    }
    Ok(json!({"checkpoint": checkpoint, "movie_sha256": movie_hash,
        "manifest_sha256": manifest_hash, "movie_bytes": offset}))
}

pub(super) fn worker_receipts(directory: &Path) -> Result<Vec<Value>> {
    let mut receipts = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF8 evidence filename")?;
        if name.starts_with("worker-") && name.ends_with(".json") {
            if receipts.len() >= 16 {
                return Err("worker receipt count exceeded".into());
            }
            receipts.insert(name.to_owned(), read(&entry.path())?);
        }
    }
    Ok(receipts.into_values().collect())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let database = Connection::open_in_memory().expect("database");
        database
            .execute_batch(
                "CREATE TABLE revisions(id INTEGER PRIMARY KEY,parent_id,kind,document);\
             CREATE TABLE history(id INTEGER PRIMARY KEY,parent_id,revision_id,request,edit);\
             CREATE TABLE state(singleton INTEGER PRIMARY KEY,head_revision,cursor,workflow);\
             CREATE TABLE redo(position INTEGER PRIMARY KEY,history_id);",
            )
            .expect("tables");
        database
    }

    fn rejects_bound(database: &mut Connection) {
        let transaction = database.transaction().expect("snapshot");
        let error = authored_snapshot(&transaction).expect_err("bounded fixture refusal");
        assert_eq!(
            error.to_string(),
            "qualification authoring fixture bound exceeded"
        );
    }

    #[test]
    fn authored_preflight_rejects_an_oversized_row_before_json_conversion() {
        let mut database = database();
        // A BLOB cannot enter json_array. The size refusal proves preflight
        // happens before even SQLite attempts that row's JSON conversion.
        database
            .execute(
                "INSERT INTO revisions(id,document) VALUES(1,zeroblob(?1))",
                [i64::try_from(MAX_AUTHORED_BYTES + 1).expect("fixture bytes fit SQLite")],
            )
            .expect("oversized row");
        rejects_bound(&mut database);
    }

    #[test]
    fn authored_preflight_checks_aggregate_bytes_across_tables() {
        let mut database = database();
        for sql in [
            "INSERT INTO revisions(id,document) VALUES(1,CAST(zeroblob(?1) AS TEXT))",
            "INSERT INTO history(id,request) VALUES(1,CAST(zeroblob(?1) AS TEXT))",
        ] {
            database
                .execute(
                    sql,
                    [
                        i64::try_from(MAX_AUTHORED_BYTES / 2 + 1)
                            .expect("fixture bytes fit SQLite"),
                    ],
                )
                .expect("large row");
        }
        rejects_bound(&mut database);
    }

    #[test]
    fn authored_preflight_checks_json_escaping_before_collecting_strings() {
        let mut database = database();
        database
            .execute(
                "INSERT INTO revisions(id,document) VALUES(1,CAST(zeroblob(?1) AS TEXT))",
                [i64::try_from(MAX_AUTHORED_BYTES / 6 + 1).expect("fixture bytes fit SQLite")],
            )
            .expect("escaped row");
        rejects_bound(&mut database);
    }

    #[test]
    fn authored_preflight_checks_total_rows_across_tables_and_retains_small_fixtures() {
        let mut database = database();
        database
            .execute(
                "WITH RECURSIVE ids(id) AS (SELECT 1 UNION ALL SELECT id+1 FROM ids WHERE id<?1) \
             INSERT INTO revisions(id) SELECT id FROM ids",
                [i64::try_from(MAX_AUTHORED_ROWS).expect("fixture rows fit SQLite")],
            )
            .expect("row limit");
        {
            let transaction = database.transaction().expect("snapshot");
            let snapshot = authored_snapshot(&transaction).expect("bounded rows");
            assert_eq!(snapshot["revisions"].as_array().expect("rows").len(), 4096);
            assert_eq!(snapshot["revisions"][0], "[1,null,null,null]");
        }
        database
            .execute("INSERT INTO state(singleton) VALUES(1)", [])
            .expect("extra table row");
        rejects_bound(&mut database);
    }
}
