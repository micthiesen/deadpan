//! Restart inspection only. Recorded metadata plus a fresh complete hash can
//! identify cooperative publication remnants; it cannot defeat a malicious
//! same-user process. These handles offer no rename, unlink, or resume method.

use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableIdentity {
    volume_uuid: [u8; 16],
    device: u64,
    inode: u64,
    birth_seconds: i64,
    birth_nanoseconds: i64,
    generation: Option<u32>,
    owner: u32,
    group: u32,
    mode: u32,
    flags: u32,
}

impl DurableIdentity {
    fn validate(&self) -> Result<()> {
        if self.volume_uuid == [0; 16]
            || self.inode == 0
            || (self.birth_seconds == 0 && self.birth_nanoseconds == 0)
            || !(0..1_000_000_000).contains(&self.birth_nanoseconds)
            || self.generation == Some(0)
        {
            return Err(FsError::invalid(
                "invalid_recovery_evidence",
                "invalid filesystem identity",
            ));
        }
        Ok(())
    }

    fn file(file: &File) -> Result<Self> {
        let before = file
            .metadata()
            .map_err(|e| FsError::new("destination_io", "inspect identity descriptor", e))?;
        let volume_uuid = deadpan_filesystem::apfs_volume_uuid(file).map_err(|e| {
            FsError::new(
                "unsupported_recovery_identity",
                "qualify APFS volume identity",
                e,
            )
        })?;
        let identity = Self::metadata(&before, volume_uuid)?;
        let after = file
            .metadata()
            .map_err(|e| FsError::new("destination_io", "recheck identity descriptor", e))?;
        if identity != Self::metadata(&after, volume_uuid)? {
            return Err(FsError::invalid(
                "destination_changed",
                "descriptor identity changed while captured",
            ));
        }
        Ok(identity)
    }

    #[cfg(target_os = "macos")]
    fn metadata(metadata: &Metadata, volume_uuid: [u8; 16]) -> Result<Self> {
        use std::os::darwin::fs::MetadataExt as MacMetadataExt;
        let generation = MacMetadataExt::st_gen(metadata);
        let identity = Self {
            volume_uuid,
            device: metadata.dev(),
            inode: metadata.ino(),
            birth_seconds: MacMetadataExt::st_birthtime(metadata),
            birth_nanoseconds: MacMetadataExt::st_birthtime_nsec(metadata),
            generation: (generation != 0).then_some(generation),
            owner: metadata.uid(),
            group: metadata.gid(),
            mode: metadata.mode(),
            flags: MacMetadataExt::st_flags(metadata),
        };
        identity.validate()?;
        Ok(identity)
    }

    #[cfg(not(target_os = "macos"))]
    fn metadata(_metadata: &Metadata, _volume_uuid: [u8; 16]) -> Result<Self> {
        Err(FsError::invalid(
            "unsupported_recovery_identity",
            "durable publication identity requires macOS APFS",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedEntry {
    #[serde(with = "path_bytes")]
    path: PathBuf,
    identity: DurableIdentity,
    #[serde(with = "optional_path_bytes")]
    symlink_target: Option<PathBuf>,
}

/// Persisted component identities, including selected aliases and the resolved
/// directory chain. Content changes to a directory do not invalidate its pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DirectoryEvidenceWire")]
pub(in super::super) struct DirectoryEvidence {
    schema_version: u32,
    #[serde(with = "path_bytes")]
    selected: PathBuf,
    #[serde(with = "path_bytes")]
    canonical: PathBuf,
    selected_entries: Vec<SelectedEntry>,
    canonical_entries: Vec<DurableIdentity>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryEvidenceWire {
    schema_version: u32,
    #[serde(with = "path_bytes")]
    selected: PathBuf,
    #[serde(with = "path_bytes")]
    canonical: PathBuf,
    #[serde(deserialize_with = "deserialize_components")]
    selected_entries: Vec<SelectedEntry>,
    #[serde(deserialize_with = "deserialize_components")]
    canonical_entries: Vec<DurableIdentity>,
}

impl TryFrom<DirectoryEvidenceWire> for DirectoryEvidence {
    type Error = FsError;
    fn try_from(value: DirectoryEvidenceWire) -> Result<Self> {
        let result = Self {
            schema_version: value.schema_version,
            selected: value.selected,
            canonical: value.canonical,
            selected_entries: value.selected_entries,
            canonical_entries: value.canonical_entries,
        };
        result.validate()?;
        Ok(result)
    }
}

impl DirectoryEvidence {
    pub(in super::super) fn selected(&self) -> &Path {
        &self.selected
    }
    pub(in super::super) fn canonical(&self) -> &Path {
        &self.canonical
    }

    pub(in super::super) fn validate(&self) -> Result<()> {
        validate_path(&self.selected)?;
        validate_path(&self.canonical)?;
        if self.schema_version != 1
            || self.selected_entries.len() != self.selected.components().count()
            || self.canonical_entries.len() != self.canonical.components().count()
            || self.selected_entries.is_empty()
            || self.canonical_entries.is_empty()
            || self
                .canonical
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        {
            return Err(FsError::invalid(
                "invalid_recovery_evidence",
                "invalid directory evidence version or component count",
            ));
        }
        let mut prefix = PathBuf::new();
        for (component, entry) in self.selected.components().zip(&self.selected_entries) {
            prefix.push(component);
            if entry.path != prefix {
                return Err(FsError::invalid(
                    "invalid_recovery_evidence",
                    "selected component path differs from its prefix",
                ));
            }
            entry.identity.validate()?;
            let file_type = identity_type(entry.identity.mode)?;
            if let Some(target) = &entry.symlink_target {
                if !file_type.is_symlink()
                    || target.as_os_str().as_bytes().is_empty()
                    || target.as_os_str().as_bytes().len() > MAX_PATH_BYTES
                    || target.as_os_str().as_bytes().contains(&0)
                {
                    return Err(FsError::invalid(
                        "invalid_recovery_evidence",
                        "invalid selected symlink evidence",
                    ));
                }
            } else if !file_type.is_dir() {
                return Err(FsError::invalid(
                    "invalid_recovery_evidence",
                    "selected component is not a directory or symlink",
                ));
            }
        }
        for identity in &self.canonical_entries {
            identity.validate()?;
            if !identity_type(identity.mode)?.is_dir() {
                return Err(FsError::invalid(
                    "invalid_recovery_evidence",
                    "canonical component is not a directory",
                ));
            }
        }
        Ok(())
    }
}

/// The pre-rename identity and byte extent. Rename can change ctime, so ctime
/// is checked against newly captured state throughout each fresh read instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FileEvidenceWire")]
pub(in super::super) struct FileEvidence {
    schema_version: u32,
    identity: DurableIdentity,
    byte_length: u64,
    links: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEvidenceWire {
    schema_version: u32,
    identity: DurableIdentity,
    byte_length: u64,
    links: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl TryFrom<FileEvidenceWire> for FileEvidence {
    type Error = FsError;
    fn try_from(value: FileEvidenceWire) -> Result<Self> {
        let result = Self {
            schema_version: value.schema_version,
            identity: value.identity,
            byte_length: value.byte_length,
            links: value.links,
            modified_seconds: value.modified_seconds,
            modified_nanoseconds: value.modified_nanoseconds,
        };
        result.validate()?;
        Ok(result)
    }
}

impl FileEvidence {
    pub(in super::super) fn byte_length(&self) -> u64 {
        self.byte_length
    }

    pub(in super::super) fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        if self.schema_version != 1
            || !(1..=MAX_BYTES).contains(&self.byte_length)
            || self.links != 1
            || !identity_type(self.identity.mode)?.is_file()
            || self.identity.mode & 0o7777 != 0o600
            || !(0..1_000_000_000).contains(&self.modified_nanoseconds)
        {
            return Err(FsError::invalid(
                "invalid_recovery_evidence",
                "invalid file evidence version, type, mode or extent",
            ));
        }
        Ok(())
    }

    fn capture(file: &File) -> Result<Self> {
        let before = inspect(file)?;
        let identity = DurableIdentity::file(file)?;
        let after = inspect(file)?;
        if !same_state(&before, &after, true) {
            return Err(FsError::invalid(
                "destination_changed",
                "file changed while identity was captured",
            ));
        }
        let result = Self {
            schema_version: 1,
            identity,
            byte_length: u64::try_from(after.st_size).map_err(|_| {
                FsError::invalid("invalid_recovery_evidence", "negative file extent")
            })?,
            links: u64::from(after.st_nlink),
            modified_seconds: after.st_mtime,
            modified_nanoseconds: after.st_mtime_nsec,
        };
        result.validate()?;
        Ok(result)
    }
}

impl Destination {
    pub(in super::super) fn evidence(&self) -> Result<DirectoryEvidence> {
        self.directory.evidence()
    }
}

impl PartialFile {
    pub(in super::super) fn evidence(&self) -> Result<FileEvidence> {
        if !self.sealed || self.poisoned {
            return Err(FsError::invalid(
                "partial_state",
                "only a sealed file has recovery evidence",
            ));
        }
        self.destination.directory.confirm()?;
        self.confirm()?;
        let evidence = FileEvidence::capture(&self.file)?;
        self.confirm()?;
        self.destination.directory.confirm()?;
        Ok(evidence)
    }
}

impl Directory {
    fn evidence(&self) -> Result<DirectoryEvidence> {
        self.confirm()?;
        let selected_entries = selected_entries(&self.selected)?;
        let canonical_entries = canonical_entries(&self.canonical)?;
        let result = DirectoryEvidence {
            schema_version: 1,
            selected: self.selected.clone(),
            canonical: self.canonical.clone(),
            selected_entries,
            canonical_entries,
        };
        result.validate()?;
        if result.canonical_entries.last() != Some(&DurableIdentity::file(&self.file)?) {
            return Err(FsError::invalid(
                "destination_changed",
                "retained directory differs from captured path",
            ));
        }
        self.confirm()?;
        Ok(result)
    }
}

/// An opened, requalified destination. Opening never creates any entry.
#[derive(Clone)]
pub(in super::super) struct RecoveredDirectory {
    directory: Arc<Directory>,
    evidence: DirectoryEvidence,
}

impl RecoveredDirectory {
    pub(in super::super) fn open(evidence: &DirectoryEvidence) -> Result<Self> {
        evidence.validate()?;
        let directory = Directory::open(evidence.selected())?;
        let result = Self {
            directory,
            evidence: evidence.clone(),
        };
        result.confirm()?;
        Ok(result)
    }

    pub(in super::super) fn confirm(&self) -> Result<()> {
        if self.directory.evidence()? != self.evidence {
            return Err(FsError::invalid(
                "destination_changed",
                "recorded directory identities differ",
            ));
        }
        Ok(())
    }

    /// Absence is distinct from a conflicting foreign or symlink entry. The
    /// caller acquires movie then report to use one consistent pair lock order.
    pub(in super::super) fn open_file(
        &self,
        name: &OsStr,
        evidence: &FileEvidence,
    ) -> Result<Option<RecoveredFile>> {
        validate_name(name)?;
        evidence.validate()?;
        self.confirm()?;
        let descriptor = match openat(
            &self.directory.file,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(value) => value,
            Err(rustix::io::Errno::NOENT) => {
                self.confirm()?;
                return Ok(None);
            }
            Err(error) => {
                return Err(FsError::new(
                    "destination_changed",
                    "open recorded publication entry",
                    error,
                ));
            }
        };
        let file = File::from(descriptor);
        let state = inspect(&file)?;
        validate_file(&state, &self.directory, evidence.byte_length)?;
        lock_file(&file)?;
        let observed = FileEvidence::capture(&file).map_err(|error| {
            FsError::new(
                "destination_changed",
                "capture recovered publication entry",
                io::Error::other(error),
            )
        })?;
        if observed != *evidence {
            return Err(FsError::invalid(
                "destination_changed",
                "publication entry differs from recorded identity or extent",
            ));
        }
        let result = RecoveredFile {
            directory: self.clone(),
            name: name.to_owned(),
            file,
            state,
            byte_length: evidence.byte_length,
        };
        result.confirm()?;
        Ok(Some(result))
    }
}

/// Read/sync capability only. The exclusive advisory lock stays held until
/// this handle is dropped, including after the reader has finished hashing.
pub(in super::super) struct RecoveredFile {
    directory: RecoveredDirectory,
    name: OsString,
    file: File,
    state: Stat,
    byte_length: u64,
}

impl RecoveredFile {
    pub(in super::super) fn reader<'a>(
        &'a self,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    ) -> Result<RecoveredReader<'a>> {
        check_control(cancelled, deadline)?;
        self.confirm()?;
        Ok(RecoveredReader {
            file: self,
            offset: 0,
            cancelled,
            deadline,
        })
    }

    pub(in super::super) fn confirm(&self) -> Result<()> {
        self.directory.confirm()?;
        self.confirm_file()
    }

    fn confirm_file(&self) -> Result<()> {
        let state = inspect(&self.file)?;
        validate_file(&state, &self.directory.directory, self.byte_length)?;
        let named = statat(
            &self.directory.directory.file,
            &self.name,
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|e| {
            FsError::new(
                "destination_changed",
                "inspect recovered publication name",
                e,
            )
        })?;
        if !same_state(&self.state, &state, true) || !same_state(&state, &named, true) {
            return Err(FsError::invalid(
                "destination_changed",
                "recovered file or entry changed during inspection",
            ));
        }
        Ok(())
    }

    /// Call only after comparing the complete observed bytes with the recorded
    /// expected hash and exact extent. This synchronizes but publishes nothing.
    pub(in super::super) fn sync_verified(&self) -> Result<()> {
        self.confirm()?;
        full_sync(&self.file)
            .map_err(|e| FsError::new("publication_durability", "sync recovered file", e))?;
        fsync(&self.directory.directory.file)
            .map_err(|e| FsError::new("publication_durability", "sync recovered directory", e))?;
        full_sync(&self.file).map_err(|e| {
            FsError::new(
                "publication_durability",
                "finish recovery durability barrier",
                e,
            )
        })?;
        self.confirm()
    }
}

pub(in super::super) struct RecoveredReader<'a> {
    file: &'a RecoveredFile,
    offset: u64,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Read for RecoveredReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let error = io::Error::other;
        check_control(self.cancelled, self.deadline).map_err(error)?;
        self.file.confirm_file().map_err(error)?;
        let count = bytes
            .len()
            .min(IO_BYTES)
            .min(usize::try_from(self.file.byte_length - self.offset).unwrap_or(usize::MAX));
        if count == 0 {
            self.file.confirm().map_err(error)?;
            return Ok(0);
        }
        let read = self
            .file
            .file
            .read_at(&mut bytes[..count], self.offset)
            .map_err(|e| {
                error(FsError::new(
                    "destination_io",
                    "read recovered publication file",
                    e,
                ))
            })?;
        if read == 0 {
            return Err(error(FsError::invalid(
                "destination_changed",
                "recovered publication file ended early",
            )));
        }
        self.file.confirm_file().map_err(error)?;
        check_control(self.cancelled, self.deadline).map_err(error)?;
        self.offset += read as u64;
        Ok(read)
    }
}

fn identity_type(mode: u32) -> Result<FileType> {
    Ok(FileType::from_raw_mode(mode.try_into().map_err(|_| {
        FsError::invalid(
            "invalid_recovery_evidence",
            "file mode exceeds platform range",
        )
    })?))
}

fn canonical_entries(path: &Path) -> Result<Vec<DurableIdentity>> {
    let flags =
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let mut file = File::from(
        openat(CWD, "/", flags, Mode::empty())
            .map_err(|e| FsError::new("destination_io", "open identity root", e))?,
    );
    let mut identities = vec![DurableIdentity::file(&file)?];
    for component in path.components() {
        if let Component::Normal(name) = component {
            file =
                File::from(openat(&file, name, flags, Mode::empty()).map_err(|e| {
                    FsError::new("destination_changed", "open identity component", e)
                })?);
            identities.push(DurableIdentity::file(&file)?);
        }
    }
    Ok(identities)
}

fn selected_entries(path: &Path) -> Result<Vec<SelectedEntry>> {
    let mut prefix = PathBuf::new();
    let mut entries = Vec::new();
    for component in path.components() {
        prefix.push(component);
        let before = std::fs::symlink_metadata(&prefix)
            .map_err(|e| FsError::new("destination_changed", "inspect selected identity", e))?;
        let (identity, symlink_target) = if before.file_type().is_symlink() {
            // A symlink lives on its containing directory's volume, not its
            // target's. Its own lstat birth/inode evidence and raw link text
            // are retained; canonical target components are recorded separately.
            let parent = prefix
                .parent()
                .ok_or_else(|| FsError::invalid("invalid_destination", "symlink has no parent"))?;
            let parent = std::fs::canonicalize(parent)
                .map_err(|e| FsError::new("destination_changed", "resolve symlink parent", e))?;
            let (file, _) = open_directory_chain(&parent)?;
            let parent_identity = DurableIdentity::file(&file)?;
            if parent_identity.device != before.dev() {
                return Err(FsError::invalid(
                    "destination_changed",
                    "symlink and parent volume differ",
                ));
            }
            let identity = DurableIdentity::metadata(&before, parent_identity.volume_uuid)?;
            let target = std::fs::read_link(&prefix)
                .map_err(|e| FsError::new("destination_changed", "read selected symlink", e))?;
            (identity, Some(target))
        } else {
            let file = File::from(
                openat(
                    CWD,
                    &prefix,
                    OFlags::RDONLY
                        | OFlags::DIRECTORY
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK
                        | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|e| FsError::new("destination_changed", "open selected identity", e))?,
            );
            let identity = DurableIdentity::file(&file)?;
            if identity != DurableIdentity::metadata(&before, identity.volume_uuid)? {
                return Err(FsError::invalid(
                    "destination_changed",
                    "selected component changed while opened",
                ));
            }
            (identity, None)
        };
        let after = std::fs::symlink_metadata(&prefix)
            .map_err(|e| FsError::new("destination_changed", "recheck selected identity", e))?;
        if DurableIdentity::metadata(&after, identity.volume_uuid)? != identity {
            return Err(FsError::invalid(
                "destination_changed",
                "selected component changed while captured",
            ));
        }
        entries.push(SelectedEntry {
            path: prefix.clone(),
            identity,
            symlink_target,
        });
    }
    Ok(entries)
}

fn deserialize_components<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Components<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Components<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 256 filesystem components")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Vec<T>, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() == MAX_COMPONENTS {
                    return Err(serde::de::Error::custom(
                        "filesystem component bound exceeded",
                    ));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Components(std::marker::PhantomData))
}

mod path_bytes {
    use super::*;
    use serde::de::{SeqAccess, Visitor};
    use std::{fmt, os::unix::ffi::OsStringExt};

    pub(super) fn serialize<S: serde::Serializer>(
        path: &Path,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_bytes(path.as_os_str().as_bytes())
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<PathBuf, D::Error> {
        struct PathVisitor;
        impl<'de> Visitor<'de> for PathVisitor {
            type Value = PathBuf;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded non-NUL Unix path byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<PathBuf, A::Error> {
                let mut bytes = Vec::new();
                while let Some(byte) = sequence.next_element::<u8>()? {
                    if bytes.len() >= MAX_PATH_BYTES || byte == 0 {
                        return Err(serde::de::Error::custom("path byte bound or NUL violation"));
                    }
                    bytes.push(byte);
                }
                Ok(PathBuf::from(OsString::from_vec(bytes)))
            }
        }
        deserializer.deserialize_seq(PathVisitor)
    }
}

mod optional_path_bytes {
    use super::*;
    #[derive(Serialize, Deserialize)]
    struct PathValue(#[serde(with = "path_bytes")] PathBuf);
    pub(super) fn serialize<S: serde::Serializer>(
        path: &Option<PathBuf>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        path.as_ref()
            .map(|p| PathValue(p.clone()))
            .serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Option<PathBuf>, D::Error> {
        Option::<PathValue>::deserialize(deserializer).map(|value| value.map(|p| p.0))
    }
}

#[cfg(test)]
mod tests;
