//! Descriptor-based APFS volume identity for cooperative recovery checks,
//! and macOS file bookmarks for finding moved linked media.
//!
//! A volume UUID is replacement evidence, not proof against a malicious owner.
//! This adapter does not open paths, take ownership of descriptors, mutate files,
//! infer content identity, or promise that an inode can never be reused.
//!
//! A bookmark is only a hint where a file went: resolving one never proves the
//! file is the same content, so callers verify the bytes they find.

use std::{fs::File, io};

pub mod bookmark;

/// Return the nonzero UUID of the APFS volume containing this open descriptor.
/// Other platforms/filesystems and absent identities fail explicitly.
pub fn apfs_volume_uuid(file: &File) -> io::Result<[u8; 16]> {
    platform::apfs_volume_uuid(file)
}

#[cfg(any(target_os = "macos", test))]
fn validate_uuid(length: u32, uuid: [u8; 16]) -> io::Result<[u8; 16]> {
    if length != 20 || uuid == [0; 16] {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "APFS volume identity is absent or has an unsupported layout",
        ));
    }
    Ok(uuid)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::{mem::MaybeUninit, os::fd::AsRawFd};

    #[repr(C)]
    struct VolumeAttributes {
        length: u32,
        uuid: [u8; 16],
    }

    #[allow(unsafe_code)]
    pub(super) fn apfs_volume_uuid(file: &File) -> io::Result<[u8; 16]> {
        let mut filesystem = MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: fstatfs writes exactly one statfs into valid aligned storage.
        // The borrowed file stays alive for the call. Storage is read only on
        // success; neither native call retains a pointer or closes the file.
        let result = unsafe { libc::fstatfs(file.as_raw_fd(), filesystem.as_mut_ptr()) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fstatfs initialized the complete statfs on success above.
        let filesystem = unsafe { filesystem.assume_init() };
        if filesystem.f_fstypename[..5] != [97, 112, 102, 115, 0] {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "durable filesystem identity requires APFS",
            ));
        }
        let mut attributes = libc::attrlist {
            bitmapcount: 5,
            reserved: 0,
            commonattr: 0,
            volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        let mut output = VolumeAttributes {
            length: 0,
            uuid: [0; 16],
        };
        // SAFETY: attrlist and the fixed 20-byte result buffer have the native
        // layouts for ATTR_VOL_UUID. Only this fixed attribute is requested.
        // Both writable pointers and the borrowed descriptor remain valid for
        // this synchronous call; the returned length is checked before use.
        let result = unsafe {
            libc::fgetattrlist(
                file.as_raw_fd(),
                (&raw mut attributes).cast(),
                (&raw mut output).cast(),
                std::mem::size_of::<VolumeAttributes>(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        validate_uuid(output.length, output.uuid)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub(super) fn apfs_volume_uuid(_file: &File) -> io::Result<[u8; 16]> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "durable filesystem identity requires macOS APFS",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_malformed_native_results_are_not_identities() {
        for length in [0, 19, 21, u32::MAX] {
            assert_eq!(
                validate_uuid(length, [1; 16]).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
        }
        assert!(validate_uuid(20, [0; 16]).is_err());
        assert_eq!(validate_uuid(20, [1; 16]).unwrap(), [1; 16]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apfs_file_and_parent_share_uuid_and_descriptor_remains_usable() {
        use std::io::{Read, Write};
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("file");
        let mut file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        let directory = File::open(folder.path()).unwrap();
        let uuid = apfs_volume_uuid(&file).unwrap();
        assert_ne!(uuid, [0; 16]);
        assert_eq!(uuid, apfs_volume_uuid(&directory).unwrap());
        file.write_all(b"still open").unwrap();
        let mut bytes = [0; 1];
        assert_eq!(file.read(&mut bytes).unwrap(), 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unsupported_descriptor_returns_error_without_consuming_it() {
        use std::io::Read;
        let mut file = File::open("/dev/null").unwrap();
        assert!(apfs_volume_uuid(&file).is_err());
        assert_eq!(file.read(&mut [0; 1]).unwrap(), 0);
    }
}
