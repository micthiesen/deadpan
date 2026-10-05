//! Uncompressed tar archives of model packs: a bounded ustar/pax reader for
//! offline import and the writer `models export` uses.
//!
//! Entry names are only compared with manifest paths; nothing is ever written
//! to a location derived from an archive name, so path traversal and links
//! cannot escape the staging directory. Matching entries must be regular files.

use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{InstallProgress, PackError};

const BLOCK: usize = 512;
/// Longest entry name accepted from any header form.
const MAX_NAME: usize = 1024;
/// Largest pax extended header accepted.
const MAX_PAX: u64 = 64 * 1024;
/// Entries read before an archive is refused as unreasonable.
const MAX_ENTRIES: usize = 100_000;

/// One archive member as the selector sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The member path without a leading `./`.
    pub path: String,
    pub size: u64,
    /// Regular file (as opposed to a link, directory or device).
    pub regular: bool,
}

/// What to do with one member.
pub enum Action {
    Skip,
    /// Write the member's bytes to this path, replacing it.
    Extract(PathBuf),
}

fn invalid(reason: &'static str) -> PackError {
    PackError::Verification {
        file: "archive".into(),
        reason,
    }
}

fn octal(field: &[u8]) -> Result<u64, PackError> {
    // GNU base-256 for values that do not fit the octal field.
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        let mut value: u64 = u64::from(field[0] & 0x7f);
        for byte in &field[1..] {
            value = value
                .checked_mul(256)
                .and_then(|value| value.checked_add(u64::from(*byte)))
                .ok_or(invalid("archive size field overflows"))?;
        }
        return Ok(value);
    }
    let text: String = field
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as char)
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| invalid("archive header has a malformed number"))
}

fn text(field: &[u8]) -> String {
    String::from_utf8_lossy(&field[..field.iter().position(|b| *b == 0).unwrap_or(field.len())])
        .into_owned()
}

fn checksum_valid(header: &[u8; BLOCK]) -> Result<bool, PackError> {
    let stored = octal(&header[148..156])?;
    let sum: u64 = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    Ok(stored == sum)
}

fn padding(size: u64) -> u64 {
    (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64
}

/// Parse `length key=value\n` pax records.
fn pax(records: &[u8], path: &mut Option<String>, size: &mut Option<u64>) -> Result<(), PackError> {
    let mut rest = records;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(invalid("archive has a malformed pax record"))?;
        let length: usize = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|text| text.parse().ok())
            .filter(|length| *length > space + 1 && *length <= rest.len())
            .ok_or(invalid("archive has a malformed pax record"))?;
        let record = &rest[space + 1..length];
        let record = record
            .strip_suffix(b"\n")
            .ok_or(invalid("archive has a malformed pax record"))?;
        if let Some(value) = record.strip_prefix(b"path=") {
            if value.len() > MAX_NAME {
                return Err(invalid("archive entry name is too long"));
            }
            *path = Some(String::from_utf8_lossy(value).into_owned());
        } else if let Some(value) = record.strip_prefix(b"size=") {
            *size = Some(
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .ok_or(invalid("archive has a malformed pax size"))?,
            );
        }
        rest = &rest[length..];
    }
    Ok(())
}

/// Stream through `path`, asking `select` about each member and calling
/// `extracted` after a member's bytes were written and synced.
pub fn read(
    path: &Path,
    cancelled: &AtomicBool,
    mut select: impl FnMut(&Entry) -> Result<Action, PackError>,
    mut extracted: impl FnMut(&Path) -> Result<(), PackError>,
) -> Result<(), PackError> {
    let mut input = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut pending_path: Option<String> = None;
    let mut pending_size: Option<u64> = None;
    let mut entries = 0;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(PackError::Cancelled);
        }
        let mut header = [0_u8; BLOCK];
        match input.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(invalid("archive ends without its end marker"));
            }
            Err(error) => return Err(error.into()),
        }
        if header.iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(invalid("archive has too many entries"));
        }
        if !checksum_valid(&header)? {
            return Err(invalid("archive header checksum is wrong"));
        }
        let kind = header[156];
        let mut size = octal(&header[124..136])?;
        match kind {
            b'x' | b'g' | b'L' | b'K' => {
                if size > MAX_PAX {
                    return Err(invalid("archive extended header is too large"));
                }
                let mut data = vec![0_u8; size as usize];
                input.read_exact(&mut data)?;
                input.seek_relative(padding(size) as i64)?;
                match kind {
                    b'x' => pax(&data, &mut pending_path, &mut pending_size)?,
                    b'L' => {
                        if data.len() > MAX_NAME + 1 {
                            return Err(invalid("archive entry name is too long"));
                        }
                        pending_path = Some(text(&data));
                    }
                    _ => {}
                }
                continue;
            }
            _ => {}
        }
        let name = match pending_path.take() {
            Some(name) => name,
            None => {
                let name = text(&header[0..100]);
                let prefix = if &header[257..262] == b"ustar" {
                    text(&header[345..500])
                } else {
                    String::new()
                };
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            }
        };
        if let Some(pax_size) = pending_size.take() {
            size = pax_size;
        }
        let entry = Entry {
            path: name.trim_start_matches("./").to_owned(),
            size,
            regular: matches!(kind, b'0' | 0 | b'7'),
        };
        // Links and directories carry no data even when a size is recorded.
        let data = if matches!(kind, b'1' | b'2' | b'5') {
            0
        } else {
            size
        };
        let skip = data
            .checked_add(padding(data))
            .and_then(|bytes| i64::try_from(bytes).ok())
            .ok_or(invalid("archive member is too large"))?;
        match select(&entry)? {
            Action::Skip => {
                input.seek_relative(skip)?;
            }
            Action::Extract(destination) => {
                {
                    // A new file only: never follow or reuse what is there.
                    let mut output = BufWriter::with_capacity(
                        1 << 20,
                        OpenOptions::new()
                            .create_new(true)
                            .write(true)
                            .open(&destination)?,
                    );
                    let mut remaining = data;
                    let mut buffer = vec![0_u8; 1 << 20];
                    while remaining > 0 {
                        if cancelled.load(Ordering::Acquire) {
                            return Err(PackError::Cancelled);
                        }
                        let want = remaining.min(buffer.len() as u64) as usize;
                        input.read_exact(&mut buffer[..want])?;
                        output.write_all(&buffer[..want])?;
                        remaining -= want as u64;
                    }
                    output.flush()?;
                    output.get_ref().sync_all()?;
                }
                input.seek_relative(padding(data) as i64)?;
                extracted(&destination)?;
            }
        }
    }
}

fn ustar_header(name: &str, size: u64, kind: u8) -> [u8; BLOCK] {
    let mut header = [0_u8; BLOCK];
    let bytes = name.as_bytes();
    let length = bytes.len().min(100);
    header[..length].copy_from_slice(&bytes[..length]);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    // Sizes above the octal field live in the pax header; record zero here.
    let shown = if size < 0o77777777777 { size } else { 0 };
    header[124..136].copy_from_slice(format!("{shown:011o}\0").as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[156] = kind;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].copy_from_slice(b"        ");
    let sum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    header
}

fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut length = body.len() + 1;
    while format!("{length}{body}").len() != length {
        length += 1;
    }
    format!("{length}{body}").into_bytes()
}

/// Write `(archive path, source file, exact size)` entries to `destination`.
pub fn write(
    destination: &Path,
    entries: &[(String, PathBuf, u64)],
    cancelled: &AtomicBool,
    mut progress: impl FnMut(InstallProgress),
) -> Result<(), PackError> {
    let total: u64 = entries.iter().map(|(_, _, size)| size).sum();
    if std::fs::symlink_metadata(destination).is_ok() {
        return Err(PackError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", destination.display()),
        )));
    }
    let mut partial_name = destination
        .file_name()
        .ok_or(invalid("the archive needs a file name"))?
        .to_os_string();
    partial_name.push(".part");
    let partial = destination.with_file_name(partial_name);
    let result = (|| {
        let mut output = BufWriter::with_capacity(
            1 << 20,
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&partial)?,
        );
        let mut completed = 0;
        let zeros = [0_u8; BLOCK];
        for (name, source, size) in entries {
            let mut input = File::open(source)?;
            if input.metadata()?.len() != *size {
                return Err(PackError::Verification {
                    file: name.clone(),
                    reason: "installed size differs from its manifest",
                });
            }
            let mut records = pax_record("path", name);
            records.extend(pax_record("size", &size.to_string()));
            output.write_all(&ustar_header("PaxHeader", records.len() as u64, b'x'))?;
            output.write_all(&records)?;
            output.write_all(&zeros[..padding(records.len() as u64) as usize])?;
            output.write_all(&ustar_header(name, *size, b'0'))?;
            let mut buffer = vec![0_u8; 1 << 20];
            let mut written = 0_u64;
            loop {
                if cancelled.load(Ordering::Acquire) {
                    return Err(PackError::Cancelled);
                }
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                output.write_all(&buffer[..read])?;
                written += read as u64;
                progress(InstallProgress {
                    completed_bytes: completed + written,
                    total_bytes: total,
                });
            }
            if written != *size {
                return Err(PackError::Verification {
                    file: name.clone(),
                    reason: "installed file changed while exporting",
                });
            }
            output.write_all(&zeros[..padding(*size) as usize])?;
            completed += size;
        }
        output.write_all(&zeros)?;
        output.write_all(&zeros)?;
        output.flush()?;
        output.get_ref().sync_all()?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            // Publish without replacing anything that appeared meanwhile.
            let published = std::fs::hard_link(&partial, destination);
            let _ = std::fs::remove_file(&partial);
            Ok(published?)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(error)
        }
    }
}

#[cfg(test)]
pub(super) fn test_octal(field: &[u8]) -> u64 {
    octal(field).unwrap()
}
