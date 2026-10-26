//! Closed-tree AI runtime inventory shared by the host and bundle builder.
//! No resource manifest supplies its own authority; the host pins these bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, open, openat, readlinkat_raw, statat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_ENTRIES: usize = 32_768;
pub const MAX_DEPTH: usize = 64;
pub const MAX_PATH_BYTES: usize = 1024;
pub const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub schema: u32,
    pub root_mode: u32,
    #[serde(deserialize_with = "unique_entries")]
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Directory {
        mode: u32,
    },
    File {
        mode: u32,
        bytes: u64,
        sha256: String,
    },
    Macho {
        mode: u32,
        content_sha256: String,
    },
    Symlink {
        target: String,
    },
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn unique_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Entry>, D::Error> {
    struct Unique;
    impl<'de> serde::de::Visitor<'de> for Unique {
        type Value = BTreeMap<String, Entry>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique bounded inventory paths")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((path, entry)) = map.next_entry::<String, Entry>()? {
                if entries.len() >= MAX_ENTRIES || entries.insert(path, entry).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive inventory entries",
                    ));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Unique)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn private_mode(mode: u32) -> bool {
    mode <= 0o777 && mode & 0o022 == 0
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && !path.contains('\0')
        && path.split('/').count() <= MAX_DEPTH
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn link_target(path: &str, target: &str) -> io::Result<String> {
    if target.is_empty()
        || target.len() > MAX_PATH_BYTES
        || target.starts_with('/')
        || target.contains('\0')
    {
        return Err(invalid("invalid inventory symlink target"));
    }
    let mut parts = path.split('/').collect::<Vec<_>>();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => return Err(invalid("noncanonical inventory symlink target")),
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| invalid("inventory symlink escapes its root"))?;
            }
            part => parts.push(part),
        }
    }
    let target = parts.join("/");
    if !valid_path(&target) {
        return Err(invalid("invalid resolved inventory symlink"));
    }
    Ok(target)
}

fn validate(inventory: &Inventory) -> io::Result<()> {
    validate_with(inventory, || Ok(()))
}

fn validate_with(
    inventory: &Inventory,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    check()?;
    if inventory.schema != 1
        || !private_mode(inventory.root_mode)
        || inventory.entries.len() > MAX_ENTRIES
    {
        return Err(invalid("unsupported or excessive AI inventory"));
    }
    let mut total = 0u64;
    for (path, entry) in &inventory.entries {
        check()?;
        if !valid_path(path) {
            return Err(invalid("noncanonical inventory path"));
        }
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if !matches!(inventory.entries.get(prefix), Some(Entry::Directory { .. })) {
                return Err(invalid("inventory path has an absent or aliased parent"));
            }
            parent = prefix;
        }
        match entry {
            Entry::Directory { mode } | Entry::Macho { mode, .. } | Entry::File { mode, .. }
                if !private_mode(*mode) =>
            {
                return Err(invalid("shared-writable or special inventory mode"));
            }
            Entry::File { bytes, sha256, .. } => {
                if *bytes > MAX_FILE_BYTES || !valid_hash(sha256) {
                    return Err(invalid("invalid inventory file bound/hash"));
                }
                total = total
                    .checked_add(*bytes)
                    .ok_or_else(|| invalid("inventory byte overflow"))?;
            }
            Entry::Macho { content_sha256, .. } if !valid_hash(content_sha256) => {
                return Err(invalid("invalid inventory Mach-O hash"));
            }
            _ => {}
        }
        if total > MAX_TOTAL_BYTES {
            return Err(invalid("inventory total exceeds its bound"));
        }
        if let Entry::Symlink { target } = entry {
            let mut target = link_target(path, target)?;
            let mut seen = BTreeSet::new();
            loop {
                check()?;
                if seen.len() >= MAX_DEPTH || !seen.insert(target.clone()) {
                    return Err(invalid("cyclic or excessive inventory symlink chain"));
                }
                match inventory.entries.get(&target) {
                    Some(Entry::File { .. } | Entry::Macho { .. }) => break,
                    Some(Entry::Symlink { target: next }) => target = link_target(&target, next)?,
                    _ => {
                        return Err(invalid(
                            "inventory symlink is dangling or aliases a directory",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn canonical_bytes(inventory: &Inventory) -> io::Result<Vec<u8>> {
    validate(inventory)?;
    let bytes = serde_json::to_vec(inventory).map_err(invalid)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(invalid("AI inventory serialization exceeds its bound"));
    }
    Ok(bytes)
}

pub fn parse(bytes: &[u8]) -> io::Result<Inventory> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(invalid("AI inventory exceeds its byte bound"));
    }
    let inventory: Inventory = serde_json::from_slice(bytes).map_err(invalid)?;
    if canonical_bytes(&inventory)? != bytes {
        return Err(invalid("AI inventory is not canonical"));
    }
    Ok(inventory)
}

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn flags(directory: bool) -> OFlags {
    let flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    if directory {
        flags | OFlags::DIRECTORY
    } else {
        flags
    }
}

fn private(metadata: &Metadata) -> io::Result<u32> {
    let mode = metadata.mode() & 0o7777;
    let user = rustix::process::getuid().as_raw();
    if (metadata.uid() != user && metadata.uid() != 0) || !private_mode(mode) {
        return Err(invalid(
            "AI runtime entry is not privately owned and writable",
        ));
    }
    Ok(mode)
}

fn same(before: &Metadata, after: &Metadata) -> bool {
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.mode() == after.mode()
        && before.uid() == after.uid()
        && before.gid() == after.gid()
        && before.len() == after.len()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
}

fn is_macho(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some(
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xce]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
                | [0xca, 0xfe, 0xba, 0xbf]
                | [0xbf, 0xba, 0xfe, 0xca]
        )
    )
}

struct Collector<C, H> {
    check: C,
    macho_hash: H,
    entries: BTreeMap<String, Entry>,
    total: u64,
}

impl<C: FnMut() -> io::Result<()>, H: FnMut(&[u8]) -> io::Result<String>> Collector<C, H> {
    fn file(&mut self, mut file: File) -> io::Result<Entry> {
        (self.check)()?;
        let before = file.metadata()?;
        let mode = private(&before)?;
        if !before.is_file() || before.len() > MAX_FILE_BYTES {
            return Err(invalid("AI runtime file exceeds its bound or changed type"));
        }
        self.total = self
            .total
            .checked_add(before.len())
            .ok_or_else(|| invalid("AI runtime byte overflow"))?;
        if self.total > MAX_TOTAL_BYTES {
            return Err(invalid("AI runtime total exceeds its byte bound"));
        }
        let mut chunk = [0; 64 * 1024];
        let mut prefix = [0; 4];
        let mut prefix_bytes = 0;
        while prefix_bytes < prefix.len() {
            (self.check)()?;
            let count = file.read(&mut prefix[prefix_bytes..])?;
            (self.check)()?;
            if count == 0 {
                break;
            }
            prefix_bytes += count;
        }
        let mut content = Vec::new();
        let mut hasher = Sha256::new();
        let mut read = prefix_bytes as u64;
        let macho = is_macho(&prefix[..prefix_bytes]);
        if macho {
            content.extend_from_slice(&prefix[..prefix_bytes]);
        } else {
            hasher.update(&prefix[..prefix_bytes]);
        }
        loop {
            (self.check)()?;
            let count = file.read(&mut chunk)?;
            (self.check)()?;
            if count == 0 {
                break;
            }
            read = read
                .checked_add(count as u64)
                .ok_or_else(|| invalid("AI runtime read overflow"))?;
            if read > before.len() {
                return Err(invalid("AI runtime file grew while reading"));
            }
            if macho {
                content.extend_from_slice(&chunk[..count]);
            } else {
                hasher.update(&chunk[..count]);
            }
        }
        if read != before.len() || !same(&before, &file.metadata()?) {
            return Err(invalid("AI runtime file changed while reading"));
        }
        if macho {
            let content_sha256 = (self.macho_hash)(&content)?;
            (self.check)()?;
            Ok(Entry::Macho {
                mode,
                content_sha256,
            })
        } else {
            Ok(Entry::File {
                mode,
                bytes: read,
                sha256: hasher
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            })
        }
    }

    fn directory(&mut self, directory: &File, relative: &str) -> io::Result<()> {
        (self.check)()?;
        let before = directory.metadata()?;
        private(&before)?;
        for entry in Dir::read_from(directory)? {
            (self.check)()?;
            let entry = entry?;
            let name = entry.file_name().to_str().map_err(invalid)?;
            if name == "." || name == ".." {
                continue;
            }
            if self.entries.len() >= MAX_ENTRIES {
                return Err(invalid("AI runtime entry count exceeds its bound"));
            }
            let path = if relative.is_empty() {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            };
            if !valid_path(&path) {
                return Err(invalid("AI runtime path exceeds its bound"));
            }
            let state = statat(directory, name, AtFlags::SYMLINK_NOFOLLOW)?;
            let value = match FileType::from_raw_mode(state.st_mode) {
                FileType::Directory => {
                    let child = File::from(openat(directory, name, flags(true), Mode::empty())?);
                    let mode = private(&child.metadata()?)?;
                    if self
                        .entries
                        .insert(path.clone(), Entry::Directory { mode })
                        .is_some()
                    {
                        return Err(invalid("AI runtime directory repeated a name"));
                    }
                    self.directory(&child, &path)?;
                    continue;
                }
                FileType::RegularFile => self.file(File::from(openat(
                    directory,
                    name,
                    flags(false),
                    Mode::empty(),
                )?))?,
                FileType::Symlink => {
                    let user = rustix::process::getuid().as_raw();
                    if state.st_uid != user && state.st_uid != 0 {
                        return Err(invalid("AI runtime symlink has a foreign owner"));
                    }
                    let mut target = [0; MAX_PATH_BYTES + 1];
                    let length = readlinkat_raw(directory, name, &mut target[..])?;
                    if length > MAX_PATH_BYTES {
                        return Err(invalid("AI runtime symlink target exceeds its bound"));
                    }
                    let target = std::str::from_utf8(&target[..length])
                        .map_err(invalid)?
                        .to_owned();
                    let after = statat(directory, name, AtFlags::SYMLINK_NOFOLLOW)?;
                    if state.st_dev != after.st_dev
                        || state.st_ino != after.st_ino
                        || state.st_mode != after.st_mode
                        || state.st_uid != after.st_uid
                        || state.st_mtime != after.st_mtime
                        || state.st_mtime_nsec != after.st_mtime_nsec
                    {
                        return Err(invalid("AI runtime symlink changed while reading"));
                    }
                    Entry::Symlink { target }
                }
                _ => return Err(invalid("AI runtime contains a special file")),
            };
            if self.entries.insert(path, value).is_some() {
                return Err(invalid("AI runtime directory repeated a name"));
            }
        }
        if !same(&before, &directory.metadata()?) {
            return Err(invalid("AI runtime directory changed while scanning"));
        }
        (self.check)()
    }
}

/// Collect a bounded, non-following descriptor-relative snapshot. The caller
/// supplies the strict signature-independent Mach-O hash (or an exact-byte
/// hash when fingerprinting a build input), and cooperative interruption.
pub fn collect(
    root: &Path,
    mut check: impl FnMut() -> io::Result<()>,
    macho_hash: impl FnMut(&[u8]) -> io::Result<String>,
) -> io::Result<Inventory> {
    check()?;
    let directory = File::from(open(root, flags(true), Mode::empty())?);
    let root_mode = private(&directory.metadata()?)?;
    let mut collector = Collector {
        check,
        macho_hash,
        entries: BTreeMap::new(),
        total: 0,
    };
    collector.directory(&directory, "")?;
    let inventory = Inventory {
        schema: 1,
        root_mode,
        entries: collector.entries,
    };
    validate_with(&inventory, &mut collector.check)?;
    Ok(inventory)
}

#[cfg(test)]
#[path = "ai_inventory/tests.rs"]
mod tests;
