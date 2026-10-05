//! Fresh observations of the mapped helper and media libraries. File digests
//! identify their checked backing objects, not relocated resident memory pages.
//! This boundary assumes trusted installed code; observations cannot restore a
//! live encoder admission or authorize encoding by themselves.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::atomic::AtomicBool,
    time::Instant,
};

use deadpan_encode::{
    SDR_POLICY_VERSION_V1, SdrPolicy,
    runtime::{self, MappedImageIdentity, RuntimeImageKind, RuntimePlatform},
};
use deadpan_jobs::Sha256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};

use super::{
    EncodedRenderError, check_control,
    protocol::{EncodedRenderContract, EncoderChoice},
};

const SCHEMA_VERSION: u32 = 1;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
const IMAGE_KINDS: [RuntimeImageKind; 5] = [
    RuntimeImageKind::Helper,
    RuntimeImageKind::Avcodec,
    RuntimeImageKind::Avformat,
    RuntimeImageKind::Avutil,
    RuntimeImageKind::Swscale,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeImageFingerprint {
    pub mapped: MappedImageIdentity,
    pub sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFingerprint {
    pub schema_version: u32,
    pub platform: RuntimePlatform,
    pub system: String,
    pub kernel_release: String,
    pub kernel_build: String,
    pub machine: String,
    pub images: [RuntimeImageFingerprint; 5],
}

impl RuntimeFingerprint {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported encoder runtime fingerprint".into());
        }
        self.platform
            .validate()
            .map_err(|error| error.to_string())?;
        for value in [
            &self.system,
            &self.kernel_release,
            &self.kernel_build,
            &self.machine,
        ] {
            validate_text(value)?;
        }
        for (image, kind) in self.images.iter().zip(IMAGE_KINDS) {
            image.mapped.validate().map_err(|error| error.to_string())?;
            if image.mapped.kind != kind || image.mapped.file_size > MAX_IMAGE_BYTES {
                return Err("runtime image order, identity or size differs".into());
            }
        }
        Ok(())
    }

    pub fn helper(&self) -> &RuntimeImageFingerprint {
        &self.images[0]
    }
}

/// Hash only descriptors matched to the current process's mapped vnode and
/// loaded UUID. Keep all descriptors until the final revalidation. Native
/// observation rejects unavailable, ambiguous and unsupported identities.
pub struct RuntimeCapture {
    fingerprint: RuntimeFingerprint,
    observed: [runtime::RuntimeImageObservation; 5],
    files: Vec<File>,
}

impl RuntimeCapture {
    pub fn fingerprint(&self) -> &RuntimeFingerprint {
        &self.fingerprint
    }

    pub fn revalidate(
        &mut self,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<(), EncodedRenderError> {
        for ((observation, file), expected) in self
            .observed
            .iter()
            .zip(&mut self.files)
            .zip(&self.fingerprint.images)
        {
            check_control(cancelled, deadline)?;
            observation.revalidate(file).map_err(native_failure)?;
            file.seek(SeekFrom::Start(0))?;
            if hash_image(file, expected.mapped.file_size, cancelled, deadline)? != expected.sha256
            {
                return Err(EncodedRenderError::Protocol(
                    "mapped runtime bytes changed during work".into(),
                ));
            }
            observation.revalidate(file).map_err(native_failure)?;
        }
        let uname = rustix::system::uname();
        if runtime::observe_platform().map_err(native_failure)? != self.fingerprint.platform
            || runtime_text(uname.sysname())? != self.fingerprint.system
            || runtime_text(uname.release())? != self.fingerprint.kernel_release
            || runtime_text(uname.version())? != self.fingerprint.kernel_build
            || runtime_text(uname.machine())? != self.fingerprint.machine
        {
            return Err(EncodedRenderError::Protocol(
                "encoder platform changed during work".into(),
            ));
        }
        check_control(cancelled, deadline)
    }
}

pub fn capture(
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RuntimeCapture, EncodedRenderError> {
    check_control(cancelled, deadline)?;
    let platform = runtime::observe_platform().map_err(native_failure)?;
    let uname = rustix::system::uname();
    let system = runtime_text(uname.sysname())?;
    let kernel_release = runtime_text(uname.release())?;
    let kernel_build = runtime_text(uname.version())?;
    let machine = runtime_text(uname.machine())?;
    let observed = runtime::capture_current().map_err(native_failure)?;
    let mut files = Vec::with_capacity(IMAGE_KINDS.len());
    let mut images = Vec::with_capacity(IMAGE_KINDS.len());
    for (observation, kind) in observed.iter().zip(IMAGE_KINDS) {
        check_control(cancelled, deadline)?;
        let mapped = observation.identity();
        mapped.validate().map_err(native_failure)?;
        if mapped.kind != kind || mapped.file_size > MAX_IMAGE_BYTES {
            return Err(EncodedRenderError::Protocol(
                "mapped runtime image exceeds its identity or byte bounds".into(),
            ));
        }
        let mut file = observation.open_matching().map_err(native_failure)?;
        let sha256 = hash_image(&mut file, mapped.file_size, cancelled, deadline)?;
        observation.revalidate(&file).map_err(native_failure)?;
        images.push(RuntimeImageFingerprint {
            mapped: mapped.clone(),
            sha256,
        });
        files.push(file);
    }
    for (observation, file) in observed.iter().zip(&files) {
        check_control(cancelled, deadline)?;
        observation.revalidate(file).map_err(native_failure)?;
    }
    if runtime::observe_platform().map_err(native_failure)? != platform {
        return Err(EncodedRenderError::Protocol(
            "encoder platform changed during runtime capture".into(),
        ));
    }
    let fingerprint = RuntimeFingerprint {
        schema_version: SCHEMA_VERSION,
        platform,
        system,
        kernel_release,
        kernel_build,
        machine,
        images: images
            .try_into()
            .map_err(|_| EncodedRenderError::Protocol("runtime image inventory differs".into()))?,
    };
    fingerprint
        .validate()
        .map_err(EncodedRenderError::Protocol)?;
    check_control(cancelled, deadline)?;
    Ok(RuntimeCapture {
        fingerprint,
        observed,
        files,
    })
}

fn hash_image(
    input: &mut impl Read,
    expected_bytes: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, EncodedRenderError> {
    if expected_bytes == 0 || expected_bytes > MAX_IMAGE_BYTES {
        return Err(EncodedRenderError::Configuration(
            "runtime image byte bound",
        ));
    }
    let mut digest = Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        check_control(cancelled, deadline)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(count).expect("bounded hash block"))
            .ok_or_else(|| EncodedRenderError::Protocol("runtime image length overflow".into()))?;
        if total > expected_bytes {
            return Err(EncodedRenderError::Protocol(
                "runtime image grew while hashing".into(),
            ));
        }
        digest.update(&buffer[..count]);
    }
    if total != expected_bytes {
        return Err(EncodedRenderError::Protocol(
            "runtime image changed while hashing".into(),
        ));
    }
    check_control(cancelled, deadline)?;
    Sha256::new(
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .map_err(|error| EncodedRenderError::Protocol(error.to_string()))
}

fn validate_text(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err("runtime text exceeds its bounds".into())
    } else {
        Ok(())
    }
}

fn runtime_text(value: &std::ffi::CStr) -> Result<String, EncodedRenderError> {
    let value = value
        .to_str()
        .map_err(|_| EncodedRenderError::Configuration("runtime identity is not UTF-8"))?;
    validate_text(value).map_err(EncodedRenderError::Protocol)?;
    Ok(value.to_owned())
}

fn native_failure(error: impl std::fmt::Display) -> EncodedRenderError {
    EncodedRenderError::Protocol(format!("mapped runtime identity: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSdrSettings {
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
    pub gop_frames: u32,
    pub b_frames: u32,
    pub movie_timescale: u32,
}

impl From<&SdrPolicy> for ResolvedSdrSettings {
    fn from(value: &SdrPolicy) -> Self {
        Self {
            video_bitrate: value.video_bitrate,
            audio_bitrate: value.audio_bitrate,
            gop_frames: value.gop_frames,
            b_frames: value.b_frames,
            movie_timescale: value.movie_timescale,
        }
    }
}

/// Strict observations and expectations, not a deserializable encode permit.
/// Only consuming a fresh QualifiedEncoder exposes the bound host execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingBinding {
    pub schema_version: u32,
    pub policy_version: u32,
    pub raster: [u32; 2],
    pub frame_rate: [u32; 2],
    pub choice: EncoderChoice,
    pub settings: ResolvedSdrSettings,
    pub runtime: RuntimeFingerprint,
}

impl EncodingBinding {
    pub(crate) fn from_contract(
        contract: &EncodedRenderContract,
        runtime: RuntimeFingerprint,
    ) -> Result<Self, String> {
        let native = contract.native_contract()?;
        let binding = Self {
            schema_version: SCHEMA_VERSION,
            policy_version: SDR_POLICY_VERSION_V1,
            raster: native.raster(),
            frame_rate: native.frame_rate(),
            choice: contract.choice,
            settings: native.policy().into(),
            runtime,
        };
        binding.validate_for(contract)?;
        Ok(binding)
    }

    pub fn validate_for(&self, contract: &EncodedRenderContract) -> Result<(), String> {
        self.runtime.validate()?;
        let native = contract.native_contract()?;
        if self.schema_version != SCHEMA_VERSION
            || self.policy_version != SDR_POLICY_VERSION_V1
            || self.raster != native.raster()
            || self.frame_rate != native.frame_rate()
            || self.choice != contract.choice
            || self.settings != ResolvedSdrSettings::from(native.policy())
        {
            return Err(
                "encoding binding differs from frozen SDR controls or output geometry".into(),
            );
        }
        Ok(())
    }

    pub fn check_current(
        &self,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<RuntimeCapture, EncodedRenderError> {
        self.runtime
            .validate()
            .map_err(EncodedRenderError::Protocol)?;
        let current = capture(cancelled, deadline)?;
        if current.fingerprint != self.runtime {
            return Err(EncodedRenderError::Protocol(
                "loaded encoder runtime differs from its fresh probe".into(),
            ));
        }
        Ok(current)
    }
}

#[cfg(test)]
pub(crate) fn test_fingerprint() -> RuntimeFingerprint {
    use deadpan_encode::runtime::RuntimeFileTime;
    RuntimeFingerprint {
        schema_version: SCHEMA_VERSION,
        platform: RuntimePlatform {
            os_build: "25F84".into(),
            hardware_model: "TestMac1,1".into(),
            cpu_family: 1,
        },
        system: "Darwin".into(),
        kernel_release: "25.5.0".into(),
        kernel_build: "test-kernel".into(),
        machine: "arm64".into(),
        images: std::array::from_fn(|index| RuntimeImageFingerprint {
            mapped: MappedImageIdentity {
                kind: IMAGE_KINDS[index],
                device: 1,
                inode: index as u64 + 1,
                uuid: [index as u8 + 1; 16],
                file_size: 128,
                modification_time: RuntimeFileTime {
                    seconds: 1,
                    nanoseconds: 0,
                },
                change_time: RuntimeFileTime {
                    seconds: 1,
                    nanoseconds: 0,
                },
                birth_time: RuntimeFileTime {
                    seconds: 1,
                    nanoseconds: 0,
                },
                generation: 1,
            },
            sha256: Sha256::new("a".repeat(64)).unwrap(),
        }),
    }
}

#[cfg(test)]
pub(crate) fn test_binding(contract: &EncodedRenderContract) -> EncodingBinding {
    EncodingBinding::from_contract(contract, test_fingerprint()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicBool, time::Duration};

    #[test]
    fn runtime_hash_rejects_changed_extent_and_control_interruption() {
        let live = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(1);
        let expected = hash_image(&mut &b"abc"[..], 3, &live, deadline).unwrap();
        assert_eq!(
            expected.as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hash_image(&mut &b"abc"[..], 2, &live, deadline).is_err());
        assert!(hash_image(&mut &b"abc"[..], 4, &live, deadline).is_err());
        assert!(matches!(
            hash_image(&mut &b"abc"[..], 3, &AtomicBool::new(true), deadline),
            Err(EncodedRenderError::Cancelled)
        ));
        assert!(matches!(
            hash_image(&mut &b"abc"[..], 3, &live, Instant::now()),
            Err(EncodedRenderError::Deadline)
        ));
    }

    #[test]
    fn binding_rejects_policy_geometry_settings_and_inventory_changes() {
        let contract = crate::encoded_render::admission::ProbeSpec {
            raster: [320, 180],
            frame_rate: [30000, 1001],
            choice: EncoderChoice {
                mode: deadpan_encode::EncoderMode::Hardware,
                b_frames: deadpan_encode::BFramePolicy::None,
            },
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        }
        .contract()
        .unwrap();
        let valid = test_binding(&contract);
        for mutate in [
            (|value: &mut EncodingBinding| value.policy_version += 1) as fn(&mut EncodingBinding),
            |value| value.raster[0] += 2,
            |value| value.frame_rate = [30, 1],
            |value| value.settings.video_bitrate += 1,
            |value| value.settings.gop_frames += 1,
            |value| value.settings.movie_timescale += 1,
            |value| value.runtime.images.swap(0, 1),
            |value| value.runtime.images[0].mapped.file_size = MAX_IMAGE_BYTES + 1,
        ] {
            let mut changed = valid.clone();
            mutate(&mut changed);
            assert!(changed.validate_for(&contract).is_err());
        }
        let mut wire = serde_json::to_value(&valid).unwrap();
        wire["unused"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<EncodingBinding>(wire).is_err());
    }
}
