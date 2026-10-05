//! Signature-independent identity of a re-signed helper executable.
//!
//! Re-signing a helper for the application replaces only its code signature,
//! so a SHA-256 of the whole file cannot match the pinned upstream release.
//! [`content_sha256`] hashes every byte the signature does not own: each
//! architecture slice up to its `LC_CODE_SIGNATURE` data, with the signature
//! size and the `__LINKEDIT` segment sizes (which grow or shrink with the
//! signature) zeroed. The pin compiled into Deadpan therefore fixes all code
//! and appended data, such as a PyInstaller archive, independently of the
//! bundle's own manifest.
//!
//! Layout rules close the places data could hide outside that hash: the
//! signature must be the last thing in each slice, its superblob must be
//! well formed with only zero padding after it, and a universal file may have
//! only zero bytes outside its slices. Bytes inside the signature superblob
//! are not covered here; a code-signature check against the expected signer
//! covers them where one exists (see `helpers.rs`).

use sha2::{Digest, Sha256};

const MH_MAGIC_64: u32 = 0xfeed_facf;
const FAT_MAGIC: u32 = 0xcafe_babe;
const LC_SEGMENT_64: u32 = 0x19;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const SUPERBLOB_MAGIC: u32 = 0xfade_0cc0;
const MAX_SLICES: usize = 4;
const MAX_COMMANDS: u32 = 4_096;
const DOMAIN: &[u8] = b"deadpan-macho-content-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not an admissible signed Mach-O executable: {0}")]
pub struct ContentError(&'static str);

fn be32(bytes: &[u8], at: usize) -> Result<u32, ContentError> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or(ContentError("truncated"))
}

fn le32(bytes: &[u8], at: usize) -> Result<u32, ContentError> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(ContentError("truncated"))
}

fn le64(bytes: &[u8], at: usize) -> Result<u64, ContentError> {
    bytes
        .get(at..at + 8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .ok_or(ContentError("truncated"))
}

fn index(value: u64) -> Result<usize, ContentError> {
    usize::try_from(value).map_err(|_| ContentError("offset overflow"))
}

/// The canonical content of one thin 64-bit little-endian slice.
fn canonical_slice(slice: &[u8]) -> Result<Vec<u8>, ContentError> {
    if le32(slice, 0)? != MH_MAGIC_64 {
        return Err(ContentError("slice is not a 64-bit Mach-O image"));
    }
    let commands = le32(slice, 16)?;
    let commands_size = index(u64::from(le32(slice, 20)?))?;
    if commands > MAX_COMMANDS || 32 + commands_size > slice.len() {
        return Err(ContentError("load commands exceed the slice"));
    }
    let mut signature = None;
    let mut linkedit = None;
    let mut at = 32;
    for _ in 0..commands {
        let command = le32(slice, at)?;
        let size = index(u64::from(le32(slice, at + 4)?))?;
        if size < 8 || at + size > 32 + commands_size {
            return Err(ContentError("malformed load command"));
        }
        if command == LC_CODE_SIGNATURE {
            if signature.is_some() || size < 16 {
                return Err(ContentError("more than one code signature"));
            }
            signature = Some((at, le32(slice, at + 8)?, le32(slice, at + 12)?));
        } else if command == LC_SEGMENT_64
            && slice.get(at + 8..at + 24).is_some_and(|name| {
                name.iter()
                    .take_while(|byte| **byte != 0)
                    .eq(b"__LINKEDIT".iter())
            })
        {
            if linkedit.is_some() || size < 72 {
                return Err(ContentError("more than one __LINKEDIT segment"));
            }
            linkedit = Some((at, le64(slice, at + 40)?, le64(slice, at + 48)?));
        }
        at += size;
    }
    let (signature_command, offset, size) = signature.ok_or(ContentError("unsigned slice"))?;
    let (linkedit_command, linkedit_offset, linkedit_size) =
        linkedit.ok_or(ContentError("no __LINKEDIT segment"))?;
    let (offset, size) = (index(offset.into())?, index(size.into())?);
    // The signature is the final data of the slice and of __LINKEDIT.
    if offset < 32 + commands_size
        || offset.checked_add(size) != Some(slice.len())
        || linkedit_offset.checked_add(linkedit_size) != Some(slice.len() as u64)
    {
        return Err(ContentError("data follows the code signature"));
    }
    if be32(slice, offset)? != SUPERBLOB_MAGIC {
        return Err(ContentError("malformed signature superblob"));
    }
    let length = index(be32(slice, offset + 4)?.into())?;
    if length < 12 || length > size || slice[offset + length..].iter().any(|byte| *byte != 0) {
        return Err(ContentError("data hidden after the signature superblob"));
    }
    let mut canonical = slice[..offset].to_vec();
    canonical[signature_command + 12..signature_command + 16].fill(0);
    canonical[linkedit_command + 32..linkedit_command + 40].fill(0);
    canonical[linkedit_command + 48..linkedit_command + 56].fill(0);
    Ok(canonical)
}

/// SHA-256 of the signature-independent content of a thin or universal file.
pub fn content_sha256(file: &[u8]) -> Result<String, ContentError> {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    let mut add = |cpu: u32, subtype: u32, canonical: Vec<u8>| {
        hasher.update(cpu.to_be_bytes());
        hasher.update(subtype.to_be_bytes());
        hasher.update((canonical.len() as u64).to_be_bytes());
        hasher.update(&canonical);
    };
    if be32(file, 0)? == FAT_MAGIC {
        let count = index(be32(file, 4)?.into())?;
        if count == 0 || count > MAX_SLICES {
            return Err(ContentError("unsupported architecture count"));
        }
        let mut slices = Vec::with_capacity(count);
        for entry in 0..count {
            let at = 8 + entry * 20;
            let offset = index(be32(file, at + 8)?.into())?;
            let size = index(be32(file, at + 12)?.into())?;
            let end = offset.checked_add(size).filter(|end| *end <= file.len());
            let end = end.ok_or(ContentError("slice exceeds the file"))?;
            slices.push((be32(file, at)?, be32(file, at + 4)?, offset, end));
        }
        let header_end = 8 + count * 20;
        let mut covered = header_end;
        let mut ordered = slices.clone();
        ordered.sort_by_key(|slice| slice.2);
        for &(_, _, offset, end) in &ordered {
            if offset < covered || file[covered..offset].iter().any(|byte| *byte != 0) {
                return Err(ContentError("overlapping slices or data between slices"));
            }
            covered = end;
        }
        if covered != file.len() {
            return Err(ContentError("data follows the last slice"));
        }
        for (cpu, subtype, offset, end) in slices {
            add(cpu, subtype, canonical_slice(&file[offset..end])?);
        }
    } else {
        let cpu = le32(file, 4)?;
        let subtype = le32(file, 8)?;
        add(cpu, subtype, canonical_slice(file)?);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal thin arm64 executable: header, __TEXT payload, __LINKEDIT
    /// and a code-signature superblob of `signature` bytes at the end.
    pub(crate) fn synthetic(payload: &[u8], signature: usize) -> Vec<u8> {
        let commands_size = 72 + 16;
        let payload_offset = 32 + commands_size;
        let signature_offset = (payload_offset + payload.len()).next_multiple_of(16);
        let total = signature_offset + signature;
        let mut file = vec![0u8; total];
        let put32 = |file: &mut Vec<u8>, at: usize, value: u32| {
            file[at..at + 4].copy_from_slice(&value.to_le_bytes())
        };
        put32(&mut file, 0, MH_MAGIC_64);
        put32(&mut file, 4, 0x0100_000c);
        put32(&mut file, 12, 2);
        put32(&mut file, 16, 2);
        put32(&mut file, 20, commands_size as u32);
        // __LINKEDIT covering everything from the payload to the end.
        put32(&mut file, 32, LC_SEGMENT_64);
        put32(&mut file, 36, 72);
        file[40..50].copy_from_slice(b"__LINKEDIT");
        let linkedit_size = (total - payload_offset) as u64;
        file[64..72].copy_from_slice(&linkedit_size.to_le_bytes());
        file[72..80].copy_from_slice(&(payload_offset as u64).to_le_bytes());
        file[80..88].copy_from_slice(&linkedit_size.to_le_bytes());
        put32(&mut file, 104, LC_CODE_SIGNATURE);
        put32(&mut file, 108, 16);
        put32(&mut file, 112, signature_offset as u32);
        put32(&mut file, 116, signature as u32);
        file[payload_offset..payload_offset + payload.len()].copy_from_slice(payload);
        file[signature_offset..signature_offset + 4]
            .copy_from_slice(&SUPERBLOB_MAGIC.to_be_bytes());
        file[signature_offset + 4..signature_offset + 8]
            .copy_from_slice(&(signature as u32 - 8).to_be_bytes());
        for (i, byte) in file[signature_offset + 12..total - 8]
            .iter_mut()
            .enumerate()
        {
            *byte = (i % 251) as u8 + 1;
        }
        file
    }

    #[test]
    fn re_signing_keeps_the_content_hash() {
        let original = synthetic(b"code and archive", 64);
        let resigned = synthetic(b"code and archive", 256);
        assert_ne!(original, resigned);
        assert_eq!(
            content_sha256(&original).unwrap(),
            content_sha256(&resigned).unwrap()
        );
        assert_ne!(
            content_sha256(&original).unwrap(),
            content_sha256(&synthetic(b"code and archivf", 64)).unwrap()
        );
    }

    #[test]
    fn data_outside_the_hashed_content_is_refused() {
        let file = synthetic(b"payload", 64);
        // Appended bytes after the signature.
        let mut appended = file.clone();
        appended.extend_from_slice(b"MEI\x0c\x0b\x0a\x0b\x0e");
        assert!(content_sha256(&appended).is_err());
        // Bytes hidden in the padding after the superblob's declared length.
        let mut hidden = file.clone();
        let last = hidden.len() - 1;
        hidden[last] = 1;
        assert!(content_sha256(&hidden).is_err());
        // Not a signature superblob.
        let mut forged = file.clone();
        let offset = u32::from_le_bytes(forged[112..116].try_into().unwrap()) as usize;
        forged[offset] ^= 0xff;
        assert!(content_sha256(&forged).is_err());
        assert!(content_sha256(b"#!/bin/sh\n").is_err());
    }

    #[test]
    fn universal_files_hash_every_slice_and_nothing_else() {
        let slice = synthetic(b"slice", 64);
        let mut fat = vec![0u8; 0x1000];
        fat[0..4].copy_from_slice(&FAT_MAGIC.to_be_bytes());
        fat[4..8].copy_from_slice(&1u32.to_be_bytes());
        fat[8..12].copy_from_slice(&0x0100_000cu32.to_be_bytes());
        fat[16..20].copy_from_slice(&0x1000u32.to_be_bytes());
        fat[20..24].copy_from_slice(&(slice.len() as u32).to_be_bytes());
        fat[24..28].copy_from_slice(&12u32.to_be_bytes());
        fat.extend_from_slice(&slice);
        let digest = content_sha256(&fat).unwrap();
        assert_eq!(digest.len(), 64);
        let mut between = fat.clone();
        between[0x800] = 1;
        assert!(content_sha256(&between).is_err());
        let mut trailing = fat.clone();
        trailing.push(0);
        assert!(content_sha256(&trailing).is_err());
    }

    /// `DEADPAN_PRINT_CONTENT_SHA256=/path` prints a helper's content pin.
    #[test]
    fn print_content_pin_when_requested() {
        if let Some(path) = std::env::var_os("DEADPAN_PRINT_CONTENT_SHA256") {
            let bytes = std::fs::read(path).unwrap();
            println!("content_sha256 {:?}", content_sha256(&bytes));
        }
    }
}
