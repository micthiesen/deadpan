//! Synchronous fixed-shape own-process observations. Native code borrows output
//! buffers and descriptors only during a call; no pointer or descriptor escapes.

use std::{
    ffi::{c_char, c_int},
    fs::File,
    os::{fd::AsRawFd, unix::ffi::OsStringExt},
    path::PathBuf,
};

use crate::runtime::{
    MappedImageIdentity, RuntimeFileTime, RuntimeIdentityError, RuntimeImageKind, RuntimePlatform,
};

#[repr(C)]
pub(crate) struct Image {
    abi_version: u32,
    kind: u32,
    device: u64,
    inode: u64,
    file_size: u64,
    modification_seconds: i64,
    change_seconds: i64,
    birth_seconds: i64,
    modification_nanoseconds: u32,
    change_nanoseconds: u32,
    birth_nanoseconds: u32,
    generation: u32,
    uuid: [u8; 16],
    cpu_type: u32,
    cpu_subtype: u32,
    file_type: u32,
    header_address: u64,
    anchor_address: u64,
    header_file_offset: u64,
    path: [c_char; 1024],
}

impl Image {
    pub(crate) fn identity(
        &self,
        kind: RuntimeImageKind,
    ) -> Result<MappedImageIdentity, RuntimeIdentityError> {
        if self.abi_version != 1 || self.kind != kind.native() {
            return Err(RuntimeIdentityError::InvalidEvidence(
                "native image ABI or role differs",
            ));
        }
        let identity = MappedImageIdentity {
            kind,
            device: self.device,
            inode: self.inode,
            uuid: self.uuid,
            file_size: self.file_size,
            modification_time: RuntimeFileTime {
                seconds: self.modification_seconds,
                nanoseconds: self.modification_nanoseconds,
            },
            change_time: RuntimeFileTime {
                seconds: self.change_seconds,
                nanoseconds: self.change_nanoseconds,
            },
            birth_time: RuntimeFileTime {
                seconds: self.birth_seconds,
                nanoseconds: self.birth_nanoseconds,
            },
            generation: self.generation,
        };
        identity.validate()?;
        Ok(identity)
    }

    pub(crate) fn path(&self) -> Result<PathBuf, RuntimeIdentityError> {
        let bytes = terminated(&self.path)?;
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        if !path.is_absolute() {
            return Err(RuntimeIdentityError::InvalidEvidence(
                "mapped reopening path is not absolute",
            ));
        }
        Ok(path)
    }
}

#[repr(C)]
struct Platform {
    abi_version: u32,
    cpu_family: u32,
    os_build: [c_char; 64],
    hardware_model: [c_char; 64],
}

#[repr(C)]
struct Error {
    code: [c_char; 48],
    message: [c_char; 256],
}

impl Error {
    fn empty() -> Self {
        Self {
            code: [0; 48],
            message: [0; 256],
        }
    }
    fn into_error(self) -> RuntimeIdentityError {
        let code = text(&self.code);
        let message = text(&self.message);
        match (code, message) {
            (Ok(code), Ok(message)) if !code.is_empty() && !message.is_empty() => {
                RuntimeIdentityError::Native { code, message }
            }
            _ => RuntimeIdentityError::InvalidEvidence("native runtime error is malformed"),
        }
    }
}

unsafe extern "C" {
    fn dp_runtime_capture(kind: u32, image: *mut Image, error: *mut Error) -> c_int;
    fn dp_runtime_revalidate(image: *const Image, fd: c_int, error: *mut Error) -> c_int;
    fn dp_runtime_observe_platform(platform: *mut Platform, error: *mut Error) -> c_int;
}

pub(crate) fn capture(kind: RuntimeImageKind) -> Result<Image, RuntimeIdentityError> {
    let mut output = std::mem::MaybeUninit::<Image>::uninit();
    let mut error = Error::empty();
    // SAFETY: fixed repr(C) buffers have the corresponding native layouts.
    // Native capture initializes the complete Image only on success and retains
    // no pointer. It observes this process through bounded kernel queries.
    let result = unsafe { dp_runtime_capture(kind.native(), output.as_mut_ptr(), &mut error) };
    if result != 1 {
        return Err(error.into_error());
    }
    // SAFETY: the successful native call initialized the complete Image above.
    Ok(unsafe { output.assume_init() })
}

pub(crate) fn revalidate(image: &Image, file: &File) -> Result<(), RuntimeIdentityError> {
    let mut error = Error::empty();
    // SAFETY: image came from capture, the borrowed regular-file candidate stays
    // open for this call, and C neither closes nor retains its descriptor. C
    // validates metadata before its bounded pread operations.
    let result = unsafe { dp_runtime_revalidate(image, file.as_raw_fd(), &mut error) };
    if result == 1 {
        Ok(())
    } else {
        Err(error.into_error())
    }
}

pub(crate) fn platform() -> Result<RuntimePlatform, RuntimeIdentityError> {
    let mut output = std::mem::MaybeUninit::<Platform>::uninit();
    let mut error = Error::empty();
    // SAFETY: native code fills this fixed repr(C) output on success and retains
    // no pointer. Each sysctl output has its own fixed native bound.
    let result = unsafe { dp_runtime_observe_platform(output.as_mut_ptr(), &mut error) };
    if result != 1 {
        return Err(error.into_error());
    }
    // SAFETY: successful platform observation initialized every output byte.
    let output = unsafe { output.assume_init() };
    if output.abi_version != 1 {
        return Err(RuntimeIdentityError::InvalidEvidence(
            "native platform ABI differs",
        ));
    }
    let value = RuntimePlatform {
        os_build: text(&output.os_build)?,
        hardware_model: text(&output.hardware_model)?,
        cpu_family: output.cpu_family,
    };
    value.validate()?;
    Ok(value)
}

fn terminated(bytes: &[c_char]) -> Result<Vec<u8>, RuntimeIdentityError> {
    let length =
        bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(RuntimeIdentityError::InvalidEvidence(
                "native string has no bounded terminator",
            ))?;
    if bytes[length..].iter().any(|byte| *byte != 0) {
        return Err(RuntimeIdentityError::InvalidEvidence(
            "native string has hidden trailing bytes",
        ));
    }
    Ok(bytes[..length]
        .iter()
        .map(|byte| byte.to_ne_bytes()[0])
        .collect())
}

fn text(bytes: &[c_char]) -> Result<String, RuntimeIdentityError> {
    String::from_utf8(terminated(bytes)?)
        .map_err(|_| RuntimeIdentityError::InvalidEvidence("native text is not UTF-8"))
}
