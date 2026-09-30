use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::Path,
};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::Result;

const MAX_REPORT: usize = 32 * 1024 * 1024;
const MAX_ARTIFACT: u64 = 512 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Authoring {
    pub(super) sha256: String,
    pub(super) bytes: u64,
    rows: BTreeMap<String, Vec<String>>,
}
impl Authoring {
    pub(super) fn summary(&self) -> Value {
        json!({"sha256": self.sha256, "bytes": self.bytes,
            "table_rows": self.rows.iter().map(|(name, rows)| (name, rows.len())).collect::<BTreeMap<_, _>>()})
    }
}

/// One consistent SQLite read transaction captures exact authored/history cells,
/// including historical document/patch JSON as retained text.
pub(super) fn authored(package: &Path) -> Result<Authoring> {
    let mut connection = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let transaction = connection.transaction()?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut captured = BTreeMap::new();
    for (name, query) in [
        (
            "revisions",
            "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        ),
        (
            "history",
            "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        ),
        (
            "state",
            "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        ),
        (
            "redo",
            "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        ),
    ] {
        hasher.update(query.as_bytes());
        let mut statement = transaction.prepare(query)?;
        let mut rows = statement.query([])?;
        let mut values = Vec::new();
        while let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let length = u64::try_from(value.len())?;
            bytes = bytes.checked_add(length).ok_or("authoring byte overflow")?;
            if bytes > 16 * 1024 * 1024 || values.len() >= 4096 {
                return Err("authoring fixture bound exceeded".into());
            }
            hasher.update(length.to_le_bytes());
            hasher.update(value.as_bytes());
            values.push(value);
        }
        captured.insert(name.to_owned(), values);
    }
    Ok(Authoring {
        sha256: hex(hasher.finalize()),
        bytes,
        rows: captured,
    })
}

pub(super) fn check(report: &mut Value, label: &str, passed: bool, actual: Value) -> Result {
    report["checks"]
        .as_array_mut()
        .ok_or("missing checks")?
        .push(json!({"label": label, "passed": passed, "actual": actual}));
    if !passed {
        return Err(format!("qualification failed: {label}").into());
    }
    Ok(())
}

pub(super) fn save(file: &mut File, value: &impl Serialize) -> Result {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_REPORT.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other("qualification JSON bound exceeded"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Bounded(Vec::new());
    serde_json::to_writer_pretty(&mut bytes, value)?;
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&bytes.0)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn save_new(path: &Path, value: &impl Serialize) -> Result {
    save(
        &mut File::options().create_new(true).write(true).open(path)?,
        value,
    )
}

pub(super) fn read_json(path: &Path) -> Result<Value> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_REPORT as u64 {
        return Err("qualification JSON bound exceeded".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_REPORT as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_REPORT {
        return Err("qualification JSON grew past bound".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn artifacts(directory: &Path) -> Result<Value> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF8 qualification artifact")?;
        if !name.ends_with(".mp4")
            && !name.ends_with(".partial")
            && !name.starts_with("deadpan-render-")
            && name != "displaced-report.json"
        {
            continue;
        }
        if entries.len() >= 16 {
            return Err("qualification artifact count bound exceeded".into());
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_ARTIFACT {
            return Err("unexpected qualification artifact type or extent".into());
        }
        let mut file = File::open(entry.path())?;
        let before = file.metadata()?;
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(count)?)
                .ok_or("artifact extent overflow")?;
            if total > MAX_ARTIFACT {
                return Err("qualification artifact grew past bound".into());
            }
            hasher.update(&buffer[..count]);
        }
        let after = file.metadata()?;
        let before = metadata_value(&before);
        let after = metadata_value(&after);
        if before != after || total != metadata.len() {
            return Err("artifact changed during evidence read".into());
        }
        entries.insert(name, json!({"path": entry.path(), "sha256": hex(hasher.finalize()), "bytes": total, "metadata": after}));
    }
    Ok(json!(entries))
}

fn metadata_value(value: &fs::Metadata) -> Value {
    use std::os::darwin::fs::MetadataExt as Darwin;
    json!({"device": value.dev(), "inode": value.ino(), "owner": value.uid(), "group": value.gid(),
        "mode": value.mode(), "links": value.nlink(), "bytes": value.len(),
        "mtime_seconds": value.mtime(), "mtime_nanoseconds": value.mtime_nsec(),
        "ctime_seconds": value.ctime(), "ctime_nanoseconds": value.ctime_nsec(),
        "birth_seconds": Darwin::st_birthtime(value), "birth_nanoseconds": Darwin::st_birthtime_nsec(value),
        "generation": Darwin::st_gen(value), "flags": Darwin::st_flags(value)})
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
