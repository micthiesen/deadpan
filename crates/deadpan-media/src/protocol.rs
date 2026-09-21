//! Small versioned wire contract for the app-managed codec helper.
//!
//! Deserialization checks structure; both endpoints must call `validate` before
//! using numeric fields. The contract deliberately supports only the qualified
//! full-range RGB8 / sRGB / BT.709 generated-video route.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REPLY_BYTES: usize = 8192;
pub const MAX_REQUEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoContract {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub rate_num: u32,
    pub rate_den: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid media contract: {0}")]
pub struct ContractError(pub &'static str);

impl VideoContract {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.width == 0 || self.height == 0 || self.width > 4096 || self.height > 4096 {
            return Err(ContractError("dimensions must be between 1 and 4096"));
        }
        if self.frames == 0 || self.frames > 10_000 {
            return Err(ContractError("frame count must be between 1 and 10000"));
        }
        if self.rate_num == 0
            || self.rate_den == 0
            || self.rate_num > i32::MAX as u32
            || self.rate_den > i32::MAX as u32
            || self.rate_num < self.rate_den
            || u64::from(self.rate_num) > 240 * u64::from(self.rate_den)
        {
            return Err(ContractError(
                "frame rate must be representable and between 1 and 240",
            ));
        }
        let mut a = self.rate_num;
        let mut b = self.rate_den;
        while b != 0 {
            (a, b) = (b, a % b);
        }
        if a != 1 {
            return Err(ContractError("frame rate must be reduced"));
        }
        Ok(())
    }

    pub fn scratch_bytes(&self) -> Result<u64, ContractError> {
        self.validate()?;
        Ok(u64::from(self.width) * u64::from(self.height) * 3 * u64::from(self.frames))
    }

    /// Exact CFR positions rounded once to Matroska's millisecond clock.
    pub fn matroska_pts(&self, ordinal: u32) -> Result<i64, ContractError> {
        self.validate()?;
        if ordinal >= self.frames {
            return Err(ContractError("frame ordinal is outside the sequence"));
        }
        let numerator = u64::from(ordinal) * u64::from(self.rate_den) * 1000;
        i64::try_from((numerator + u64::from(self.rate_num) / 2) / u64::from(self.rate_num))
            .map_err(|_| ContractError("timestamp overflow"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionLimits {
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_scratch_bytes: u64,
    pub timeout_ms: u64,
}

impl ConversionLimits {
    pub fn validate(&self) -> Result<(), ContractError> {
        if [
            self.max_input_bytes,
            self.max_output_bytes,
            self.max_scratch_bytes,
        ]
        .into_iter()
        .any(|bytes| bytes == 0 || bytes > MAX_FILE_BYTES)
        {
            return Err(ContractError(
                "file budgets must be positive and at most 16 GiB",
            ));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 24 * 60 * 60 * 1000 {
            return Err(ContractError(
                "deadline must be positive and at most 24 hours",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionRequest {
    pub protocol: u32,
    pub video: VideoContract,
    pub input_byte_length: u64,
    pub limits: ConversionLimits,
}

impl ConversionRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ContractError("unsupported protocol version"));
        }
        self.video.validate()?;
        self.limits.validate()?;
        if self.input_byte_length == 0 || self.input_byte_length > self.limits.max_input_bytes {
            return Err(ContractError("input length exceeds budget or is empty"));
        }
        if self.video.scratch_bytes()? > self.limits.max_scratch_bytes {
            return Err(ContractError("decoded sequence exceeds scratch budget"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionReport {
    pub protocol: u32,
    pub video: VideoContract,
    pub output_bytes: u64,
    pub input_rgb_sha256: String,
    pub output_rgb_sha256: String,
    pub input_time_base_num: u32,
    pub input_time_base_den: u32,
    pub output_time_base_num: u32,
    pub output_time_base_den: u32,
    pub first_output_pts: i64,
    pub last_output_pts: i64,
    pub ffv1_version: u32,
    pub slice_crc: bool,
    pub discarded_audio_streams: u32,
}

impl ConversionReport {
    pub fn validate(&self, request: &ConversionRequest) -> Result<(), ContractError> {
        request.validate()?;
        if self.protocol != PROTOCOL_VERSION || self.video != request.video {
            return Err(ContractError("worker changed the requested video contract"));
        }
        if self.output_bytes == 0 || self.output_bytes > request.limits.max_output_bytes {
            return Err(ContractError("output size is empty or exceeds budget"));
        }
        if !is_sha256(&self.input_rgb_sha256) || self.input_rgb_sha256 != self.output_rgb_sha256 {
            return Err(ContractError(
                "decoded pixel identities differ or are invalid",
            ));
        }
        if self.input_time_base_num == 0
            || self.input_time_base_den == 0
            || self.input_time_base_num > i32::MAX as u32
            || self.input_time_base_den > i32::MAX as u32
            || self.output_time_base_num != 1
            || self.output_time_base_den != 1000
            || self.first_output_pts != 0
            || self.last_output_pts != self.video.matroska_pts(self.video.frames - 1)?
        {
            return Err(ContractError("invalid observed media clocks"));
        }
        if self.ffv1_version != 3 || !self.slice_crc || self.discarded_audio_streams > 7 {
            return Err(ContractError("unexpected codec profile or stream count"));
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerReply {
    Success { report: ConversionReport },
    Failure { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video() -> VideoContract {
        VideoContract {
            width: 768,
            height: 320,
            frames: 30,
            rate_num: 30000,
            rate_den: 1001,
        }
    }

    #[test]
    fn exact_clock_rounds_each_origin_based_position_once() {
        let video = video();
        assert_eq!(video.matroska_pts(0).unwrap(), 0);
        assert_eq!(video.matroska_pts(29).unwrap(), 968);
        assert_eq!(video.scratch_bytes().unwrap(), 22_118_400);
        assert!(video.matroska_pts(30).is_err());
        assert!(
            VideoContract {
                rate_num: 60,
                rate_den: 2,
                ..video
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoContract {
                rate_num: 0,
                ..video
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoContract {
                width: 4097,
                ..video
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn wire_rejects_unknown_and_duplicate_fields() {
        let wire = serde_json::to_string(&video()).unwrap();
        assert!(
            serde_json::from_str::<VideoContract>(&wire.replacen('{', "{\"width\":1,", 1)).is_err()
        );
        assert!(
            serde_json::from_str::<VideoContract>(&wire.replacen('{', "{\"path\":\"evil\",", 1))
                .is_err()
        );
    }
}
