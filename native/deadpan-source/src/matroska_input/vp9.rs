//! Check AV1 container interpretation and resolve VP9 from its first key.
//! The native guard subsequently checks every packet, including hidden frames.
use super::{Result, require};
use crate::{
    ColorMetadata, ContentLight, IgnoredStaticMetadata, MasteringDisplay, Mp4ColorDescription,
    Mp4Vp9Configuration,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Video {
    pub width: u64,
    pub height: u64,
    pub fields: BTreeMap<u32, u64>,
    pub mastering: Option<BTreeMap<u32, f64>>,
}

/// VP9 has no in-band static HDR declarations. Preserve the single container
/// declaration before FFmpeg can omit partial/zero values or narrow integers.
#[derive(Debug, Default)]
pub(crate) struct StaticMetadata {
    mastering: Option<MasteringDisplay>,
    content_light: Option<ContentLight>,
    ignored: IgnoredStaticMetadata,
}
impl StaticMetadata {
    pub(crate) fn apply(self, color: &mut ColorMetadata) -> Result<()> {
        if !color.transfer.is_hdr()
            && (self.mastering.is_some()
                || self.content_light.is_some()
                || !self.ignored.is_empty())
        {
            return Err(super::error(
                "unsupported_hdr",
                "SDR VP9 carries HDR static metadata",
            ));
        }
        color.mastering = self.mastering;
        color.content_light = self.content_light;
        color.ignored_static = self.ignored;
        Ok(())
    }
}

impl Video {
    pub fn static_metadata(&self) -> StaticMetadata {
        let mut result = StaticMetadata::default();
        if let Some(fields) = &self.mastering {
            let parse = || {
                let units = |id, scale: f64, maximum: u32| {
                    let value = *fields.get(&id)?;
                    let scaled = (value * scale).round();
                    (scaled >= 0.0 && scaled <= f64::from(maximum) && scaled / scale == value)
                        .then_some(scaled as u32)
                };
                let xy = |id| u16::try_from(units(id, 50_000.0, 50_000)?).ok();
                Some(MasteringDisplay {
                    primaries: [
                        [xy(0x55d1)?, xy(0x55d2)?],
                        [xy(0x55d3)?, xy(0x55d4)?],
                        [xy(0x55d5)?, xy(0x55d6)?],
                    ],
                    white_point: [xy(0x55d7)?, xy(0x55d8)?],
                    max_luminance: units(0x55d9, 10_000.0, 100_000_000)?,
                    min_luminance: units(0x55da, 10_000.0, 500_000)?,
                })
            };
            result.mastering = parse().filter(MasteringDisplay::is_valid);
            result.ignored.mastering = result.mastering.is_none();
        }
        if self.fields.contains_key(&0x55bc) || self.fields.contains_key(&0x55bd) {
            let parse = || {
                Some(ContentLight {
                    max_cll: u16::try_from(*self.fields.get(&0x55bc)?).ok()?,
                    max_fall: u16::try_from(*self.fields.get(&0x55bd)?).ok()?,
                })
            };
            result.content_light = parse().filter(ContentLight::is_valid);
            result.ignored.content_light = result.content_light.is_none();
        }
        result
    }

    pub fn av1_configuration(&self, header: &[u8]) -> Result<()> {
        crate::input::av1_configuration_prefix(header)?;
        self.layout()?;
        let get = |id| self.fields.get(&id).copied();
        let depth = if header[2] & 64 == 0 { 8 } else { 10 };
        let vertical_chroma = if header[2] & 3 == 1 { 2 } else { 1 };
        require(
            get(0x55b2).is_none_or(|v| v == depth)
                && [0x55b3, 0x55b4]
                    .iter()
                    .all(|id| get(*id).is_none_or(|v| v == 1))
                && [0x55b5, 0x55b6]
                    .iter()
                    .all(|id| get(*id).is_none_or(|v| v == 0))
                && get(0x55b7).is_none_or(|v| v == 1)
                && get(0x55b8).is_none_or(|v| v == vertical_chroma),
            "AV1 Matroska chroma/depth and av1C disagree",
        )?;
        // Optional container declarations must be interpreted, not discarded
        // by the demuxer. The native guard compares them to the sequence OBU.
        require(
            get(0x55b1).is_none_or(|v| matches!(v, 1 | 5 | 6 | 9))
                && get(0x55b9).is_none_or(|v| matches!(v, 1 | 2))
                && get(0x55ba).is_none_or(|v| matches!(v, 1 | 8 | 13 | 16 | 18))
                && get(0x55bb).is_none_or(|v| matches!(v, 1 | 9 | 12)),
            "unqualified Matroska AV1 color interpretation",
        )
    }

    fn layout(&self) -> Result<()> {
        let get = |id| self.fields.get(&id).copied();
        require(
            matches!(get(0x9a), None | Some(0 | 2))
                && matches!(get(0x9d), None | Some(0 | 2))
                && [0x53b8, 0x53c0, 0x54aa, 0x54bb, 0x54cc, 0x54dd]
                    .iter()
                    .all(|id| get(*id).is_none_or(|value| value == 0)),
            "unqualified Matroska video field, stereo, alpha or crop interpretation",
        )?;
        require(
            matches!(get(0x54b2), None | Some(0 | 3 | 4))
                && [0x54b0, 0x54ba]
                    .iter()
                    .all(|id| get(*id).is_none_or(|value| value > 0 && value <= i32::MAX as u64))
                && (get(0x54b2) != Some(4) || (get(0x54b0).is_none() && get(0x54ba).is_none())),
            "unqualified Matroska video display dimensions or units",
        )?;
        require(
            get(0x54b2) != Some(3) || (get(0x54b0).is_some() && get(0x54ba).is_some()),
            "unqualified Matroska video display dimensions or units",
        )
    }

    pub fn configuration(&self, header: &[u8]) -> Result<Mp4Vp9Configuration> {
        let mut bits = Bits {
            data: header,
            at: 0,
        };
        require(bits.read(2)? == 2, "invalid VP9 frame marker")?;
        let profile = bits.read(1)? | (bits.read(1)? << 1);
        require(matches!(profile, 0 | 2), "unqualified Matroska VP9 profile")?;
        require(
            bits.read(1)? == 0,
            "Matroska VP9 must begin with a keyframe",
        )?;
        require(
            bits.read(1)? == 0,
            "Matroska VP9 must begin with a keyframe",
        )?;
        bits.read(2)?; // show_frame and error_resilient_mode
        require(bits.read(24)? == 0x498342, "invalid VP9 keyframe sync")?;
        let depth = if profile == 2 {
            10 + 2 * bits.read(1)?
        } else {
            8
        };
        require(depth <= 10, "twelve-bit VP9 is not qualified")?;
        let matrix = [2, 5, 1, 6, 7, 9, 3, 0][bits.read(3)? as usize];
        let full_range = bits.read(1)? != 0;
        let width = u64::from(bits.read(16)?) + 1;
        let height = u64::from(bits.read(16)?) + 1;
        require(
            (width, height) == (self.width, self.height),
            "VP9 keyframe and Matroska raster disagree",
        )?;
        if bits.read(1)? != 0 {
            require(
                u64::from(bits.read(16)?) + 1 == width && u64::from(bits.read(16)?) + 1 == height,
                "VP9 render size differs from its coded raster",
            )?;
        }
        self.layout()?;
        let get = |id| self.fields.get(&id).copied();
        require(
            get(0x55b1) == Some(matrix)
                && get(0x55b2).is_none_or(|value| value == u64::from(depth))
                && get(0x55b9) == Some(if full_range { 2 } else { 1 }),
            "VP9 keyframe and Matroska color interpretation disagree",
        )?;
        // Subsampling is specified by profiles 0/2. Chroma siting must be
        // explicit; do not invent a phase when the container omits it.
        require(
            [0x55b3, 0x55b4]
                .iter()
                .all(|id| get(*id).is_none_or(|v| v == 1))
                && [0x55b5, 0x55b6]
                    .iter()
                    .all(|id| get(*id).is_none_or(|v| v == 0))
                && get(0x55b7) == Some(1)
                && matches!(get(0x55b8), Some(1 | 2)),
            "Matroska VP9 needs explicit supported chroma siting",
        )?;
        let transfer = get(0x55ba);
        let primaries = get(0x55bb);
        require(
            if matches!(transfer, Some(16 | 18)) {
                depth == 10 && !full_range && matrix == 9 && primaries == Some(9)
            } else {
                matches!(transfer, Some(1 | 8 | 13)) && matches!(primaries, Some(1 | 9 | 12))
            },
            "Matroska VP9 needs qualified SDR color or ten-bit limited-range BT.2020 NCL PQ/HLG",
        )?;
        Ok(Mp4Vp9Configuration {
            profile: profile as u8,
            level: 0, // No level claim in this container binding.
            bit_depth: depth as u8,
            chroma_subsampling: u8::from(get(0x55b8) == Some(1)),
            color: Mp4ColorDescription {
                primaries: primaries.expect("checked primaries") as u16,
                transfer: transfer.expect("checked transfer") as u16,
                matrix: matrix as u16,
                full_range,
                range_byte: if full_range { 128 } else { 0 },
            },
        })
    }
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn read(&mut self, count: usize) -> Result<u32> {
        require(
            self.at + count <= self.data.len() * 8,
            "truncated VP9 keyframe header",
        )?;
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | u32::from((self.data[self.at / 8] >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(profile: u32, twelve_bit: bool, width: u32, height: u32) -> Vec<u8> {
        let mut fields = vec![
            (2, 2),
            (profile & 1, 1),
            (profile >> 1, 1),
            (0, 1),
            (0, 1),
            (1, 1),
            (0, 1),
            (0x498342, 24),
        ];
        if profile == 2 {
            fields.push((u32::from(twelve_bit), 1));
        }
        fields.extend([(2, 3), (0, 1), (width - 1, 16), (height - 1, 16), (0, 1)]);
        let mut data = Vec::new();
        let mut at = 0;
        for (value, count) in fields {
            for shift in (0..count).rev() {
                if at % 8 == 0 {
                    data.push(0);
                }
                data[at / 8] |= (((value >> shift) & 1) as u8) << (7 - at % 8);
                at += 1;
            }
        }
        data
    }

    fn video() -> Video {
        Video {
            width: 96,
            height: 64,
            fields: BTreeMap::from([
                (0x55b1, 1),
                (0x55b9, 1),
                (0x55ba, 1),
                (0x55bb, 1),
                (0x55b7, 1),
                (0x55b8, 2),
            ]),
            mastering: None,
        }
    }

    #[test]
    fn hdr_requires_ten_bit_limited_bt2020_ncl_and_preserves_static_declarations() {
        for transfer in [16, 18] {
            let mut video = video();
            video.fields.insert(0x55b1, 9);
            video.fields.insert(0x55ba, transfer);
            video.fields.insert(0x55bb, 9);
            let mut hdr_key = key(2, false, 96, 64);
            hdr_key[4] = (hdr_key[4] & 0x8f) | 0x50; // colorspace BT.2020
            assert_eq!(
                video.configuration(&hdr_key).unwrap().color.transfer,
                transfer as u16
            );
            assert!(video.configuration(&key(0, false, 96, 64)).is_err());
            for (id, invalid) in [(0x55b1, 1), (0x55b9, 2), (0x55bb, 1), (0x9a, 1)] {
                let old = video.fields.insert(id, invalid);
                assert!(video.configuration(&hdr_key).is_err(), "{id:x}");
                if let Some(value) = old {
                    video.fields.insert(id, value);
                } else {
                    video.fields.remove(&id);
                }
            }
            video.fields.insert(0x55bc, u64::from(u32::MAX) + 1001);
            video.fields.insert(0x55bd, 400);
            let meta = video.static_metadata();
            assert!(meta.ignored.content_light);
            assert!(meta.content_light.is_none());
            video.fields.insert(0x55bc, 0);
            video.fields.insert(0x55bd, 0);
            assert_eq!(
                video.static_metadata().content_light,
                Some(ContentLight {
                    max_cll: 0,
                    max_fall: 0
                })
            );
            video.fields.remove(&0x55bd);
            assert!(video.static_metadata().ignored.content_light);
            video.mastering = Some(BTreeMap::new());
            assert!(video.static_metadata().ignored.mastering);
        }
    }

    #[test]
    fn first_key_bounds_profile_depth_dimensions_and_every_header_bit() {
        for profile in [0, 2] {
            let key = key(profile, false, 96, 64);
            assert_eq!(
                video().configuration(&key).unwrap().bit_depth,
                if profile == 0 { 8 } else { 10 }
            );
            for length in 0..key.len() {
                assert!(video().configuration(&key[..length]).is_err());
            }
        }
        for key in [
            key(1, false, 96, 64),
            key(3, false, 96, 64),
            key(2, true, 96, 64),
            key(0, false, 97, 64),
            key(0, false, 96, 65),
        ] {
            assert!(video().configuration(&key).is_err());
        }
        let mut existing = key(0, false, 96, 64);
        existing[0] |= 8;
        assert!(video().configuration(&existing).is_err());
    }

    #[test]
    fn conflicting_subsampling_and_ignored_display_metadata_are_refused() {
        let key = key(0, false, 96, 64);
        for (id, value) in [
            (0x53b8, 1),
            (0x53c0, 1),
            (0x54aa, 1),
            (0x54bb, 1),
            (0x54cc, 1),
            (0x54dd, 1),
            (0x55b2, 10),
            (0x55b3, 0),
            (0x55b4, 0),
            (0x55b5, 1),
            (0x55b6, 1),
            (0x54b2, 1),
            (0x54b0, u64::MAX),
        ] {
            let mut video = video();
            video.fields.insert(id, value);
            assert!(video.configuration(&key).is_err(), "{id:x} {value}");
        }
        let mut video = video();
        video.fields.extend([(0x54b2, 4), (0x54b0, 96)]);
        assert!(video.configuration(&key).is_err());
        video.fields.insert(0x54b2, 3);
        assert!(video.configuration(&key).is_err());
    }
}
