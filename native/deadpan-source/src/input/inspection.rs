//! Observations from admitted MP4 tables and descriptor-backed AVC samples.
//! This does not decode media, establish closed GOPs, or authorize publication.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mp4TrackKind {
    Video,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4Edit {
    /// In the movie time scale, not the track's media time scale.
    pub segment_duration: u64,
    /// Signed track media ticks; -1 denotes the source grammar's empty edit.
    pub media_time: i64,
    pub media_rate_integer: i16,
    pub media_rate_fraction: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4AvcConfiguration {
    pub profile: u8,
    pub compatibility: u8,
    pub level: u8,
    pub nal_length_bytes: u8,
}

/// Admitted hvcC fields. Admission requires profile space 0, Main10
/// (profile_idc 2), 4:2:0 (chroma_format_idc 1), ten-bit luma and chroma,
/// a 1, 2 or 4 byte NAL length and complete nonempty VPS/SPS/PPS arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4HevcConfiguration {
    pub profile_space: u8,
    pub tier: u8,
    pub profile_idc: u8,
    pub level_idc: u8,
    pub chroma_format_idc: u8,
    pub bit_depth_luma: u8,
    pub bit_depth_chroma: u8,
    pub nal_length_bytes: u8,
    pub vps_count: u8,
    pub sps_count: u8,
    pub pps_count: u8,
    /// Prefix and suffix SEI units carried in the configuration.
    pub sei_count: u8,
    /// First SPS `pic_width/height_in_luma_samples` (the decoder's allocation
    /// size) and its conformance-window cropped size. Every SPS is checked
    /// against the decode dimension and pixel limits before decoding.
    pub sps_coded_size: [u32; 2],
    pub sps_cropped_size: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4ColorDescription {
    pub primaries: u16,
    pub transfer: u16,
    pub matrix: u16,
    pub full_range: bool,
    /// Complete nclx flag byte, including reserved low bits. Source admission
    /// preserves it; a strict export consumer must reject unsupported bits.
    pub range_byte: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp4TrackInspection {
    /// Zero-based container track order, matching the admitted demux stream order.
    pub index: u32,
    pub id: u32,
    pub kind: Mp4TrackKind,
    pub flags: u32,
    /// Movie ticks. None retains the version-specific all-ones unknown value.
    pub duration: Option<u64>,
    /// Raw signed fixed-point tkhd matrix in file order; no transform is applied.
    pub matrix: [i32; 9],
    /// Only an unscaled quarter-turn matrix is recognized. Translation is retained.
    pub rotation_quarter_turns: Option<u8>,
    pub display_width_16_16: u32,
    pub display_height_16_16: u32,
    pub sample_dimensions: Option<[u32; 2]>,
    pub media_timescale: u32,
    pub media_duration: Option<u64>,
    pub edits: Vec<Mp4Edit>,
    pub sample_count: u32,
    /// Exact sum of stts runs in media ticks, independently of mdhd's declaration.
    pub timing_duration: u64,
    pub avc: Option<Mp4AvcConfiguration>,
    /// hvc1 tracks only; `avc` is then None.
    pub hevc: Option<Mp4HevcConfiguration>,
    pub color: Option<Mp4ColorDescription>,
    /// `mdcv` box, converted from its stored G, B, R primary order to R, G, B.
    pub mastering: Option<MasteringDisplay>,
    /// `clli` box.
    pub content_light: Option<ContentLight>,
    pub pixel_aspect_ratio: Option<[u32; 2]>,
    pub sample_audio_channels: Option<u32>,
    pub sample_audio_rate: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp4Inspection {
    pub movie_timescale: u32,
    pub movie_duration: Option<u64>,
    pub movie_matrix: [i32; 9],
    pub moov_before_mdat: bool,
    pub tracks: Vec<Mp4TrackInspection>,
}

pub(super) struct MovieHeader {
    pub(super) timescale: u32,
    pub(super) duration: Option<u64>,
    pub(super) matrix: [i32; 9],
}

pub(super) struct TrackHeader {
    flags: u32,
    duration: Option<u64>,
    matrix: [i32; 9],
    width: u32,
    height: u32,
}

pub(super) fn duration(
    reader: &mut Reader<'_>,
    span: Span,
    version: u8,
    narrow: u64,
    wide: u64,
) -> Result<Option<u64>> {
    if version == 0 {
        let value = reader.u32(span.start + narrow)?;
        Ok((value != u32::MAX).then_some(u64::from(value)))
    } else {
        let value = reader.u64(span.start + wide)?;
        Ok((value != u64::MAX).then_some(value))
    }
}

pub(super) fn track_header(
    reader: &mut Reader<'_>,
    span: Span,
    version: u8,
    full: u32,
) -> Result<TrackHeader> {
    let matrix_start = span.start + if version == 0 { 40 } else { 52 };
    let matrix = matrix(reader, matrix_start)?;
    Ok(TrackHeader {
        flags: full & 0x00ff_ffff,
        duration: duration(reader, span, version, 20, 28)?,
        matrix,
        width: reader.u32(matrix_start + 36)?,
        height: reader.u32(matrix_start + 40)?,
    })
}

pub(super) fn matrix(reader: &mut Reader<'_>, start: u64) -> Result<[i32; 9]> {
    let mut matrix = [0; 9];
    for (index, value) in matrix.iter_mut().enumerate() {
        *value = i32::from_be_bytes(reader.bytes(start + index as u64 * 4)?);
    }
    Ok(matrix)
}

fn rotation(matrix: [i32; 9]) -> Option<u8> {
    let [a, b, u, c, d, v, _, _, w] = matrix;
    if u != 0 || v != 0 || w != 1 << 30 {
        return None;
    }
    match [a, b, c, d] {
        [65_536, 0, 0, 65_536] => Some(0),
        [0, 65_536, -65_536, 0] => Some(1),
        [-65_536, 0, 0, -65_536] => Some(2),
        [0, -65_536, 65_536, 0] => Some(3),
        _ => None,
    }
}

fn summary(layout: &Mp4Layout) -> Result<Mp4Inspection> {
    let mut tracks = Vec::with_capacity(layout.tracks.len());
    for (index, track) in layout.tracks.iter().enumerate() {
        let header = track
            .header
            .as_ref()
            .ok_or_else(|| invalid("MP4 track lacks a retained header"))?;
        tracks.push(Mp4TrackInspection {
            index: u32::try_from(index).map_err(|_| limit("MP4 track index overflow"))?,
            id: track
                .id
                .ok_or_else(|| invalid("MP4 track lacks identity"))?,
            kind: match track
                .codec
                .ok_or_else(|| invalid("MP4 track lacks codec"))?
            {
                Kind::Video => Mp4TrackKind::Video,
                Kind::Audio => Mp4TrackKind::Audio,
            },
            flags: header.flags,
            duration: header.duration,
            matrix: header.matrix,
            rotation_quarter_turns: rotation(header.matrix),
            display_width_16_16: header.width,
            display_height_16_16: header.height,
            sample_dimensions: track.dimensions,
            media_timescale: track.media_timescale,
            media_duration: track.media_duration,
            edits: track.edit_entries.clone(),
            sample_count: track
                .sizes
                .ok_or_else(|| invalid("MP4 track lacks samples"))?
                .count,
            timing_duration: track.timing_duration,
            avc: track.avc,
            hevc: track.hevc,
            color: track.color,
            mastering: track.mastering,
            content_light: track.content_light,
            pixel_aspect_ratio: track.pixel_aspect_ratio,
            sample_audio_channels: track.audio_channels,
            sample_audio_rate: track.audio_sample_rate,
        });
    }
    Ok(Mp4Inspection {
        movie_timescale: layout.movie.timescale,
        movie_duration: layout.movie.duration,
        movie_matrix: layout.movie.matrix,
        moov_before_mdat: layout.moov_before_mdat,
        tracks,
    })
}

fn reader<'a>(
    file: &'a File,
    limits: DecodeLimits,
    control: DecodeControl<'a>,
) -> Result<Reader<'a>> {
    limits.validate()?;
    if control.timeout.is_zero() || control.timeout > std::time::Duration::from_secs(60) {
        return Err(SourceDecodeError::InvalidConfiguration(
            "timeout must be positive and at most 60 seconds",
        ));
    }
    let started = Instant::now();
    if control.cancelled.load(Ordering::Relaxed) {
        return Err(SourceDecodeError::Native {
            code: "cancelled".into(),
            message: "MP4 inspection cancelled before starting".into(),
        });
    }
    let metadata = file.metadata()?;
    require(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= limits.max_input_bytes,
        "MP4 inspection requires a nonempty bounded regular snapshot",
    )?;
    let reader = Reader {
        file,
        control,
        started,
        length: metadata.len(),
        cache: std::array::from_fn(|_| Page::default()),
        cache_clock: 0,
        read_bytes: 0,
        header_bytes: 0,
        atoms: 0,
        rows: 0,
        samples: 0,
        limits: limits.into(),
    };
    reader.check()?;
    Ok(reader)
}

/// Inspect a caller-owned immutable snapshot using the source parser's existing
/// 16 MiB header, one-million sample/table-row, track and packet-size bounds.
/// No MP4 codec, duration, edit, matrix or timing observation is repaired.
/// Source-import admission remains broader than an export contract: callers
/// must separately require two expected tracks, fast-start and simple edits.
pub fn inspect_mp4(
    file: &File,
    limits: DecodeLimits,
    control: DecodeControl<'_>,
) -> Result<Mp4Inspection> {
    let mut reader = reader(file, limits, control)?;
    let layout = mp4_layout(&mut reader)?;
    let inspection = summary(&layout)?;
    reader.check()?;
    Ok(inspection)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4PresentationTime {
    pub dts: i64,
    pub pts: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4H264Packet {
    pub nal_count: u32,
    /// Bit n means a length-prefixed NAL header declared nal_unit_type n.
    pub nal_types: u32,
    pub idr_nal_count: u32,
    pub non_idr_vcl_nal_count: u32,
}

/// Two-byte HEVC NAL headers of one hvc1 sample. IRAP presence does not
/// prove a decodable picture or a closed GOP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4HevcPacket {
    pub nal_count: u32,
    /// Bit n means a NAL header declared nal_unit_type n (0..=63).
    pub nal_types: u64,
    /// nal_unit_type 16..=23 (BLA, IDR, CRA and reserved IRAP).
    pub irap_nal_count: u32,
    /// nal_unit_type 19 or 20 (IDR_W_RADL, IDR_N_LP).
    pub idr_nal_count: u32,
    /// nal_unit_type 0..=9 (non-IRAP VCL).
    pub non_irap_vcl_nal_count: u32,
    /// nal_unit_type 32..=34 (VPS, SPS, PPS) repeated in-band.
    pub parameter_set_nal_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4PacketObservation {
    pub track_index: u32,
    pub track_id: u32,
    /// Zero-based sample ordinal within this track, in decode order.
    pub sample_index: u32,
    pub offset: u64,
    pub length: u32,
    /// Raw media ticks from stts and ctts, before any edit is applied.
    pub dts: i64,
    pub pts: i64,
    pub duration: u32,
    /// Exact ticks after subtracting the sole normal media edit's media_time.
    /// No edits maps directly. Multiple/empty edits yield None, never rounding.
    /// This retains priming/padding packets even outside the presentation edit.
    pub presentation: Option<Mp4PresentationTime>,
    /// The stss claim (or MP4's all-sync default), not a proven random-access point.
    pub table_sync: bool,
    /// NAL headers only. IDR presence does not prove a valid picture or closed GOP.
    pub h264: Option<Mp4H264Packet>,
    /// hvc1 tracks only; `h264` is then None.
    pub hevc: Option<Mp4HevcPacket>,
}

#[derive(Default)]
struct Run {
    row: u32,
    remaining: u32,
    value: i64,
}

impl Run {
    fn next(&mut self, reader: &mut Reader<'_>, table: Table, signed: bool) -> Result<i64> {
        if self.remaining == 0 {
            require(
                self.row < table.rows,
                "timing table ended before its samples",
            )?;
            let at = table.data.start + u64::from(self.row) * 8;
            self.remaining = reader.u32(at)?;
            let value = reader.u32(at + 4)?;
            self.value = if signed {
                i64::from(value as i32)
            } else {
                i64::from(value)
            };
            require(
                self.remaining > 0,
                "empty timing run during packet inspection",
            )?;
            self.row += 1;
        }
        self.remaining -= 1;
        Ok(self.value)
    }
}

#[derive(Default)]
struct Cursor {
    sample: u32,
    chunk: u32,
    mapping_row: u32,
    chunk_remaining: u32,
    offset: u64,
    dts: i64,
    timing: Run,
    composition: Run,
    sync_row: u32,
}

impl Cursor {
    fn chunk(&mut self, reader: &mut Reader<'_>, track: &Track) -> Result<()> {
        let (chunks, wide) = track.chunks.ok_or_else(|| invalid("missing chunk table"))?;
        let mapping = track
            .mapping
            .ok_or_else(|| invalid("missing chunk mapping"))?;
        require(
            self.chunk < chunks.rows,
            "chunk table ended before its samples",
        )?;
        let one_based = self.chunk + 1;
        while self.mapping_row + 1 < mapping.rows {
            let next = reader.u32(mapping.data.start + u64::from(self.mapping_row + 1) * 12)?;
            if one_based < next {
                break;
            }
            self.mapping_row += 1;
        }
        let row = mapping.data.start + u64::from(self.mapping_row) * 12;
        self.chunk_remaining = reader.u32(row + 4)?;
        require(
            self.chunk_remaining > 0,
            "empty sample chunk during inspection",
        )?;
        let at = chunks.data.start + u64::from(self.chunk) * if wide { 8 } else { 4 };
        self.offset = if wide {
            reader.u64(at)?
        } else {
            u64::from(reader.u32(at)?)
        };
        self.chunk += 1;
        Ok(())
    }

    fn sync(&mut self, reader: &mut Reader<'_>, track: &Track) -> Result<bool> {
        let Some(table) = track.sync else {
            return Ok(true);
        };
        if self.sync_row == table.rows {
            return Ok(false);
        }
        let value = reader.u32(table.data.start + u64::from(self.sync_row) * 4)?;
        require(value > self.sample, "sync table moved behind its sample")?;
        if value == self.sample + 1 {
            self.sync_row += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Sequential, descriptor-backed packet observations. Tracks are visited in
/// container order; samples within a track are visited in decode order. This
/// is not global mux order. Only fixed-size table pages and NAL header bytes
/// are retained; packet payloads are never allocated or copied in full.
///
/// Caller retains immutable byte/hash authority. Each call has the existing
/// cooperative DecodeControl/I/O bounds; pass shrinking remaining time from
/// one outer job deadline. A failed traversal poisons this reader.
pub struct Mp4PacketReader {
    file: File,
    limits: DecodeLimits,
    length: u64,
    layout: Mp4Layout,
    inspection: Mp4Inspection,
    track: usize,
    cursor: Cursor,
    cache: [Page; 8],
    cache_clock: u64,
    poisoned: bool,
}

impl Mp4PacketReader {
    pub fn open(file: File, limits: DecodeLimits, control: DecodeControl<'_>) -> Result<Self> {
        let mut reader = reader(&file, limits, control)?;
        let layout = mp4_layout(&mut reader)?;
        let inspection = summary(&layout)?;
        reader.check()?;
        let length = reader.length;
        let cache = reader.cache;
        let cache_clock = reader.cache_clock;
        Ok(Self {
            file,
            limits,
            length,
            layout,
            inspection,
            track: 0,
            cursor: Cursor::default(),
            cache,
            cache_clock,
            poisoned: false,
        })
    }

    pub fn inspection(&self) -> &Mp4Inspection {
        &self.inspection
    }

    pub fn next_packet(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<Option<Mp4PacketObservation>> {
        if self.poisoned {
            return Err(invalid(
                "MP4 packet reader is poisoned by an earlier failure",
            ));
        }
        // Pre-cancelled or invalid control calls do not consume traversal state.
        let mut reader = reader(&self.file, self.limits, control)?;
        self.poisoned = true;
        require(reader.length == self.length, "MP4 snapshot changed length")?;
        reader.cache = std::mem::replace(&mut self.cache, std::array::from_fn(|_| Page::default()));
        reader.cache_clock = self.cache_clock;
        let result = next_packet(&mut reader, &self.layout, &mut self.track, &mut self.cursor)
            .and_then(|packet| {
                reader.check()?;
                Ok(packet)
            });
        self.cache = reader.cache;
        self.cache_clock = reader.cache_clock;
        self.poisoned = result.is_err();
        result
    }
}

fn next_packet(
    reader: &mut Reader<'_>,
    layout: &Mp4Layout,
    track_index: &mut usize,
    cursor: &mut Cursor,
) -> Result<Option<Mp4PacketObservation>> {
    while let Some(track) = layout.tracks.get(*track_index) {
        reader.check()?;
        let sizes = track.sizes.ok_or_else(|| invalid("missing sample table"))?;
        if cursor.sample == sizes.count {
            require(
                cursor.dts as u64 == track.timing_duration,
                "packet timing sum changed",
            )?;
            *track_index += 1;
            *cursor = Cursor::default();
            continue;
        }
        if cursor.chunk_remaining == 0 {
            cursor.chunk(reader, track)?;
        }
        let length = reader.sample_size(sizes, cursor.sample)?;
        let timing = track
            .timing
            .ok_or_else(|| invalid("missing timing table"))?;
        let duration = cursor.timing.next(reader, timing, false)?;
        require(
            duration > 0 && duration <= i64::from(i32::MAX),
            "invalid packet duration",
        )?;
        let composition = match track.composition {
            Some((table, version)) => cursor.composition.next(reader, table, version == 1)?,
            None => 0,
        };
        let pts = cursor
            .dts
            .checked_add(composition)
            .ok_or_else(|| invalid("packet PTS overflow"))?;
        let presentation = match track.edit_entries.as_slice() {
            [] => Some(Mp4PresentationTime {
                dts: cursor.dts,
                pts,
            }),
            [edit]
                if edit.media_time >= 0
                    && edit.media_rate_integer == 1
                    && edit.media_rate_fraction == 0 =>
            {
                Some(Mp4PresentationTime {
                    dts: cursor
                        .dts
                        .checked_sub(edit.media_time)
                        .ok_or_else(|| invalid("edited DTS overflow"))?,
                    pts: pts
                        .checked_sub(edit.media_time)
                        .ok_or_else(|| invalid("edited PTS overflow"))?,
                })
            }
            _ => None,
        };
        let table_sync = cursor.sync(reader, track)?;
        let h264 = match track.avc {
            Some(avc) => Some(nal_headers(
                reader,
                cursor.offset,
                length,
                avc.nal_length_bytes,
            )?),
            None => None,
        };
        let hevc = match track.hevc {
            Some(hevc) => Some(hevc_nal_headers(
                reader,
                cursor.offset,
                length,
                hevc.nal_length_bytes,
            )?),
            None => None,
        };
        let packet = Mp4PacketObservation {
            track_index: u32::try_from(*track_index).map_err(|_| limit("track index overflow"))?,
            track_id: track.id.ok_or_else(|| invalid("missing track identity"))?,
            sample_index: cursor.sample,
            offset: cursor.offset,
            length,
            dts: cursor.dts,
            pts,
            duration: u32::try_from(duration).map_err(|_| invalid("packet duration overflow"))?,
            presentation,
            table_sync,
            h264,
            hevc,
        };
        cursor.dts = cursor
            .dts
            .checked_add(duration)
            .ok_or_else(|| invalid("packet DTS overflow"))?;
        cursor.offset = cursor
            .offset
            .checked_add(u64::from(length))
            .ok_or_else(|| invalid("packet offset overflow"))?;
        cursor.sample += 1;
        cursor.chunk_remaining -= 1;
        return Ok(Some(packet));
    }
    Ok(None)
}

fn packet_bytes<const N: usize>(
    reader: &mut Reader<'_>,
    at: u64,
    used: &mut u64,
) -> Result<[u8; N]> {
    reader.check()?;
    require(
        at <= reader.length && N as u64 <= reader.length - at,
        "truncated MP4 packet field",
    )?;
    let charged = used
        .checked_add(N as u64)
        .ok_or_else(|| limit("packet inspection I/O overflow"))?;
    if reader
        .read_bytes
        .checked_add(charged)
        .is_none_or(|total| total > reader.limits.max_io_bytes_per_call)
    {
        return Err(limit("packet inspection exceeds per-call I/O budget"));
    }
    let mut bytes = [0; N];
    let mut read = 0;
    while read < N {
        reader.check()?;
        let count = match reader.file.read_at(&mut bytes[read..], at + read as u64) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        require(count != 0, "MP4 packet ended during inspection")?;
        read += count;
    }
    *used = charged;
    reader.check()?;
    Ok(bytes)
}

fn nal_headers(
    reader: &mut Reader<'_>,
    offset: u64,
    length: u32,
    width: u8,
) -> Result<Mp4H264Packet> {
    require(matches!(width, 1 | 2 | 4), "invalid AVC NAL length width")?;
    let end = offset
        .checked_add(u64::from(length))
        .ok_or_else(|| invalid("AVC packet extent overflow"))?;
    require(end <= reader.length, "AVC packet leaves input descriptor")?;
    let mut used = 0;
    if length >= 7 {
        let prefix = packet_bytes::<7>(reader, offset, &mut used)?;
        require(
            !(prefix[0] == 1 && prefix[4] & 0xfc == 0xfc && prefix[5] & 0xe0 == 0xe0),
            "AVC packet carries replacement configuration",
        )?;
    }
    let mut packet = Mp4H264Packet {
        nal_count: 0,
        nal_types: 0,
        idr_nal_count: 0,
        non_idr_vcl_nal_count: 0,
    };
    let mut at = offset;
    while at < end {
        reader.check()?;
        if packet.nal_count == 4096 {
            return Err(limit("AVC packet exceeds 4096 NAL units"));
        }
        require(
            u64::from(width) <= end - at,
            "truncated AVC packet NAL length",
        )?;
        let length = match width {
            1 => u32::from(packet_bytes::<1>(reader, at, &mut used)?[0]),
            2 => u32::from(u16::from_be_bytes(packet_bytes(reader, at, &mut used)?)),
            4 => u32::from_be_bytes(packet_bytes(reader, at, &mut used)?),
            _ => unreachable!("NAL width validated"),
        };
        at += u64::from(width);
        require(
            length > 0 && u64::from(length) <= end - at,
            "AVC NAL escapes its packet",
        )?;
        let header = packet_bytes::<1>(reader, at, &mut used)?[0];
        let kind = header & 31;
        require(header & 128 == 0 && kind != 0, "invalid AVC NAL header")?;
        packet.nal_count += 1;
        packet.nal_types |= 1_u32 << kind;
        packet.idr_nal_count += u32::from(kind == 5);
        packet.non_idr_vcl_nal_count += u32::from(matches!(kind, 1..=4 | 19..=21));
        at += u64::from(length);
    }
    require(
        packet.nal_count > 0 && at == end,
        "empty or incomplete AVC packet",
    )?;
    Ok(packet)
}

fn hevc_nal_headers(
    reader: &mut Reader<'_>,
    offset: u64,
    length: u32,
    width: u8,
) -> Result<Mp4HevcPacket> {
    require(matches!(width, 1 | 2 | 4), "invalid HEVC NAL length width")?;
    let end = offset
        .checked_add(u64::from(length))
        .ok_or_else(|| invalid("HEVC packet extent overflow"))?;
    require(end <= reader.length, "HEVC packet leaves input descriptor")?;
    let mut used = 0;
    let mut packet = Mp4HevcPacket {
        nal_count: 0,
        nal_types: 0,
        irap_nal_count: 0,
        idr_nal_count: 0,
        non_irap_vcl_nal_count: 0,
        parameter_set_nal_count: 0,
    };
    let mut at = offset;
    while at < end {
        reader.check()?;
        if packet.nal_count == 4096 {
            return Err(limit("HEVC packet exceeds 4096 NAL units"));
        }
        require(
            u64::from(width) <= end - at,
            "truncated HEVC packet NAL length",
        )?;
        let length = match width {
            1 => u32::from(packet_bytes::<1>(reader, at, &mut used)?[0]),
            2 => u32::from(u16::from_be_bytes(packet_bytes(reader, at, &mut used)?)),
            4 => u32::from_be_bytes(packet_bytes(reader, at, &mut used)?),
            _ => unreachable!("NAL width validated"),
        };
        at += u64::from(width);
        require(
            length >= 2 && u64::from(length) <= end - at,
            "HEVC NAL escapes its packet",
        )?;
        let header = packet_bytes::<2>(reader, at, &mut used)?;
        require(
            header[0] & 0x81 == 0 && header[1] >> 3 == 0 && header[1] & 7 != 0,
            "invalid or multilayer HEVC NAL header",
        )?;
        let kind = (header[0] >> 1) & 63;
        packet.nal_count += 1;
        packet.nal_types |= 1_u64 << kind;
        packet.irap_nal_count += u32::from((16..=23).contains(&kind));
        packet.idr_nal_count += u32::from(matches!(kind, 19 | 20));
        packet.non_irap_vcl_nal_count += u32::from(kind <= 9);
        packet.parameter_set_nal_count += u32::from((32..=34).contains(&kind));
        at += u64::from(length);
    }
    require(
        packet.nal_count > 0 && at == end,
        "empty or incomplete HEVC packet",
    )?;
    Ok(packet)
}

#[cfg(test)]
mod tests;
