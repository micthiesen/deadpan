//! FFV1 configuration allocation admission, before opening an unsafe decoder.
//!
//! The bitstream rules and default probability table follow RFC 9043, sections
//! 3.8.1 and 4.1 through 4.3: <https://www.rfc-editor.org/rfc/rfc9043.html>.
//! This independent parser does not copy FFmpeg implementation code. Allocation
//! accounting is checked against FFmpeg 8.0.3's `ffv1_parse.c`, `ffv1.c`, and
//! `ffv1dec.c`. It is specific to one software decoder with frame threading off.
//! It bounds configuration-controlled state and scratch, not the entire decoder
//! heap, frame buffers, packet storage, or processing time of arbitrary frames.
//!
//! RFC-derived Code Components, including the probability table, carry this
//! Simplified BSD notice; the surrounding original admission code is MIT:
//!
//! Copyright (c) 2021 IETF Trust and the persons identified as authors of
//! RFC 9043. All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//!
//! 1. Redistributions of source code must retain the above copyright notice,
//!    this list of conditions and the following disclaimer.
//! 2. Redistributions in binary form must reproduce the above copyright notice,
//!    this list of conditions and the following disclaimer in the documentation
//!    and/or other materials provided with the distribution.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
//! AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
//! ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
//! LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
//! CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
//! SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
//! INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
//! CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
//! ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
//! POSSIBILITY OF SUCH DAMAGE.

use crate::SourceDecodeError;

const CONFIG_BYTES: usize = 64 * 1024;
const MAX_SLICES: u64 = 16;
const STATE_BYTES: u64 = 32 * 1024 * 1024;
const BINARY_OPERATIONS: u32 = 1_000_000;
const DEFAULT_TRANSITION: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37,
    37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 56, 57, 58, 59,
    60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 75, 76, 77, 78, 79, 80, 81, 82,
    83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103,
    104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 114, 115, 116, 117, 118, 119, 120, 121,
    122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 133, 134, 135, 136, 137, 138, 139,
    140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 152, 153, 154, 155, 156, 157,
    158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 171, 172, 173, 174, 175,
    176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 190, 191, 192, 194,
    194, 195, 196, 197, 198, 199, 200, 201, 202, 202, 204, 205, 206, 207, 208, 209, 209, 210, 211,
    212, 213, 215, 215, 216, 217, 218, 219, 220, 220, 222, 223, 224, 225, 226, 227, 227, 229, 229,
    230, 231, 232, 234, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248,
    248, 0, 0, 0, 0, 0, 0, 0,
];

type Result<T> = std::result::Result<T, SourceDecodeError>;

fn error(code: &'static str, message: &'static str) -> SourceDecodeError {
    SourceDecodeError::Native {
        code: code.into(),
        message: message.into(),
    }
}

fn require(value: bool, message: &'static str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(error("invalid_input", message))
    }
}

/// No input-sized allocation. Every binary decision is charged, including
/// decisions consuming no bytes. Closed-mode zero fill is limited to two bytes.
struct RangeBits<'a> {
    payload: &'a [u8],
    cursor: usize,
    span: u32,
    value: u32,
    operations: u32,
}

impl<'a> RangeBits<'a> {
    fn new(payload: &'a [u8]) -> Result<Self> {
        require(payload.len() >= 2, "truncated FFV1 range prefix")?;
        let value = u32::from(u16::from_be_bytes([payload[0], payload[1]]));
        require(value < 65280, "invalid FFV1 range prefix")?;
        Ok(Self {
            payload,
            cursor: 2,
            span: 65280,
            value,
            operations: 0,
        })
    }

    fn bit(&mut self, probability: &mut u8) -> Result<bool> {
        if self.operations == BINARY_OPERATIONS {
            return Err(error(
                "resource_limit",
                "FFV1 configuration work bound exceeded",
            ));
        }
        self.operations += 1;
        let ones = self.span * u32::from(*probability) / 256;
        require(
            ones > 0 && ones < self.span,
            "invalid FFV1 probability state",
        )?;
        let zero_end = self.span - ones;
        let one = self.value >= zero_end;
        if one {
            self.value -= zero_end;
            self.span = ones;
            *probability = DEFAULT_TRANSITION[usize::from(*probability)];
        } else {
            self.span = zero_end;
            let next = DEFAULT_TRANSITION[256 - usize::from(*probability)];
            require(next != 0, "invalid FFV1 probability transition")?;
            *probability = 0_u8.wrapping_sub(next);
        }
        if self.span < 256 {
            let next = if let Some(byte) = self.payload.get(self.cursor) {
                u32::from(*byte)
            } else {
                require(
                    self.cursor < self.payload.len() + 2,
                    "truncated FFV1 range stream",
                )?;
                0
            };
            self.cursor += 1;
            self.span *= 256;
            self.value = self.value * 256 + next;
        }
        Ok(one)
    }

    fn magnitude(&mut self, contexts: &mut [u8; 32]) -> Result<(u32, usize)> {
        if self.bit(&mut contexts[0])? {
            return Ok((0, 0));
        }
        let mut exponent = 0;
        while self.bit(&mut contexts[1 + exponent.min(9)])? {
            exponent += 1;
            require(exponent <= 30, "FFV1 scalar exceeds positive signed range")?;
        }
        let mut value = 1_u32;
        for index in (0..exponent).rev() {
            value = value * 2 + u32::from(self.bit(&mut contexts[22 + index.min(9)])?);
        }
        Ok((value, exponent))
    }

    fn unsigned(&mut self, contexts: &mut [u8; 32]) -> Result<u32> {
        self.magnitude(contexts).map(|(value, _)| value)
    }

    fn signed(&mut self, contexts: &mut [u8; 32]) -> Result<i64> {
        let (value, exponent) = self.magnitude(contexts)?;
        let negative = value != 0 && self.bit(&mut contexts[11 + exponent.min(10)])?;
        Ok(if negative {
            -i64::from(value)
        } else {
            i64::from(value)
        })
    }
}

// IEEE CRC, zero initial remainder, MSB first, no inversion (RFC 9043 §4.9.3).
fn crc(bytes: &[u8]) -> u32 {
    let mut remainder = 0_u32;
    for byte in bytes {
        remainder ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            let high = remainder >> 31;
            remainder = (remainder << 1) ^ (0x04c1_1db7 * high);
        }
    }
    remainder
}

fn context_count(bits: &mut RangeBits<'_>) -> Result<u32> {
    let mut scale = 1_u32;
    for _ in 0..5 {
        let mut contexts = [128; 32];
        let mut covered = 0_u32;
        let mut runs = 0_u32;
        while covered < 128 {
            let minus_one = bits.unsigned(&mut contexts)?;
            require(
                minus_one < 128 - covered,
                "FFV1 quantization run exceeds table",
            )?;
            covered += minus_one + 1;
            runs += 1;
        }
        // The pinned decoder rejects scale > 32768 before allocating states.
        // Scale is <= 32768 on entry and each factor is <= 255.
        scale *= 2 * runs - 1;
        require(
            scale <= 32768,
            "FFV1 quantization context count exceeds decoder bound",
        )?;
    }
    Ok(scale.div_ceil(2))
}

fn state_budget(
    counts: &[u32],
    slices: u64,
    planes: u64,
    range_coder: bool,
    max_width: u64,
) -> Result<()> {
    let total: u64 = counts.iter().map(|count| u64::from(*count)).sum();
    let largest = u64::from(*counts.iter().max().expect("validated nonempty table set"));
    // FFmpeg 8.0.3: initial_states uses 32 bytes/context. Each slice/plane
    // can select any table, allocating 32 bytes/context for range coding or
    // eight for VlcState. Both full-width sample buffers coexist: (width+6)
    // * 3 * MAX_PLANES(4) * (sizeof(i16)+sizeof(i32)) = 72*(width+6).
    // 16 KiB/slice and 1 MiB global conservatively cover fixed codec structs,
    // plane wrappers, quantization arrays, and allocator metadata. This is an
    // admission estimate, not a runtime allocator or a whole-process limit.
    let bytes = 32 * total
        + slices
            * (planes * largest * if range_coder { 32 } else { 8 }
                + 72 * (max_width + 6)
                + 16 * 1024)
        + 1024 * 1024;
    if bytes > STATE_BYTES {
        return Err(error(
            "resource_limit",
            "FFV1 configuration state and scratch exceed 32 MiB admission bound",
        ));
    }
    Ok(())
}

/// Admit the finite v3.4 configuration grammar and its allocation expansion.
/// Geometry is separately checked by the container guard. These validated hard
/// ceilings conservatively bound width-dependent scratch without trusting coded
/// dimensions, which are not present in an FFV1 configuration record.
pub(crate) fn validate_ffv1(bytes: &[u8], max_pixels: u64, max_dimension: u32) -> Result<()> {
    if !(1..=8192 * 8192).contains(&max_pixels) || !(1..=8192).contains(&max_dimension) {
        return Err(SourceDecodeError::InvalidConfiguration(
            "FFV1 geometry limits exceed hard bounds",
        ));
    }
    if bytes.len() > CONFIG_BYTES {
        return Err(error("resource_limit", "FFV1 configuration exceeds 64 KiB"));
    }
    require(
        bytes.len() >= 6,
        "FFV1 requires a complete v3 configuration record",
    )?;
    require(crc(bytes) == 0, "FFV1 configuration CRC mismatch")?;
    let mut bits = RangeBits::new(&bytes[..bytes.len() - 4])?;
    let mut parameters = [128; 32];
    if bits.unsigned(&mut parameters)? != 3 || bits.unsigned(&mut parameters)? != 4 {
        return Err(error(
            "unsupported_codec",
            "only FFV1 version 3.4 configurations are admitted",
        ));
    }
    let coder = bits.unsigned(&mut parameters)?;
    if coder > 2 {
        return Err(error("unsupported_codec", "unqualified FFV1 entropy coder"));
    }
    if coder == 2 {
        for baseline in DEFAULT_TRANSITION.iter().skip(1) {
            let transition = i64::from(*baseline) + bits.signed(&mut parameters)?;
            require(
                (1..=255).contains(&transition),
                "invalid FFV1 custom probability transition",
            )?;
        }
    }
    let colorspace = bits.unsigned(&mut parameters)?;
    let depth = bits.unsigned(&mut parameters)?;
    let chroma = bits.bit(&mut parameters[0])?;
    let horizontal_shift = bits.unsigned(&mut parameters)?;
    let vertical_shift = bits.unsigned(&mut parameters)?;
    let alpha = bits.bit(&mut parameters[0])?;
    require(
        colorspace <= 1 && depth <= 16,
        "unqualified FFV1 colorspace or sample depth",
    )?;
    require(
        horizontal_shift <= 4 && vertical_shift <= 4,
        "invalid FFV1 chroma shift",
    )?;
    require(
        colorspace == 0 || (chroma && horizontal_shift == 0 && vertical_shift == 0),
        "invalid FFV1 RGB plane layout",
    )?;
    let horizontal = u64::from(bits.unsigned(&mut parameters)?) + 1;
    let vertical = u64::from(bits.unsigned(&mut parameters)?) + 1;
    let slices = horizontal * vertical;
    if slices > MAX_SLICES
        || slices > max_pixels
        || horizontal > u64::from(max_dimension)
        || vertical > u64::from(max_dimension)
    {
        return Err(error(
            "resource_limit",
            "FFV1 slice raster exceeds admission bounds",
        ));
    }
    let tables = bits.unsigned(&mut parameters)?;
    require(
        (1..=8).contains(&tables),
        "invalid FFV1 quantization table count",
    )?;
    let mut counts = [0; 8];
    let counts = &mut counts[..tables as usize];
    for count in counts.iter_mut() {
        *count = context_count(&mut bits)?;
    }
    state_budget(
        counts,
        slices,
        2 + u64::from(alpha),
        coder != 0,
        u64::from(max_dimension).min(max_pixels),
    )?;
    let mut initial_contexts = [[128; 32]; 32];
    for count in counts {
        if bits.bit(&mut parameters[0])? {
            for _ in 0..*count {
                for contexts in &mut initial_contexts {
                    // Values affect probabilities, not allocation. Consume the
                    // signed deltas without materializing the expanded states.
                    bits.signed(contexts)?;
                }
            }
        }
    }
    require(
        bits.unsigned(&mut parameters)? <= 1,
        "unqualified FFV1 error correction mode",
    )?;
    require(
        bits.unsigned(&mut parameters)? <= 1,
        "invalid FFV1 intra-frame flag",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(error: SourceDecodeError) -> String {
        match error {
            SourceDecodeError::Native { code, .. } => code,
            other => panic!("unexpected error: {other}"),
        }
    }

    fn validate(bytes: &[u8]) -> Result<()> {
        validate_ffv1(bytes, 16_777_216, 8192)
    }

    fn fixtures() -> [(&'static str, &'static [u8]); 7] {
        [
            (
                "anamorphic",
                include_bytes!("../tests/fixtures/anamorphic.mkv"),
            ),
            ("full709", include_bytes!("../tests/fixtures/full709.mkv")),
            ("hdr-pq", include_bytes!("../tests/fixtures/hdr-pq.mkv")),
            (
                "interlaced",
                include_bytes!("../tests/fixtures/interlaced.mkv"),
            ),
            (
                "limited709",
                include_bytes!("../tests/fixtures/limited709.mkv"),
            ),
            (
                "sdr-with-stream-hdr",
                include_bytes!("../tests/fixtures/sdr-with-stream-hdr.mkv"),
            ),
            ("ten-bit", include_bytes!("../tests/fixtures/ten-bit.mkv")),
        ]
    }

    // These small committed fixtures have one CodecPrivate. Container traversal
    // is tested separately; here only its exact retained configuration is used.
    fn private(bytes: &[u8]) -> &[u8] {
        let at = bytes
            .windows(2)
            .position(|bytes| bytes == [0x63, 0xa2])
            .unwrap()
            + 2;
        let width = bytes[at].leading_zeros() as usize + 1;
        assert!(width <= 8);
        let mut length = usize::from(bytes[at] & (0xff_u8 >> width));
        for byte in &bytes[at + 1..at + width] {
            length = length * 256 + usize::from(*byte);
        }
        &bytes[at + width..at + width + length]
    }

    #[test]
    fn committed_ffv1_configurations_admit_including_semantic_negatives() {
        for (name, bytes) in fixtures() {
            let config = private(bytes);
            assert!(matches!(config.len(), 42 | 200));
            validate(config).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }

    #[test]
    fn every_truncation_and_configuration_corruption_is_rejected() {
        for (_, fixture) in fixtures() {
            let config = private(fixture);
            for length in 0..config.len() {
                assert!(
                    validate(&config[..length]).is_err(),
                    "accepted prefix {length}"
                );
            }
            for index in 0..config.len() {
                let mut changed = config.to_vec();
                changed[index] ^= 1;
                assert_eq!(code(validate(&changed).unwrap_err()), "invalid_input");
            }
        }
    }

    // Test-only reference encoder. It records the RFC interval partition for
    // each desired bit, then solves those transformations backward from a final
    // value of zero. This avoids copying an implementation's carry-flush coder.
    // Only bounded synthetic headers are emitted, never executable media.
    struct Partition {
        zero_end: u32,
        one: bool,
        refill: bool,
    }

    struct Encoder {
        span: u32,
        partitions: Vec<Partition>,
    }

    impl Encoder {
        fn new() -> Self {
            Self {
                span: 65280,
                partitions: Vec::new(),
            }
        }

        fn bit(&mut self, probability: &mut u8, one: bool) {
            assert!(self.partitions.len() < 5_000_000);
            let ones = self.span * u32::from(*probability) / 256;
            let zero_end = self.span - ones;
            assert!(ones > 0 && ones < self.span);
            self.span = if one { ones } else { zero_end };
            *probability = if one {
                DEFAULT_TRANSITION[usize::from(*probability)]
            } else {
                0_u8.wrapping_sub(DEFAULT_TRANSITION[256 - usize::from(*probability)])
            };
            let refill = self.span < 256;
            self.partitions.push(Partition {
                zero_end,
                one,
                refill,
            });
            if refill {
                self.span *= 256;
            }
        }

        fn magnitude(&mut self, contexts: &mut [u8; 32], value: u32) -> usize {
            self.bit(&mut contexts[0], value == 0);
            if value == 0 {
                return 0;
            }
            let exponent = (31 - value.leading_zeros()) as usize;
            for index in 0..exponent {
                self.bit(&mut contexts[1 + index.min(9)], true);
            }
            self.bit(&mut contexts[1 + exponent.min(9)], false);
            for index in (0..exponent).rev() {
                self.bit(&mut contexts[22 + index.min(9)], value & (1 << index) != 0);
            }
            exponent
        }

        fn unsigned(&mut self, contexts: &mut [u8; 32], value: u32) {
            self.magnitude(contexts, value);
        }

        fn signed(&mut self, contexts: &mut [u8; 32], value: i32) {
            let exponent = self.magnitude(contexts, value.unsigned_abs());
            if value != 0 {
                self.bit(&mut contexts[11 + exponent.min(10)], value < 0);
            }
        }

        fn finish(self) -> Vec<u8> {
            let mut value = 0_u32;
            let mut reversed = Vec::new();
            for partition in self.partitions.into_iter().rev() {
                if partition.refill {
                    reversed.push((value & 255) as u8);
                    value /= 256;
                }
                if partition.one {
                    value += partition.zero_end;
                }
            }
            let mut bytes = u16::try_from(value).unwrap().to_be_bytes().to_vec();
            bytes.extend(reversed.into_iter().rev());
            seal(&mut bytes);
            bytes
        }
    }

    fn seal(bytes: &mut Vec<u8>) {
        bytes.extend(crc(bytes).to_be_bytes());
        assert_eq!(crc(bytes), 0);
    }

    #[test]
    fn range_scalars_preserve_signed_values_and_reject_exponent_overflow() {
        let values = [0, 1, -1, 127, -128, 65535, -65535, i32::MAX, -i32::MAX];
        let mut encoder = Encoder::new();
        let mut contexts = [128; 32];
        for value in values {
            encoder.signed(&mut contexts, value);
        }
        let bytes = encoder.finish();
        let mut reader = RangeBits::new(&bytes[..bytes.len() - 4]).unwrap();
        let mut contexts = [128; 32];
        for value in values {
            assert_eq!(reader.signed(&mut contexts).unwrap(), i64::from(value));
        }
        let mut encoder = Encoder::new();
        encoder.unsigned(&mut [128; 32], 1 << 31);
        let bytes = encoder.finish();
        let mut reader = RangeBits::new(&bytes[..bytes.len() - 4]).unwrap();
        assert_eq!(
            code(reader.unsigned(&mut [128; 32]).unwrap_err()),
            "invalid_input"
        );
    }

    #[derive(Clone)]
    struct Header {
        version: u32,
        micro: u32,
        coder: u32,
        colorspace: u32,
        depth: u32,
        chroma: bool,
        horizontal_shift: u32,
        vertical_shift: u32,
        alpha: bool,
        horizontal: u32,
        vertical: u32,
        tables: Vec<[u32; 5]>,
        initial: bool,
        ec: u32,
        intra: u32,
        invalid_transition: bool,
    }

    impl Default for Header {
        fn default() -> Self {
            Self {
                version: 3,
                micro: 4,
                coder: 1,
                colorspace: 0,
                depth: 8,
                chroma: true,
                horizontal_shift: 1,
                vertical_shift: 1,
                alpha: false,
                horizontal: 2,
                vertical: 2,
                tables: vec![[6, 6, 6, 1, 1]],
                initial: false,
                ec: 1,
                intra: 0,
                invalid_transition: false,
            }
        }
    }

    impl Header {
        fn encode(&self) -> Vec<u8> {
            let mut encoder = Encoder::new();
            let mut parameters = [128; 32];
            encoder.unsigned(&mut parameters, self.version);
            encoder.unsigned(&mut parameters, self.micro);
            encoder.unsigned(&mut parameters, self.coder);
            if self.coder == 2 {
                for (index, baseline) in DEFAULT_TRANSITION.iter().enumerate().skip(1) {
                    let next = if index == 1 && self.invalid_transition {
                        256
                    } else {
                        i32::from(*baseline).clamp(1, 255)
                    };
                    encoder.signed(&mut parameters, next - i32::from(*baseline));
                }
            }
            encoder.unsigned(&mut parameters, self.colorspace);
            encoder.unsigned(&mut parameters, self.depth);
            encoder.bit(&mut parameters[0], self.chroma);
            encoder.unsigned(&mut parameters, self.horizontal_shift);
            encoder.unsigned(&mut parameters, self.vertical_shift);
            encoder.bit(&mut parameters[0], self.alpha);
            encoder.unsigned(&mut parameters, self.horizontal - 1);
            encoder.unsigned(&mut parameters, self.vertical - 1);
            encoder.unsigned(&mut parameters, self.tables.len() as u32);
            let mut counts = Vec::new();
            for table in &self.tables {
                let mut scale = 1;
                for runs in table {
                    assert!((1..=128).contains(runs));
                    let mut contexts = [128; 32];
                    for _ in 1..*runs {
                        encoder.unsigned(&mut contexts, 0);
                    }
                    encoder.unsigned(&mut contexts, 128 - runs);
                    scale *= 2 * runs - 1;
                }
                counts.push(scale.div_ceil(2));
            }
            let mut initial_contexts = [[128; 32]; 32];
            for count in counts {
                encoder.bit(&mut parameters[0], self.initial);
                if self.initial {
                    for _ in 0..count {
                        for contexts in &mut initial_contexts {
                            encoder.signed(contexts, 0);
                        }
                    }
                }
            }
            encoder.unsigned(&mut parameters, self.ec);
            encoder.unsigned(&mut parameters, self.intra);
            encoder.finish()
        }
    }

    #[test]
    fn valid_synthesized_configurations_cover_coders_and_initial_states() {
        for coder in 0..=2 {
            for initial in [false, true] {
                let header = Header {
                    coder,
                    initial,
                    ..Header::default()
                };
                validate(&header.encode()).unwrap();
            }
        }
        validate(
            &Header {
                depth: 10,
                ..Header::default()
            }
            .encode(),
        )
        .unwrap();
        validate(
            &Header {
                alpha: true,
                ..Header::default()
            }
            .encode(),
        )
        .unwrap();
    }

    #[test]
    fn valid_large_slice_raster_and_context_expansion_fail_before_codec_open() {
        let header = Header {
            horizontal: 8,
            vertical: 8,
            ..Header::default()
        };
        assert_eq!(
            code(validate(&header.encode()).unwrap_err()),
            "resource_limit"
        );
        // Two 91-run tables produce (181*181 + 1)/2 = 16381 contexts.
        // This is valid in FFmpeg, but sixteen alpha/range-coded slices plus
        // width-dependent scratch exceed this boundary's 32 MiB estimate.
        let mut header = Header {
            tables: vec![[91, 91, 1, 1, 1]],
            alpha: true,
            horizontal: 1,
            vertical: 1,
            ..Header::default()
        };
        validate(&header.encode()).unwrap();
        header.horizontal = 16;
        assert_eq!(
            code(validate(&header.encode()).unwrap_err()),
            "resource_limit"
        );
        header.coder = 0;
        validate(&header.encode()).unwrap();
    }

    #[test]
    fn unsupported_versions_and_reserved_fields_do_not_reach_codec_open() {
        for version in [0, 1, 2, 4, 5] {
            assert_eq!(
                code(
                    validate(
                        &Header {
                            version,
                            ..Header::default()
                        }
                        .encode()
                    )
                    .unwrap_err()
                ),
                "unsupported_codec"
            );
        }
        for micro in [0, 3, 5, 65536] {
            assert_eq!(
                code(
                    validate(
                        &Header {
                            micro,
                            ..Header::default()
                        }
                        .encode()
                    )
                    .unwrap_err()
                ),
                "unsupported_codec"
            );
        }
        assert_eq!(
            code(
                validate(
                    &Header {
                        coder: 3,
                        ..Header::default()
                    }
                    .encode()
                )
                .unwrap_err()
            ),
            "unsupported_codec"
        );
        for header in [
            Header {
                colorspace: 2,
                ..Header::default()
            },
            Header {
                depth: 32,
                ..Header::default()
            },
            Header {
                horizontal_shift: 5,
                ..Header::default()
            },
            Header {
                vertical_shift: 5,
                ..Header::default()
            },
            Header {
                colorspace: 1,
                ..Header::default()
            },
            Header {
                ec: 2,
                ..Header::default()
            },
            Header {
                intra: 2,
                ..Header::default()
            },
            Header {
                tables: vec![],
                ..Header::default()
            },
            Header {
                tables: vec![[1; 5]; 9],
                ..Header::default()
            },
            Header {
                tables: vec![[128, 128, 1, 1, 1]],
                ..Header::default()
            },
            Header {
                coder: 2,
                invalid_transition: true,
                ..Header::default()
            },
        ] {
            assert_eq!(
                code(validate(&header.encode()).unwrap_err()),
                "invalid_input"
            );
        }
    }

    #[test]
    fn compact_expanded_initial_states_have_a_binary_work_ceiling() {
        let header = Header {
            tables: vec![[64, 64, 1, 1, 1]; 8],
            initial: true,
            ..Header::default()
        };
        let bytes = header.encode();
        assert!(bytes.len() < CONFIG_BYTES);
        assert_eq!(code(validate(&bytes).unwrap_err()), "resource_limit");
    }

    #[test]
    fn truncated_range_streams_recomputed_crcs_and_extreme_inputs_are_bounded() {
        let bytes = Header::default().encode();
        for length in 0..bytes.len() - 4 {
            let mut prefix = bytes[..length].to_vec();
            seal(&mut prefix);
            assert!(
                validate(&prefix).is_err(),
                "accepted incomplete parameters at {length}"
            );
        }
        for value in [0, 0xff] {
            let mut bytes = vec![value; CONFIG_BYTES - 4];
            seal(&mut bytes);
            assert!(validate(&bytes).is_err());
        }
        assert_eq!(
            code(validate(&vec![0; CONFIG_BYTES + 1]).unwrap_err()),
            "resource_limit"
        );
        let mut bits = RangeBits::new(&[0; 16]).unwrap();
        bits.operations = BINARY_OPERATIONS;
        assert_eq!(code(bits.bit(&mut 128).unwrap_err()), "resource_limit");
    }
}
