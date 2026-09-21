//! Developer-only generated-media storage qualification.
//!
//! This example proves that published bytes remain readable after package
//! relocation. It does not register media, validate media, accept a candidate,
//! or consult a worker/model path.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod supported {
    use std::error::Error;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, Write};
    use std::path::{Path, PathBuf};

    use deadpan_core::{
        ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    };
    use deadpan_store::generated_media::{
        GeneratedContentId, GeneratedMediaLimits, GeneratedObjectRef,
    };
    use deadpan_store::{AccessMode, ProjectStore};

    const MAX_MEDIA_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_REFERENCE_BYTES: u64 = 16 * 1024;
    const COPY_BUFFER_BYTES: usize = 64 * 1024;

    type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

    fn limits() -> Result<GeneratedMediaLimits> {
        Ok(GeneratedMediaLimits::new(MAX_MEDIA_BYTES)?)
    }

    fn initial_document() -> Result<ProjectDocument> {
        Ok(ProjectDocument::new(
            ProjectId::new("generated-storage-qualification")?,
            RevisionId::new("r0")?,
            PresentationBasis {
                width: 768,
                height: 320,
                frame_rate: FrameRate::new(30_000, 1_001)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root")?,
        )?)
    }

    fn stream_digest(reader: &mut impl Read) -> Result<(String, u64)> {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; COPY_BUFFER_BYTES];
        let mut length = 0u64;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            length = length
                .checked_add(u64::try_from(read)?)
                .ok_or("input length overflow")?;
            if length > MAX_MEDIA_BYTES {
                return Err(
                    format!("media exceeds {MAX_MEDIA_BYTES} byte qualification limit").into(),
                );
            }
            hasher.update(&buffer[..read]);
        }
        Ok((hasher.finalize().to_hex().to_string(), length))
    }

    fn expected_reference(input: &Path) -> Result<(GeneratedObjectRef, File)> {
        let mut file = File::open(input)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("canonical master input is not a regular file".into());
        }
        if metadata.len() == 0 || metadata.len() > MAX_MEDIA_BYTES {
            return Err("canonical master input is empty or exceeds its bound".into());
        }
        let (digest, length) = stream_digest(&mut file)?;
        file.rewind()?;
        Ok((
            GeneratedObjectRef::new(GeneratedContentId::new(digest)?, length)?,
            file,
        ))
    }

    fn write_reference(path: &Path, expected: &GeneratedObjectRef) -> Result<()> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        serde_json::to_writer(&mut file, expected)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        File::open(parent)?.sync_all()?;
        Ok(())
    }

    fn read_reference(path: &Path) -> Result<GeneratedObjectRef> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_REFERENCE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.is_empty() || u64::try_from(bytes.len())? > MAX_REFERENCE_BYTES {
            return Err("reference JSON exceeds its bound".into());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn open_or_create(path: &Path) -> Result<ProjectStore> {
        if path.exists() {
            return Ok(ProjectStore::open(path, AccessMode::ReadWrite)?);
        }
        Ok(ProjectStore::create(path, &initial_document()?)?)
    }

    fn write_snapshot(
        package: &Path,
        reference: &GeneratedObjectRef,
        output: &Path,
    ) -> Result<(String, u64)> {
        let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
        let mut snapshot = store.snapshot_generated_object(reference, limits()?)?;
        if snapshot.reference() != reference {
            return Err("stored snapshot returned a different typed reference".into());
        }
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; COPY_BUFFER_BYTES];
        let mut length = 0u64;
        loop {
            let read = snapshot.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            length = length
                .checked_add(u64::try_from(read)?)
                .ok_or("snapshot length overflow")?;
            if length > MAX_MEDIA_BYTES {
                return Err("snapshot exceeded its qualification limit".into());
            }
            hasher.update(&buffer[..read]);
            destination.write_all(&buffer[..read])?;
        }
        destination.sync_all()?;
        File::open(parent)?.sync_all()?;
        Ok((hasher.finalize().to_hex().to_string(), length))
    }

    fn promote(package: &Path, input: &Path, reference_path: &Path) -> Result<()> {
        let (expected, mut input_file) = expected_reference(input)?;
        let mut store = open_or_create(package)?;
        let published = store.promote_generated_object(&mut input_file, &expected, limits()?)?;
        if published != expected {
            return Err("store returned a different reference after publication".into());
        }
        write_reference(reference_path, &expected)?;
        println!(
            "{{\"command\":\"promote\",\"package\":{},\"reference\":{}}}",
            serde_json::to_string(package.to_string_lossy().as_ref())?,
            serde_json::to_string(&expected)?,
        );
        Ok(())
    }

    fn read(package: &Path, reference_path: &Path, output: &Path) -> Result<()> {
        let expected = read_reference(reference_path)?;
        let (digest, length) = write_snapshot(package, &expected, output)?;
        if digest != expected.content().digest() || length != expected.byte_length() {
            return Err("readback digest or length does not match the typed reference".into());
        }
        println!(
            "{{\"command\":\"read\",\"package\":{},\"reference\":{},\"output\":{},\"verified_blake3\":{},\"verified_byte_length\":{}}}",
            serde_json::to_string(package.to_string_lossy().as_ref())?,
            serde_json::to_string(&expected)?,
            serde_json::to_string(output.to_string_lossy().as_ref())?,
            serde_json::to_string(&digest)?,
            length,
        );
        Ok(())
    }

    pub fn run() -> Result<()> {
        let mut arguments = std::env::args_os();
        let _program = arguments.next();
        let command = arguments.next().ok_or("expected promote or read")?;
        let values: Vec<PathBuf> = arguments.map(PathBuf::from).collect();
        match (command.to_str(), values.as_slice()) {
            (Some("promote"), [package, input, reference]) => promote(package, input, reference),
            (Some("read"), [package, reference, output]) => read(package, reference, output),
            _ => Err("usage: qualify_generated_storage promote PACKAGE.deadpan INPUT.mkv REFERENCE.json\n       qualify_generated_storage read PACKAGE.deadpan REFERENCE.json OUTPUT.mkv".into()),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() {
    if let Err(error) = supported::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("generated media storage is unavailable on this target");
    std::process::exit(1);
}
