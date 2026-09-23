//! Closed container grammar checked before FFmpeg can allocate from header counts.
//! This is allocation admission for immutable snapshots, not media qualification.

use crate::audio::AudioDecodeLimits;
use crate::{DecodeControl, DecodeLimits, SourceDecodeError};
use std::{fs::File, os::unix::fs::FileExt, sync::atomic::Ordering, time::Instant};

const HEADER_BYTES: u64 = 16 * 1024 * 1024;
const ITEMS: u64 = 1_000_000;
const ATOMS: u32 = 100_000;
const TRACKS: usize = 33;
const EXTRA_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Audio(u32),
    FirstAudio,
    Video,
}

/// Container allocation ceilings derived from already validated decoder limits.
/// Audio selection keeps the existing hard video bounds for ignored tracks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InputLimits {
    pub(crate) max_input_bytes: u64,
    pub(crate) max_packets: u64,
    pub(crate) max_io_bytes_per_call: u64,
    pub(crate) max_packet_bytes: u64,
    pub(crate) max_decoded_samples: u64,
    pub(crate) max_channels: u32,
    pub(crate) max_sample_rate: u32,
    pub(crate) max_pixels: u64,
    pub(crate) max_dimension: u32,
}

impl From<AudioDecodeLimits> for InputLimits {
    fn from(limits: AudioDecodeLimits) -> Self {
        Self {
            max_input_bytes: limits.max_input_bytes,
            max_packets: limits.max_packets,
            max_io_bytes_per_call: limits.max_io_bytes_per_call,
            max_packet_bytes: u64::from(limits.max_packet_bytes),
            max_decoded_samples: limits.max_decoded_samples,
            max_channels: limits.max_channels,
            max_sample_rate: limits.max_sample_rate,
            max_pixels: 8192 * 8192,
            max_dimension: 8192,
        }
    }
}

impl From<DecodeLimits> for InputLimits {
    fn from(limits: DecodeLimits) -> Self {
        Self {
            max_input_bytes: limits.max_input_bytes,
            max_packets: limits.max_packets,
            max_io_bytes_per_call: limits.max_io_bytes_per_call,
            max_packet_bytes: limits.max_packet_bytes,
            max_decoded_samples: 1_000_000_000_000,
            max_channels: 32,
            max_sample_rate: 384_000,
            max_pixels: limits.max_pixels,
            max_dimension: limits.max_dimension,
        }
    }
}

type Result<T> = std::result::Result<T, SourceDecodeError>;

fn invalid(message: &'static str) -> SourceDecodeError {
    SourceDecodeError::Native {
        code: "invalid_input".into(),
        message: message.into(),
    }
}
fn limit(message: &'static str) -> SourceDecodeError {
    SourceDecodeError::Native {
        code: "resource_limit".into(),
        message: message.into(),
    }
}
fn selection() -> SourceDecodeError {
    SourceDecodeError::Native {
        code: "unsupported_streams".into(),
        message: "selected media streams do not match the admitted container grammar".into(),
    }
}
fn require(value: bool, message: &'static str) -> Result<()> {
    if value { Ok(()) } else { Err(invalid(message)) }
}

#[derive(Clone, Copy)]
struct Span {
    start: u64,
    end: u64,
}
impl Span {
    fn len(self) -> u64 {
        self.end - self.start
    }
    fn suffix(self, length: u64) -> Result<Self> {
        require(
            length <= self.len(),
            "container field exceeds its enclosing box",
        )?;
        Ok(Self {
            start: self.start + length,
            end: self.end,
        })
    }
}
#[derive(Clone, Copy)]
struct Atom {
    tag: [u8; 4],
    body: Span,
}
#[derive(Clone, Copy)]
struct Table {
    rows: u32,
    data: Span,
}
#[derive(Clone, Copy)]
struct Sizes {
    count: u32,
    constant: u32,
    bits: u32,
    data: Span,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Audio,
    Video,
}
#[derive(Default)]
struct Track {
    id: Option<u32>,
    handler: Option<Kind>,
    codec: Option<Kind>,
    mdhd: bool,
    minf: bool,
    edits: bool,
    sizes: Option<Sizes>,
    chunks: Option<(Table, bool)>,
    mapping: Option<Table>,
    timing_count: Option<u64>,
    composition_count: Option<u64>,
    sync: Option<Table>,
    roll_description: bool,
    roll_samples: Option<u32>,
}

struct Page {
    bytes: [u8; 4096],
    start: u64,
    length: usize,
    used: u64,
}
impl Default for Page {
    fn default() -> Self {
        Self {
            bytes: [0; 4096],
            start: 0,
            length: 0,
            used: 0,
        }
    }
}

struct Reader<'a> {
    file: &'a File,
    control: DecodeControl<'a>,
    started: Instant,
    length: u64,
    // Sample sizes, chunk offsets, and run tables are walked together. Separate
    // resident pages prevent charging a whole page again for every sample.
    cache: [Page; 8],
    cache_clock: u64,
    read_bytes: u64,
    header_bytes: u64,
    atoms: u32,
    rows: u64,
    samples: u64,
    limits: InputLimits,
}
impl Reader<'_> {
    fn check(&self) -> Result<()> {
        if self.control.cancelled.load(Ordering::Relaxed) {
            return Err(SourceDecodeError::Native {
                code: "cancelled".into(),
                message: "input preflight cancelled".into(),
            });
        }
        if self.started.elapsed() >= self.control.timeout {
            return Err(SourceDecodeError::Native {
                code: "deadline_exceeded".into(),
                message: "input preflight exceeded its cooperative deadline".into(),
            });
        }
        Ok(())
    }
    fn bytes<const N: usize>(&mut self, position: u64) -> Result<[u8; N]> {
        self.check()?;
        require(
            N <= 64 && position <= self.length && N as u64 <= self.length - position,
            "truncated container field",
        )?;
        let mut output = [0; N];
        let mut copied = 0;
        while copied < N {
            self.check()?;
            let at = position + copied as u64;
            let index = if let Some(index) = self.cache.iter().position(|page| {
                page.length > 0 && at >= page.start && at - page.start < page.length as u64
            }) {
                index
            } else {
                let index = self
                    .cache
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, page)| page.used)
                    .map(|(index, _)| index)
                    .expect("fixed nonempty cache");
                let start = at - at % 4096;
                self.cache[index].start = start;
                self.cache[index].length = 0;
                let amount = (self.length - start).min(4096) as usize;
                if self.read_bytes + amount as u64 > HEADER_BYTES
                    || self.read_bytes + amount as u64 > self.limits.max_io_bytes_per_call
                {
                    return Err(limit("input preflight header read budget exceeded"));
                }
                while self.cache[index].length < amount {
                    self.check()?;
                    let length = self.cache[index].length;
                    let count = match self.file.read_at(
                        &mut self.cache[index].bytes[length..amount],
                        start + length as u64,
                    ) {
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        result => result?,
                    };
                    require(count != 0, "snapshot ended during container preflight")?;
                    self.read_bytes += count as u64;
                    self.cache[index].length += count;
                }
                index
            };
            self.cache_clock += 1;
            let page = &mut self.cache[index];
            page.used = self.cache_clock;
            let offset = (at - page.start) as usize;
            let count = (N - copied).min(page.length - offset);
            output[copied..copied + count].copy_from_slice(&page.bytes[offset..offset + count]);
            copied += count;
        }
        Ok(output)
    }
    fn u32(&mut self, position: u64) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(position)?))
    }
    fn u64(&mut self, position: u64) -> Result<u64> {
        Ok(u64::from_be_bytes(self.bytes(position)?))
    }
    fn charge_header(&mut self, bytes: u64) -> Result<()> {
        self.header_bytes = self
            .header_bytes
            .checked_add(bytes)
            .ok_or_else(|| limit("container header size overflow"))?;
        if self.header_bytes > HEADER_BYTES {
            return Err(limit("container headers exceed 16 MiB"));
        }
        Ok(())
    }
    fn atom(&mut self, cursor: &mut u64, end: u64, depth: u32) -> Result<Atom> {
        self.check()?;
        if depth > 16 || self.atoms >= ATOMS {
            return Err(limit("container nesting or atom count exceeds bound"));
        }
        self.atoms += 1;
        require(*cursor <= end && end - *cursor >= 8, "truncated box header")?;
        let size = self.u32(*cursor)?;
        let tag = self.bytes(*cursor + 4)?;
        let (size, header) = if size == 1 {
            require(end - *cursor >= 16, "truncated extended box size")?;
            (self.u64(*cursor + 8)?, 16)
        } else {
            (u64::from(size), 8)
        };
        require(
            size >= header && size <= end - *cursor,
            "box size exceeds its parent or uses an unsupported open end",
        )?;
        let body = Span {
            start: *cursor + header,
            end: *cursor + size,
        };
        *cursor += size;
        Ok(Atom { tag, body })
    }
    fn full(&mut self, span: Span, versions: &[u8], flags: u32) -> Result<u8> {
        require(span.len() >= 4, "truncated full box")?;
        let word = self.u32(span.start)?;
        let version = (word >> 24) as u8;
        require(
            versions.contains(&version) && word & 0x00ff_ffff == flags,
            "unsupported box version or flags",
        )?;
        Ok(version)
    }
    fn table(&mut self, span: Span, width: u64, versions: &[u8]) -> Result<(Table, u8)> {
        let version = self.full(span, versions, 0)?;
        require(span.len() >= 8, "truncated table count")?;
        let rows = self.u32(span.start + 4)?;
        require(
            span.len() == 8 + u64::from(rows) * width,
            "table count disagrees with its box length",
        )?;
        self.charge_rows(u64::from(rows))?;
        Ok((
            Table {
                rows,
                data: span.suffix(8)?,
            },
            version,
        ))
    }
    fn charge_rows(&mut self, count: u64) -> Result<()> {
        self.rows = self
            .rows
            .checked_add(count)
            .ok_or_else(|| limit("table row count overflow"))?;
        if self.rows > ITEMS {
            return Err(limit("aggregate container table rows exceed one million"));
        }
        Ok(())
    }
    fn fixed(&mut self, span: Span, length: u64) -> Result<()> {
        self.check()?;
        require(span.len() == length, "unsupported fixed box length")
    }
    fn sample_size(&mut self, sizes: Sizes, ordinal: u32) -> Result<u32> {
        self.check()?;
        require(ordinal < sizes.count, "sample table index out of range")?;
        let ordinal = u64::from(ordinal);
        let size = if sizes.constant != 0 {
            sizes.constant
        } else {
            match sizes.bits {
                4 => {
                    let byte = self.bytes::<1>(sizes.data.start + ordinal / 2)?[0];
                    u32::from(if ordinal.is_multiple_of(2) {
                        byte >> 4
                    } else {
                        byte & 15
                    })
                }
                8 => u32::from(self.bytes::<1>(sizes.data.start + ordinal)?[0]),
                16 => u32::from(u16::from_be_bytes(
                    self.bytes(sizes.data.start + ordinal * 2)?,
                )),
                32 => self.u32(sizes.data.start + ordinal * 4)?,
                _ => return Err(invalid("unsupported sample-size field width")),
            }
        };
        if size == 0 || u64::from(size) > self.limits.max_packet_bytes {
            return Err(limit(
                "declared media sample exceeds packet bound or is empty",
            ));
        }
        Ok(size)
    }
}

pub(crate) fn validate(
    file: &File,
    policy: Selection,
    limits: InputLimits,
    control: DecodeControl<'_>,
) -> Result<u64> {
    Ok(validate_selection(file, policy, limits, control)?.io_bytes)
}

/// Resolve the first audio stream from the same bounded grammar pass that
/// admits its allocation. Never probe or repeatedly open guessed stream IDs.
pub(crate) fn validate_audio(
    file: &File,
    selected: Option<u32>,
    limits: InputLimits,
    control: DecodeControl<'_>,
) -> Result<(u32, u64)> {
    let policy = selected.map_or(Selection::FirstAudio, Selection::Audio);
    let admitted = validate_selection(file, policy, limits, control)?;
    Ok((admitted.audio.ok_or_else(selection)?, admitted.io_bytes))
}

struct Admission {
    io_bytes: u64,
    audio: Option<u32>,
}

fn validate_selection(
    file: &File,
    policy: Selection,
    limits: InputLimits,
    control: DecodeControl<'_>,
) -> Result<Admission> {
    if control.timeout.is_zero() || control.timeout > std::time::Duration::from_secs(60) {
        return Err(SourceDecodeError::InvalidConfiguration(
            "timeout must be positive and at most 60 seconds",
        ));
    }
    let started = Instant::now();
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limits.max_input_bytes {
        return Err(SourceDecodeError::Native {
            code: "invalid_input".into(),
            message: "input must be a nonempty bounded regular snapshot".into(),
        });
    }
    let mut reader = Reader {
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
        limits,
    };
    let magic = reader.bytes::<4>(0)?;
    if magic == [0x1a, 0x45, 0xdf, 0xa3] {
        if policy != Selection::Video {
            return Err(SourceDecodeError::Native {
                code: "unsupported_container".into(),
                message: "Matroska audio admission is not qualified".into(),
            });
        }
        let remaining_io = limits
            .max_io_bytes_per_call
            .min(HEADER_BYTES)
            .checked_sub(reader.read_bytes)
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| limit("Matroska detection exhausted the input admission budget"))?;
        let timeout = control
            .timeout
            .checked_sub(started.elapsed())
            .filter(|timeout| !timeout.is_zero())
            .ok_or_else(|| SourceDecodeError::Native {
                code: "deadline_exceeded".into(),
                message: "Matroska detection exhausted the admission deadline".into(),
            })?;
        let charged = crate::matroska_input::validate(
            file,
            InputLimits {
                max_io_bytes_per_call: remaining_io,
                ..limits
            },
            DecodeControl { timeout, ..control },
        )?;
        return Ok(Admission {
            io_bytes: reader.read_bytes + charged,
            audio: None,
        });
    }
    let audio = if magic == *b"RIFF" {
        let selected_stream = match policy {
            Selection::Audio(index) => index,
            Selection::FirstAudio => 0,
            Selection::Video => return Err(selection()),
        };
        wave(&mut reader, selected_stream)?;
        Some(selected_stream)
    } else if reader.length >= 8 && reader.bytes::<4>(4)? == *b"ftyp" {
        mp4(&mut reader, policy)?
    } else {
        return Err(SourceDecodeError::Native {
            code: "unsupported_container".into(),
            message: "only strict MP4, finite FFV1 Matroska, and PCM16 RIFF/WAVE are admitted"
                .into(),
        });
    };
    reader.check()?;
    Ok(Admission {
        io_bytes: reader.read_bytes,
        audio,
    })
}

fn wave(r: &mut Reader<'_>, selected: u32) -> Result<()> {
    if selected != 0 {
        return Err(selection());
    }
    require(
        r.length >= 12 && r.bytes::<4>(8)? == *b"WAVE",
        "only plain RIFF/WAVE is admitted",
    )?;
    let declared = u64::from(u32::from_le_bytes(r.bytes(4)?));
    require(
        declared + 8 == r.length,
        "RIFF length disagrees with immutable snapshot length",
    )?;
    r.charge_header(12)?;
    let mut cursor = 12;
    let mut align = None;
    let mut data = false;
    while cursor < r.length {
        r.check()?;
        if r.atoms >= ATOMS {
            return Err(limit("WAVE chunk count exceeds bound"));
        }
        r.atoms += 1;
        require(r.length - cursor >= 8, "truncated WAVE chunk header")?;
        let tag = r.bytes::<4>(cursor)?;
        let length = u64::from(u32::from_le_bytes(r.bytes(cursor + 4)?));
        let start = cursor + 8;
        require(
            length <= r.length - start && length + (length & 1) <= r.length - start,
            "WAVE chunk or alignment padding exceeds snapshot",
        )?;
        match &tag {
            b"fmt " => {
                require(
                    align.is_none() && !data && length == 16,
                    "WAVE requires one plain fmt16 before data",
                )?;
                let bytes = r.bytes::<16>(start)?;
                let format = u16::from_le_bytes([bytes[0], bytes[1]]);
                if format != 1 {
                    return Err(SourceDecodeError::Native {
                        code: "unsupported_codec".into(),
                        message: "only signed16 PCM WAVE is admitted".into(),
                    });
                }
                let channels = u32::from(u16::from_le_bytes([bytes[2], bytes[3]]));
                let rate = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
                let byte_rate = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
                let block = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
                require(
                    u16::from_le_bytes([bytes[14], bytes[15]]) == 16,
                    "WAVE sample width is not signed16",
                )?;
                if channels == 0
                    || channels > r.limits.max_channels
                    || rate == 0
                    || rate > r.limits.max_sample_rate
                {
                    return Err(limit("WAVE channels or rate exceed configured bounds"));
                }
                require(
                    block == channels * 2
                        && u64::from(byte_rate) == u64::from(rate) * u64::from(block),
                    "WAVE block alignment or byte rate disagrees with PCM16",
                )?;
                if u64::from(block) > r.limits.max_packet_bytes {
                    return Err(limit("one PCM sample frame exceeds packet bound"));
                }
                align = Some(block);
                r.charge_header(8 + length)?;
            }
            b"data" => {
                let block = align.ok_or_else(|| invalid("WAVE data precedes format"))?;
                require(
                    !data && length != 0 && length.is_multiple_of(u64::from(block)),
                    "WAVE requires one complete aligned nonempty data chunk",
                )?;
                if length / u64::from(block) > r.limits.max_decoded_samples {
                    return Err(limit("WAVE sample count exceeds configured bound"));
                }
                data = true;
                r.charge_header(8)?;
            }
            b"JUNK" => r.charge_header(8 + length + (length & 1))?,
            _ => return Err(invalid("unqualified WAVE metadata or container extension")),
        }
        cursor = start + length + (length & 1);
    }
    require(data && align.is_some(), "WAVE has no complete PCM stream")
}

fn mp4(r: &mut Reader<'_>, policy: Selection) -> Result<Option<u32>> {
    let mut cursor = 0;
    let mut brands = false;
    let mut movie = false;
    let mut media = None;
    let mut tracks = Vec::new();
    while cursor < r.length {
        let start = cursor;
        let atom = r.atom(&mut cursor, r.length, 0)?;
        if atom.tag != *b"mdat" {
            r.charge_header(cursor - start)?;
        } else {
            r.charge_header(atom.body.start - start)?;
        }
        match &atom.tag {
            b"ftyp" => {
                require(
                    !brands
                        && start == 0
                        && atom.body.len() >= 8
                        && atom.body.len() <= 256
                        && atom.body.len().is_multiple_of(4),
                    "unsupported or duplicate MP4 file type",
                )?;
                for offset in (0..atom.body.len()).step_by(4) {
                    if offset == 4 {
                        continue;
                    }
                    let brand = r.bytes::<4>(atom.body.start + offset)?;
                    require(
                        matches!(
                            &brand,
                            b"isom" | b"iso2" | b"mp41" | b"mp42" | b"avc1" | b"M4A "
                        ),
                        "unqualified MP4 brand",
                    )?;
                }
                brands = true;
            }
            b"moov" => {
                require(
                    brands && !movie,
                    "MP4 requires one movie after its file type",
                )?;
                movie = true;
                movie_box(r, atom.body, &mut tracks)?;
            }
            b"mdat" => {
                require(
                    brands && media.is_none() && atom.body.len() > 0,
                    "MP4 requires one nonempty media-data box",
                )?;
                media = Some(atom.body);
            }
            b"free" => require(
                atom.body.len() == 0,
                "only empty MP4 free boxes are admitted",
            )?,
            _ => {
                return Err(invalid(
                    "unknown, fragmented, compressed, or encrypted MP4 root metadata",
                ));
            }
        }
    }
    require(
        brands && movie && !tracks.is_empty(),
        "MP4 lacks its bounded movie header",
    )?;
    let media = media.ok_or_else(|| invalid("MP4 has no media-data box"))?;
    for track in &tracks {
        validate_track(r, track, media)?;
    }
    match policy {
        Selection::Audio(index) => {
            if tracks.get(index as usize).and_then(|track| track.codec) != Some(Kind::Audio) {
                return Err(selection());
            }
            Ok(Some(index))
        }
        Selection::FirstAudio => {
            let index = tracks
                .iter()
                .position(|track| track.codec == Some(Kind::Audio))
                .ok_or_else(selection)?;
            Ok(Some(
                u32::try_from(index).map_err(|_| limit("audio stream index overflow"))?,
            ))
        }
        Selection::Video => {
            if tracks
                .iter()
                .filter(|track| track.codec == Some(Kind::Video))
                .count()
                != 1
            {
                return Err(selection());
            }
            Ok(None)
        }
    }
}

fn movie_box(r: &mut Reader<'_>, span: Span, tracks: &mut Vec<Track>) -> Result<()> {
    let mut cursor = span.start;
    let mut header = false;
    let mut metadata = false;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 1)?;
        match &atom.tag {
            b"mvhd" => {
                require(!header, "duplicate movie header")?;
                let version = r.full(atom.body, &[0, 1], 0)?;
                r.fixed(atom.body, if version == 0 { 100 } else { 112 })?;
                let timescale = r.u32(atom.body.start + if version == 0 { 12 } else { 20 })?;
                require(
                    timescale > 0 && timescale <= i32::MAX as u32,
                    "invalid movie time scale",
                )?;
                header = true;
            }
            b"trak" => {
                if tracks.len() >= TRACKS {
                    return Err(limit("MP4 track count exceeds bound"));
                }
                let track = track_box(r, atom.body)?;
                require(
                    !tracks.iter().any(|other| other.id == track.id),
                    "duplicate MP4 track identity",
                )?;
                tracks.push(track);
            }
            b"udta" => {
                require(!metadata, "duplicate movie metadata")?;
                metadata = true;
                encoder_metadata(r, atom.body)?;
            }
            _ => return Err(invalid("unqualified movie metadata or fragmentation")),
        }
    }
    require(header, "MP4 lacks movie header")
}

fn track_box(r: &mut Reader<'_>, span: Span) -> Result<Track> {
    let mut track = Track::default();
    let mut cursor = span.start;
    let mut media = false;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 2)?;
        match &atom.tag {
            b"tkhd" => {
                require(track.id.is_none(), "duplicate track header")?;
                require(atom.body.len() >= 4, "truncated track header")?;
                let full = r.u32(atom.body.start)?;
                let version = (full >> 24) as u8;
                require(
                    version <= 1 && full & 0x00ff_fff8 == 0,
                    "unsupported track header version or flags",
                )?;
                r.fixed(atom.body, if version == 0 { 84 } else { 96 })?;
                let id = r.u32(atom.body.start + if version == 0 { 12 } else { 20 })?;
                require(id != 0, "invalid MP4 track identity")?;
                track.id = Some(id);
            }
            b"edts" => {
                require(!track.edits, "duplicate track edits")?;
                track.edits = true;
                edit_box(r, atom.body)?;
            }
            b"mdia" => {
                require(!media, "duplicate track media header")?;
                media = true;
                media_box(r, atom.body, &mut track)?;
            }
            _ => return Err(invalid("unqualified track metadata")),
        }
    }
    require(
        track.id.is_some() && media,
        "track lacks identity or media header",
    )?;
    Ok(track)
}

fn edit_box(r: &mut Reader<'_>, span: Span) -> Result<()> {
    let mut cursor = span.start;
    let atom = r.atom(&mut cursor, span.end, 3)?;
    require(
        cursor == span.end && atom.tag == *b"elst",
        "edit container must contain only one edit list",
    )?;
    let version = r.full(atom.body, &[0, 1], 0)?;
    let (table, _) = r.table(atom.body, if version == 0 { 12 } else { 20 }, &[version])?;
    require(
        (1..=2).contains(&table.rows),
        "only one media edit optionally preceded by one empty edit is admitted",
    )?;
    for index in 0..table.rows {
        let at = table.data.start + u64::from(index) * if version == 0 { 12 } else { 20 };
        let (duration, time, rate) = if version == 0 {
            (
                u64::from(r.u32(at)?),
                i64::from(r.u32(at + 4)? as i32),
                r.u32(at + 8)?,
            )
        } else {
            (r.u64(at)?, r.u64(at + 8)? as i64, r.u32(at + 16)?)
        };
        require(
            duration > 0 && duration <= i64::MAX as u64 && rate == 0x0001_0000,
            "unsupported edit duration or rate",
        )?;
        require(
            if index == 0 && table.rows == 2 {
                time == -1
            } else {
                time >= 0
            },
            "unsupported repeated or negative media edit",
        )?;
    }
    Ok(())
}

fn media_box(r: &mut Reader<'_>, span: Span, track: &mut Track) -> Result<()> {
    let mut cursor = span.start;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 3)?;
        match &atom.tag {
            b"mdhd" => {
                require(!track.mdhd, "duplicate media header")?;
                let version = r.full(atom.body, &[0, 1], 0)?;
                r.fixed(atom.body, if version == 0 { 24 } else { 36 })?;
                let timescale = r.u32(atom.body.start + if version == 0 { 12 } else { 20 })?;
                require(
                    timescale > 0 && timescale <= i32::MAX as u32,
                    "invalid media time scale",
                )?;
                track.mdhd = true;
            }
            b"hdlr" => {
                require(track.handler.is_none(), "duplicate media handler")?;
                let handler = handler(r, atom.body)?;
                track.handler = Some(match &handler {
                    b"soun" => Kind::Audio,
                    b"vide" => Kind::Video,
                    _ => return Err(invalid("only audio and video MP4 tracks are admitted")),
                });
            }
            b"minf" => {
                require(!track.minf, "duplicate media information")?;
                track.minf = true;
                information_box(r, atom.body, track)?;
            }
            _ => return Err(invalid("unqualified media metadata")),
        }
    }
    require(
        track.mdhd && track.handler.is_some() && track.minf,
        "incomplete media header",
    )
}
fn handler(r: &mut Reader<'_>, span: Span) -> Result<[u8; 4]> {
    r.full(span, &[0], 0)?;
    require(
        (24..=1024).contains(&span.len()),
        "invalid or excessive handler name",
    )?;
    r.bytes(span.start + 8)
}
fn information_box(r: &mut Reader<'_>, span: Span, track: &mut Track) -> Result<()> {
    let mut cursor = span.start;
    let mut kind = None;
    let mut data_ref = false;
    let mut samples = false;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 4)?;
        match &atom.tag {
            b"smhd" | b"vmhd" => {
                require(kind.is_none(), "duplicate media information header")?;
                let audio = atom.tag == *b"smhd";
                r.fixed(atom.body, if audio { 8 } else { 12 })?;
                r.full(atom.body, &[0], if audio { 0 } else { 1 })?;
                kind = Some(if audio { Kind::Audio } else { Kind::Video });
            }
            b"dinf" => {
                require(!data_ref, "duplicate data reference")?;
                data_ref = true;
                data_reference(r, atom.body)?;
            }
            b"stbl" => {
                require(!samples, "duplicate sample table container")?;
                samples = true;
                sample_tables(r, atom.body, track)?;
            }
            _ => return Err(invalid("unqualified media information")),
        }
    }
    require(
        samples && data_ref && kind.is_some() && kind == track.codec,
        "incomplete or inconsistent media information",
    )
}
fn data_reference(r: &mut Reader<'_>, span: Span) -> Result<()> {
    let mut cursor = span.start;
    let atom = r.atom(&mut cursor, span.end, 5)?;
    require(
        cursor == span.end && atom.tag == *b"dref",
        "data information requires one self-contained reference",
    )?;
    r.fixed(atom.body, 20)?;
    r.full(atom.body, &[0], 0)?;
    require(
        r.u32(atom.body.start + 4)? == 1
            && r.u32(atom.body.start + 8)? == 12
            && r.bytes::<4>(atom.body.start + 12)? == *b"url "
            && r.u32(atom.body.start + 16)? == 1,
        "external or ambiguous MP4 data reference",
    )
}

fn sample_tables(r: &mut Reader<'_>, span: Span, track: &mut Track) -> Result<()> {
    let mut cursor = span.start;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 5)?;
        match &atom.tag {
            b"stsd" => {
                require(track.codec.is_none(), "duplicate sample description")?;
                track.codec = Some(sample_description(r, atom.body)?);
            }
            b"stsz" | b"stz2" => {
                require(track.sizes.is_none(), "duplicate sample-size table")?;
                track.sizes = Some(size_table(r, atom)?);
            }
            b"stco" | b"co64" => {
                require(track.chunks.is_none(), "duplicate chunk-offset table")?;
                let wide = atom.tag == *b"co64";
                let (table, _) = r.table(atom.body, if wide { 8 } else { 4 }, &[0])?;
                require(table.rows != 0, "empty chunk table")?;
                track.chunks = Some((table, wide));
            }
            b"stsc" => {
                require(track.mapping.is_none(), "duplicate sample-to-chunk table")?;
                let (table, _) = r.table(atom.body, 12, &[0])?;
                require(table.rows != 0, "empty sample-to-chunk table")?;
                track.mapping = Some(table);
            }
            b"stts" | b"ctts" => {
                let composition = atom.tag == *b"ctts";
                require(
                    if composition {
                        track.composition_count.is_none()
                    } else {
                        track.timing_count.is_none()
                    },
                    "duplicate timing table",
                )?;
                let (table, version) =
                    r.table(atom.body, 8, if composition { &[0, 1] } else { &[0] })?;
                require(table.rows != 0, "empty timing table")?;
                let mut samples = 0_u64;
                let mut duration = 0_u64;
                for row in 0..table.rows {
                    let at = table.data.start + u64::from(row) * 8;
                    let count = r.u32(at)?;
                    let value = r.u32(at + 4)?;
                    require(count > 0, "zero timing run count")?;
                    samples += u64::from(count);
                    if samples > ITEMS {
                        return Err(limit("expanded timing sample count exceeds one million"));
                    }
                    if !composition {
                        require(
                            value > 0 && value <= i32::MAX as u32,
                            "invalid timing delta",
                        )?;
                        duration = duration
                            .checked_add(u64::from(count) * u64::from(value))
                            .ok_or_else(|| invalid("timing duration overflow"))?;
                        require(
                            duration <= i64::MAX as u64,
                            "timing duration exceeds signed timestamps",
                        )?;
                    } else if version == 0 {
                        require(
                            value <= i32::MAX as u32,
                            "unsigned composition offset exceeds supported signed interpretation",
                        )?;
                    }
                }
                if composition {
                    track.composition_count = Some(samples);
                } else {
                    track.timing_count = Some(samples);
                }
            }
            b"stss" => {
                require(track.sync.is_none(), "duplicate sync-sample table")?;
                track.sync = Some(r.table(atom.body, 4, &[0])?.0);
            }
            b"sgpd" => {
                require(
                    !track.roll_description,
                    "duplicate sample-group description",
                )?;
                r.fixed(atom.body, 18)?;
                r.full(atom.body, &[1], 0)?;
                require(
                    r.bytes::<4>(atom.body.start + 4)? == *b"roll"
                        && r.u32(atom.body.start + 8)? == 2
                        && r.u32(atom.body.start + 12)? == 1,
                    "only one fixed-size roll description is admitted",
                )?;
                track.roll_description = true;
            }
            b"sbgp" => {
                require(
                    track.roll_samples.is_none(),
                    "duplicate sample-to-group table",
                )?;
                r.fixed(atom.body, 20)?;
                r.full(atom.body, &[0], 0)?;
                require(
                    r.bytes::<4>(atom.body.start + 4)? == *b"roll"
                        && r.u32(atom.body.start + 8)? == 1
                        && r.u32(atom.body.start + 16)? == 1,
                    "only one roll group is admitted",
                )?;
                track.roll_samples = Some(r.u32(atom.body.start + 12)?);
            }
            _ => return Err(invalid("unqualified sample table or encryption metadata")),
        }
    }
    Ok(())
}

fn size_table(r: &mut Reader<'_>, atom: Atom) -> Result<Sizes> {
    r.full(atom.body, &[0], 0)?;
    require(atom.body.len() >= 12, "truncated sample-size table")?;
    let field = r.u32(atom.body.start + 4)?;
    let count = r.u32(atom.body.start + 8)?;
    require(count > 0, "empty sample-size table")?;
    r.samples += u64::from(count);
    if r.samples > ITEMS || r.samples > r.limits.max_packets {
        return Err(limit(
            "aggregate declared media samples exceed admission bound",
        ));
    }
    let (constant, bits) = if atom.tag == *b"stsz" {
        (field, 32)
    } else {
        require(
            matches!(field, 4 | 8 | 16),
            "unsupported compact sample-size field",
        )?;
        (0, field)
    };
    let bytes = if constant == 0 {
        (u64::from(count) * u64::from(bits)).div_ceil(8)
    } else {
        0
    };
    require(
        atom.body.len() == 12 + bytes,
        "sample-size count disagrees with box length",
    )?;
    if constant == 0 {
        r.charge_rows(u64::from(count))?;
    }
    let sizes = Sizes {
        count,
        constant,
        bits,
        data: atom.body.suffix(12)?,
    };
    if constant != 0 {
        r.sample_size(sizes, 0)?;
    } else {
        for ordinal in 0..count {
            r.sample_size(sizes, ordinal)?;
        }
        if bits == 4 && count % 2 == 1 {
            require(
                r.bytes::<1>(sizes.data.end - 1)?[0] & 15 == 0,
                "nonzero compact sample-table padding",
            )?;
        }
    }
    Ok(sizes)
}

fn sample_description(r: &mut Reader<'_>, span: Span) -> Result<Kind> {
    r.full(span, &[0], 0)?;
    require(
        span.len() >= 16 && r.u32(span.start + 4)? == 1,
        "only one sample description per track is admitted",
    )?;
    let mut cursor = span.start + 8;
    let entry = r.atom(&mut cursor, span.end, 6)?;
    require(
        cursor == span.end,
        "sample description count or size disagrees with box",
    )?;
    let (kind, fixed) = match &entry.tag {
        b"avc1" => (Kind::Video, 78),
        b"mp4a" => (Kind::Audio, 28),
        _ => {
            return Err(SourceDecodeError::Native {
                code: "unsupported_codec".into(),
                message: "only avc1 and mp4a MP4 sample descriptions are admitted".into(),
            });
        }
    };
    require(entry.body.len() >= fixed, "truncated sample description")?;
    require(
        r.bytes::<6>(entry.body.start)? == [0; 6] && r.bytes::<2>(entry.body.start + 6)? == [0, 1],
        "sample description is not self-contained",
    )?;
    if kind == Kind::Audio {
        require(
            r.bytes::<8>(entry.body.start + 8)? == [0; 8],
            "versioned QuickTime audio descriptions are unqualified",
        )?;
        let channels = u32::from(u16::from_be_bytes(r.bytes(entry.body.start + 16)?));
        let rate = r.u32(entry.body.start + 24)?;
        if channels == 0
            || channels > r.limits.max_channels
            || rate >> 16 == 0
            || rate >> 16 > r.limits.max_sample_rate
        {
            return Err(limit(
                "MP4 audio sample description exceeds channel or rate limits",
            ));
        }
        require(
            rate & 0xffff == 0 && r.bytes::<2>(entry.body.start + 18)? == [0, 16],
            "unsupported MP4 audio sample description units",
        )?;
    } else {
        let width = u32::from(u16::from_be_bytes(r.bytes(entry.body.start + 24)?));
        let height = u32::from(u16::from_be_bytes(r.bytes(entry.body.start + 26)?));
        require(
            width > 0 && height > 0,
            "MP4 video dimensions must be positive",
        )?;
        if width > r.limits.max_dimension
            || height > r.limits.max_dimension
            || u64::from(width) * u64::from(height) > r.limits.max_pixels
        {
            return Err(limit("MP4 video dimensions exceed configured bounds"));
        }
    }
    let children = entry.body.suffix(fixed)?;
    let mut cursor = children.start;
    let mut config = false;
    let mut color = false;
    let mut bitrate = false;
    let mut aspect = false;
    while cursor < children.end {
        let atom = r.atom(&mut cursor, children.end, 7)?;
        match &atom.tag {
            b"avcC" if kind == Kind::Video => {
                require(!config, "duplicate video configuration")?;
                config = true;
                avcc(r, atom.body)?;
            }
            b"esds" if kind == Kind::Audio => {
                require(!config, "duplicate audio configuration")?;
                config = true;
                esds(r, atom.body)?;
            }
            b"colr" if kind == Kind::Video => {
                require(!color, "duplicate color description")?;
                color = true;
                r.fixed(atom.body, 11)?;
                require(
                    r.bytes::<4>(atom.body.start)? == *b"nclx",
                    "only bounded nclx color metadata is admitted",
                )?;
            }
            b"pasp" if kind == Kind::Video => {
                require(!aspect, "duplicate pixel aspect")?;
                aspect = true;
                r.fixed(atom.body, 8)?;
                require(
                    r.u32(atom.body.start)? > 0 && r.u32(atom.body.start + 4)? > 0,
                    "invalid pixel aspect",
                )?;
            }
            b"btrt" => {
                require(!bitrate, "duplicate bitrate metadata")?;
                bitrate = true;
                r.fixed(atom.body, 12)?;
            }
            _ => return Err(invalid("unqualified sample-description metadata")),
        }
    }
    require(
        config,
        "sample description has no bounded codec configuration",
    )?;
    Ok(kind)
}

fn avcc(r: &mut Reader<'_>, span: Span) -> Result<()> {
    require(
        (7..=EXTRA_BYTES).contains(&span.len()),
        "invalid AVC configuration size",
    )?;
    let bytes = r.bytes::<6>(span.start)?;
    require(
        bytes[0] == 1 && bytes[4] & 0xfc == 0xfc && bytes[4] & 3 != 2 && bytes[5] & 0xe0 == 0xe0,
        "unsupported AVC configuration prefix",
    )?;
    let mut cursor = span.start + 6;
    let sps = bytes[5] & 31;
    require(sps != 0, "AVC configuration has no SPS")?;
    nal_units(r, &mut cursor, span.end, sps)?;
    require(cursor < span.end, "AVC configuration has no PPS count")?;
    let pps = r.bytes::<1>(cursor)?[0];
    cursor += 1;
    require(pps != 0, "AVC configuration has no PPS")?;
    nal_units(r, &mut cursor, span.end, pps)?;
    if cursor < span.end {
        require(
            matches!(bytes[1], 100 | 110 | 122 | 144 | 244),
            "unqualified AVC configuration extension",
        )?;
        require(
            span.end - cursor >= 4,
            "truncated AVC configuration extension",
        )?;
        let extension = r.bytes::<4>(cursor)?;
        cursor += 4;
        require(
            extension[0] & 0xfc == 0xfc
                && extension[1] & 0xf8 == 0xf8
                && extension[2] & 0xf8 == 0xf8,
            "invalid AVC configuration extension",
        )?;
        nal_units(r, &mut cursor, span.end, extension[3])?;
    }
    require(cursor == span.end, "AVC configuration exceeds its box")
}
fn nal_units(r: &mut Reader<'_>, cursor: &mut u64, end: u64, count: u8) -> Result<()> {
    for _ in 0..count {
        require(
            *cursor <= end && end - *cursor >= 2,
            "truncated AVC unit length",
        )?;
        let size = u64::from(u16::from_be_bytes(r.bytes(*cursor)?));
        *cursor += 2;
        require(
            size > 0 && size <= end - *cursor,
            "AVC unit exceeds configuration bounds",
        )?;
        *cursor += size;
    }
    Ok(())
}
fn descriptor(r: &mut Reader<'_>, cursor: &mut u64, end: u64, expected: u8) -> Result<Span> {
    require(
        *cursor < end && r.bytes::<1>(*cursor)?[0] == expected,
        "unsupported MPEG-4 descriptor tag",
    )?;
    *cursor += 1;
    let mut length = 0_u64;
    let mut terminal = false;
    for _ in 0..4 {
        require(*cursor < end, "truncated MPEG-4 descriptor length")?;
        let byte = r.bytes::<1>(*cursor)?[0];
        *cursor += 1;
        length = length * 128 + u64::from(byte & 127);
        if byte & 128 == 0 {
            terminal = true;
            break;
        }
    }
    // FFmpeg-authored fixtures legitimately use padded four-byte lengths.
    require(
        terminal && length <= end - *cursor,
        "MPEG-4 descriptor length exceeds enclosing descriptor",
    )?;
    if length > EXTRA_BYTES {
        return Err(limit("MPEG-4 descriptor exceeds codec header bound"));
    }
    let body = Span {
        start: *cursor,
        end: *cursor + length,
    };
    *cursor = body.end;
    Ok(body)
}
fn esds(r: &mut Reader<'_>, span: Span) -> Result<()> {
    require(
        span.len() <= EXTRA_BYTES,
        "audio configuration exceeds codec header bound",
    )?;
    r.full(span, &[0], 0)?;
    let mut cursor = span.start + 4;
    let es = descriptor(r, &mut cursor, span.end, 3)?;
    require(
        cursor == span.end && es.len() >= 3,
        "unsupported ES descriptor envelope",
    )?;
    require(
        r.bytes::<1>(es.start + 2)?[0] == 0,
        "external or dependent ES descriptors are unqualified",
    )?;
    let mut cursor = es.start + 3;
    let config = descriptor(r, &mut cursor, es.end, 4)?;
    require(
        config.len() >= 13 && r.bytes::<2>(config.start)? == [0x40, 0x15],
        "only AAC audio decoder configuration is admitted",
    )?;
    let mut config_cursor = config.start + 13;
    let specific = descriptor(r, &mut config_cursor, config.end, 5)?;
    require(
        config_cursor == config.end && specific.len() >= 2,
        "invalid AAC-specific descriptor",
    )?;
    if r.bytes::<1>(specific.start)?[0] >> 3 != 2 {
        return Err(SourceDecodeError::Native {
            code: "unsupported_codec".into(),
            message: "only AAC-LC configuration is admitted".into(),
        });
    }
    let sl = descriptor(r, &mut cursor, es.end, 6)?;
    require(
        cursor == es.end && sl.len() == 1 && r.bytes::<1>(sl.start)?[0] == 2,
        "unsupported SL descriptor",
    )
}

fn encoder_metadata(r: &mut Reader<'_>, span: Span) -> Result<()> {
    require(
        span.len() <= 4096,
        "movie metadata exceeds bounded encoder string grammar",
    )?;
    let mut cursor = span.start;
    let meta = r.atom(&mut cursor, span.end, 2)?;
    require(
        cursor == span.end && meta.tag == *b"meta",
        "only one encoder metadata container is admitted",
    )?;
    r.full(meta.body, &[0], 0)?;
    let mut cursor = meta.body.start + 4;
    let mut found_handler = false;
    let mut found_list = false;
    while cursor < meta.body.end {
        let atom = r.atom(&mut cursor, meta.body.end, 3)?;
        match &atom.tag {
            b"hdlr" => {
                require(
                    !found_handler && handler(r, atom.body)? == *b"mdir",
                    "unsupported encoder metadata handler",
                )?;
                found_handler = true;
            }
            b"ilst" => {
                require(!found_list, "duplicate encoder metadata list")?;
                found_list = true;
                let mut item_cursor = atom.body.start;
                let item = r.atom(&mut item_cursor, atom.body.end, 4)?;
                require(
                    item_cursor == atom.body.end && item.tag == [0xa9, b't', b'o', b'o'],
                    "only the encoder-name metadata item is admitted",
                )?;
                let mut value_cursor = item.body.start;
                let value = r.atom(&mut value_cursor, item.body.end, 5)?;
                require(
                    value_cursor == item.body.end
                        && value.tag == *b"data"
                        && (8..=1024).contains(&value.body.len()),
                    "invalid encoder metadata payload",
                )?;
                require(
                    r.u32(value.body.start)? == 1 && r.u32(value.body.start + 4)? == 0,
                    "encoder metadata is not a plain UTF-8 data item",
                )?;
            }
            _ => return Err(invalid("unknown encoder metadata field")),
        }
    }
    require(found_handler && found_list, "incomplete encoder metadata")
}

fn validate_track(r: &mut Reader<'_>, track: &Track, media: Span) -> Result<()> {
    require(
        track.codec.is_some() && track.codec == track.handler,
        "track handler and sample description disagree",
    )?;
    let sizes = track
        .sizes
        .ok_or_else(|| invalid("track lacks sample sizes"))?;
    let (chunks, wide) = track
        .chunks
        .ok_or_else(|| invalid("track lacks chunk offsets"))?;
    let mapping = track
        .mapping
        .ok_or_else(|| invalid("track lacks sample-to-chunk mapping"))?;
    require(
        track.timing_count == Some(u64::from(sizes.count)),
        "timing runs do not cover exactly the declared samples",
    )?;
    require(
        track
            .composition_count
            .is_none_or(|count| count == u64::from(sizes.count)),
        "composition runs do not cover exactly the declared samples",
    )?;
    require(
        track.roll_description == track.roll_samples.is_some(),
        "incomplete roll-group metadata",
    )?;
    require(
        track.roll_samples.is_none_or(|count| count == sizes.count),
        "roll group does not cover declared samples",
    )?;
    if let Some(sync) = track.sync {
        let mut previous = 0;
        for row in 0..sync.rows {
            let sample = r.u32(sync.data.start + u64::from(row) * 4)?;
            require(
                sample > previous && sample <= sizes.count,
                "invalid sync-sample identity",
            )?;
            previous = sample;
        }
    }
    let mut sample = 0_u32;
    let mut previous_end = media.start;
    for row in 0..mapping.rows {
        let at = mapping.data.start + u64::from(row) * 12;
        let first = r.u32(at)?;
        let count = r.u32(at + 4)?;
        let description = r.u32(at + 8)?;
        let next = if row + 1 < mapping.rows {
            r.u32(at + 12)?
        } else {
            chunks.rows + 1
        };
        require(
            first > 0
                && (row != 0 || first == 1)
                && first < next
                && next <= chunks.rows + 1
                && count > 0
                && description == 1,
            "invalid sample-to-chunk run",
        )?;
        let expanded = u64::from(next - first) * u64::from(count);
        require(
            expanded <= u64::from(sizes.count - sample),
            "chunk mapping expands beyond declared samples",
        )?;
        for chunk in first - 1..next - 1 {
            let at = chunks.data.start + u64::from(chunk) * if wide { 8 } else { 4 };
            let offset = if wide {
                r.u64(at)?
            } else {
                u64::from(r.u32(at)?)
            };
            require(
                offset >= previous_end && offset < media.end,
                "chunk offsets overlap or escape media-data bounds",
            )?;
            let mut bytes = 0_u64;
            for _ in 0..count {
                bytes += u64::from(r.sample_size(sizes, sample)?);
                sample += 1;
            }
            require(
                bytes <= media.end - offset,
                "declared media samples extend beyond media-data box",
            )?;
            previous_end = offset + bytes;
        }
    }
    require(
        sample == sizes.count,
        "chunk mapping does not cover exactly the declared samples",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

    static CANCELLED: AtomicBool = AtomicBool::new(false);
    // Keep the original audio guard's tests intact while exercising the shared
    // policy boundary. Production callers validate their own decoder limits.
    fn validate(
        file: &File,
        selected: u32,
        limits: AudioDecodeLimits,
        control: DecodeControl<'_>,
    ) -> Result<u64> {
        limits.validate()?;
        super::validate(file, Selection::Audio(selected), limits.into(), control)
    }
    fn control() -> DecodeControl<'static> {
        DecodeControl {
            timeout: Duration::from_secs(10),
            cancelled: &CANCELLED,
        }
    }
    fn path(directory: &str, name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(directory)
            .join(name)
    }
    fn fixture() -> Vec<u8> {
        std::fs::read(path("fixtures", "cfr-bframes.mp4")).unwrap()
    }
    fn file(bytes: &[u8]) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file
    }
    fn code(error: SourceDecodeError) -> String {
        match error {
            SourceDecodeError::Native { code, .. } => code,
            other => panic!("unexpected error: {other}"),
        }
    }
    fn reject(bytes: &[u8], expected: &str) {
        let error = validate(&file(bytes), 1, AudioDecodeLimits::default(), control()).unwrap_err();
        assert_eq!(code(error), expected);
    }
    fn tag(bytes: &[u8], value: &[u8; 4]) -> usize {
        bytes.windows(4).position(|window| window == value).unwrap()
    }
    fn set32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    fn atom(tag: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
        let mut bytes = (u32::try_from(body.len()).unwrap() + 8)
            .to_be_bytes()
            .to_vec();
        bytes.extend(tag);
        bytes.extend(body);
        bytes
    }
    fn words(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }
    // Synthetic table topology only. Packet bytes are deliberately not a codec
    // fixture; successful preflight says nothing about whether AAC decodes.
    fn small_mp4(bits: u32, wide: bool, constant: bool) -> Vec<u8> {
        chunked_mp4(bits, wide, constant, 1)
    }
    fn chunked_mp4(bits: u32, wide: bool, constant: bool, chunks: u32) -> Vec<u8> {
        described_mp4(Kind::Audio, bits, wide, constant, chunks)
    }
    fn described_mp4(kind: Kind, bits: u32, wide: bool, constant: bool, chunks: u32) -> Vec<u8> {
        let original = fixture();
        let description_start = tag(&original, b"stsd");
        let at = description_start
            + tag(
                &original[description_start..],
                if kind == Kind::Audio {
                    b"mp4a"
                } else {
                    b"avc1"
                },
            )
            - 4;
        let length = u32::from_be_bytes(original[at..at + 4].try_into().unwrap()) as usize;
        let description = original[at..at + length].to_vec();
        let mut stsd = words(&[0, 1]);
        stsd.extend(description);
        let sample_bytes = if constant { 12 } else { 13 };
        let ftyp = atom(b"ftyp", [b"isom".as_slice(), &[0; 4], b"mp41"].concat());
        let offset = u32::try_from(ftyp.len()).unwrap() + 8;
        let sizes = if constant {
            atom(b"stsz", words(&[0, 6, 2 * chunks]))
        } else {
            let mut body = words(&[0, bits, 2 * chunks]);
            for _ in 0..chunks {
                match bits {
                    4 => body.push(0x67),
                    8 => body.extend([6, 7]),
                    16 => body.extend([0, 6, 0, 7]),
                    _ => panic!("test width"),
                }
            }
            atom(b"stz2", body)
        };
        let mut offsets = words(&[0, chunks]);
        for chunk in 0..chunks {
            let offset = offset + chunk * sample_bytes;
            if wide {
                offsets.extend(u64::from(offset).to_be_bytes());
            } else {
                offsets.extend(offset.to_be_bytes());
            }
        }
        let stbl = atom(
            b"stbl",
            [
                atom(b"stsd", stsd),
                atom(b"stts", words(&[0, 1, 2 * chunks, 1024])),
                atom(b"stsc", words(&[0, 1, 1, 2, 1])),
                sizes,
                atom(if wide { b"co64" } else { b"stco" }, offsets),
            ]
            .concat(),
        );
        let dref = atom(
            b"dref",
            [words(&[0, 1, 12]), b"url ".to_vec(), words(&[1])].concat(),
        );
        let header = if kind == Kind::Audio {
            atom(b"smhd", vec![0; 8])
        } else {
            atom(b"vmhd", words(&[1, 0, 0]))
        };
        let minf = atom(b"minf", [header, atom(b"dinf", dref), stbl].concat());
        let mut mdhd = vec![0; 24];
        set32(&mut mdhd, 12, 48_000);
        let mut hdlr = vec![0; 24];
        hdlr[8..12].copy_from_slice(if kind == Kind::Audio {
            b"soun"
        } else {
            b"vide"
        });
        let mdia = atom(
            b"mdia",
            [atom(b"mdhd", mdhd), atom(b"hdlr", hdlr), minf].concat(),
        );
        let mut tkhd = vec![0; 84];
        set32(&mut tkhd, 0, 3);
        set32(&mut tkhd, 12, 1);
        let trak = atom(b"trak", [atom(b"tkhd", tkhd), mdia].concat());
        let mut mvhd = vec![0; 100];
        set32(&mut mvhd, 12, 1000);
        let moov = atom(b"moov", [atom(b"mvhd", mvhd), trak].concat());
        [
            ftyp,
            atom(b"mdat", vec![0; (sample_bytes * chunks) as usize]),
            moov,
        ]
        .concat()
    }

    fn with_tracks(video: u32, audio: u32) -> Vec<u8> {
        let video_file = described_mp4(Kind::Video, 8, false, false, 1);
        let audio_file = small_mp4(8, false, false);
        let movie = tag(&video_file, b"moov") - 4;
        let video_start = tag(&video_file, b"trak") - 4;
        let audio_start = tag(&audio_file, b"trak") - 4;
        let mut body = video_file[movie + 8..video_start].to_vec();
        for index in 0..video + audio {
            let mut track = if index < video {
                video_file[video_start..].to_vec()
            } else {
                audio_file[audio_start..].to_vec()
            };
            let header = tag(&track, b"tkhd");
            set32(&mut track, header + 16, index + 1);
            body.extend(track);
        }
        [video_file[..movie].to_vec(), atom(b"moov", body)].concat()
    }

    fn video_limits() -> InputLimits {
        InputLimits {
            max_pixels: 16_777_216,
            ..AudioDecodeLimits::default().into()
        }
    }

    fn video_admission(file: &File, limits: InputLimits) -> Result<u64> {
        super::validate(file, Selection::Video, limits, control())
    }

    #[test]
    fn first_audio_uses_complete_admitted_inventory_without_extra_header_reads() {
        for (videos, audio, expected) in [(0, 2, 0), (1, 2, 1), (2, 1, 2)] {
            let input = file(&with_tracks(videos, audio));
            let limits = AudioDecodeLimits::default();
            let (selected, first_bytes) =
                super::validate_audio(&input, None, limits.into(), control()).unwrap();
            let (exact, exact_bytes) =
                super::validate_audio(&input, Some(expected), limits.into(), control()).unwrap();
            assert_eq!((selected, exact), (expected, expected));
            assert_eq!(first_bytes, exact_bytes);
        }
        let error = super::validate_audio(
            &file(&with_tracks(1, 0)),
            None,
            AudioDecodeLimits::default().into(),
            control(),
        )
        .unwrap_err();
        assert_eq!(code(error), "unsupported_streams");
    }

    #[test]
    fn every_committed_mp4_passes_video_allocation_admission() {
        use std::io::{Seek, SeekFrom};
        let fixtures = [
            path("fixtures", "cfr-bframes.mp4"),
            path("fixtures", "offset-bframes.mp4"),
            path("fixtures", "vfr.mp4"),
            path("fixtures", "rotated90.mp4"),
            path("../../deadpan-media-worker/tests/fixtures", "rgb1_24.mp4"),
            path(
                "../../deadpan-media-worker/tests/fixtures",
                "rgb2_24_audio.mp4",
            ),
            path("../../deadpan-media-worker/tests/fixtures", "rgb25_24.mp4"),
            path(
                "../../deadpan-media-worker/tests/fixtures",
                "rgb30_30000_1001.mp4",
            ),
            // Admission establishes bounded declarations, not payload or color validity.
            path(
                "../../deadpan-media-worker/tests/fixtures",
                "rgb1_24_corrupt.mp4",
            ),
            path(
                "../../deadpan-media-worker/tests/fixtures",
                "rgb1_24_no_tags.mp4",
            ),
        ];
        for path in fixtures {
            let mut source = File::open(&path).unwrap();
            source.seek(SeekFrom::Start(19)).unwrap();
            let used = video_admission(&source, video_limits())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(used > 0 && used <= HEADER_BYTES);
            assert_eq!(source.stream_position().unwrap(), 19);
        }
    }

    #[test]
    fn video_selection_requires_one_video_and_bounded_audio_inventory() {
        for (video, audio) in [(0, 1), (2, 0)] {
            assert_eq!(
                code(
                    video_admission(&file(&with_tracks(video, audio)), video_limits()).unwrap_err()
                ),
                "unsupported_streams"
            );
        }
        video_admission(&file(&with_tracks(1, 32)), video_limits()).unwrap();
        assert_eq!(
            code(video_admission(&file(&with_tracks(1, 33)), video_limits()).unwrap_err()),
            "resource_limit"
        );
        assert_eq!(
            code(
                video_admission(
                    &File::open(path("audio-fixtures", "pcm-stereo-48000.wav")).unwrap(),
                    video_limits(),
                )
                .unwrap_err()
            ),
            "unsupported_streams"
        );
    }

    #[test]
    fn declared_video_geometry_obeys_dimension_and_pixel_limits() {
        let original = described_mp4(Kind::Video, 8, false, false, 1);
        let dimensions = tag(&original, b"avc1") + 28;
        for (width, height, expected) in [
            (0_u16, 16_u16, "invalid_input"),
            (16, 0, "invalid_input"),
            (8193, 16, "resource_limit"),
            (16, 8193, "resource_limit"),
            (8192, 8192, "resource_limit"),
        ] {
            let mut changed = original.clone();
            changed[dimensions..dimensions + 2].copy_from_slice(&width.to_be_bytes());
            changed[dimensions + 2..dimensions + 4].copy_from_slice(&height.to_be_bytes());
            assert_eq!(
                code(video_admission(&file(&changed), video_limits()).unwrap_err()),
                expected
            );
        }
        for limits in [
            InputLimits {
                max_pixels: 1,
                ..video_limits()
            },
            InputLimits {
                max_dimension: 1,
                ..video_limits()
            },
        ] {
            assert_eq!(
                code(video_admission(&file(&original), limits).unwrap_err()),
                "resource_limit"
            );
        }
    }

    #[test]
    fn video_admission_checks_ignored_audio_counts_and_every_packet_size() {
        let original = with_tracks(1, 1);
        let tables: Vec<_> = original
            .windows(4)
            .enumerate()
            .filter_map(|(index, bytes)| (bytes == b"stz2").then_some(index))
            .collect();
        assert_eq!(tables.len(), 2);
        for table in tables {
            let mut changed = original.clone();
            set32(&mut changed, table + 12, 1_000_001);
            assert_eq!(
                code(video_admission(&file(&changed), video_limits()).unwrap_err()),
                "resource_limit"
            );
            let mut changed = original.clone();
            changed[table + 16] = 8;
            assert_eq!(
                code(
                    video_admission(
                        &file(&changed),
                        InputLimits {
                            max_packet_bytes: 7,
                            ..video_limits()
                        }
                    )
                    .unwrap_err()
                ),
                "resource_limit"
            );
        }
        assert_eq!(
            code(
                video_admission(
                    &file(&original),
                    InputLimits {
                        max_packets: 3,
                        ..video_limits()
                    }
                )
                .unwrap_err()
            ),
            "resource_limit"
        );
        let original = fixture();
        for table in original
            .windows(4)
            .enumerate()
            .filter_map(|(index, bytes)| (bytes == b"stsz").then_some(index))
        {
            let mut changed = original.clone();
            set32(&mut changed, table + 16, 16 * 1024 * 1024 + 1);
            assert_eq!(
                code(video_admission(&file(&changed), video_limits()).unwrap_err()),
                "resource_limit"
            );
        }
    }

    #[test]
    fn sparse_large_table_is_rejected_before_reading_or_allocating_its_rows() {
        let mut source = tempfile::tempfile().unwrap();
        let prefix = atom(b"ftyp", [b"isom".as_slice(), &[0; 4], b"mp41"].concat());
        source.write_all(&prefix).unwrap();
        let rows = 1_000_001_u32;
        let table_size = 16 + rows * 4;
        let parents = [b"moov", b"trak", b"mdia", b"minf", b"stbl"];
        for (index, tag) in parents.iter().enumerate() {
            source
                .write_all(&(table_size + (parents.len() - index) as u32 * 8).to_be_bytes())
                .unwrap();
            source.write_all(*tag).unwrap();
        }
        source.write_all(&table_size.to_be_bytes()).unwrap();
        source.write_all(b"stco").unwrap();
        source.write_all(&words(&[0, rows])).unwrap();
        source
            .set_len(prefix.len() as u64 + parents.len() as u64 * 8 + u64::from(table_size))
            .unwrap();
        assert_eq!(
            code(video_admission(&source, video_limits()).unwrap_err()),
            "resource_limit"
        );
    }

    #[test]
    fn committed_aac_h264_and_pcm_fixtures_pass_without_changing_descriptors() {
        use std::io::{Seek, SeekFrom};
        for name in ["cfr-bframes.mp4", "offset-bframes.mp4", "vfr.mp4"] {
            let mut file = File::open(path("fixtures", name)).unwrap();
            file.seek(SeekFrom::Start(19)).unwrap();
            let used = validate(&file, 1, AudioDecodeLimits::default(), control()).unwrap();
            assert!(used > 0 && used <= HEADER_BYTES);
            assert_eq!(file.stream_position().unwrap(), 19);
        }
        for name in ["pcm-stereo-48000.wav", "pcm-mono-44100.wav"] {
            validate(
                &File::open(path("audio-fixtures", name)).unwrap(),
                0,
                AudioDecodeLimits::default(),
                control(),
            )
            .unwrap();
        }
    }
    #[test]
    fn compact_and_constant_sizes_and_wide_offsets_have_bounded_semantic_coverage() {
        for bits in [4, 8, 16] {
            for wide in [false, true] {
                validate(
                    &file(&small_mp4(bits, wide, false)),
                    0,
                    AudioDecodeLimits::default(),
                    control(),
                )
                .unwrap();
            }
        }
        validate(
            &file(&small_mp4(4, true, true)),
            0,
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
    }
    #[test]
    fn thousands_of_interleaved_chunk_and_size_reads_reuse_bounded_pages() {
        let source = file(&chunked_mp4(16, true, false, 5_000));
        let used = validate(&source, 0, AudioDecodeLimits::default(), control()).unwrap();
        // A single-page cache used more than 16 MiB by rereading a page for
        // each switch between these two tables. The eight-page cache stays
        // within the small, actual header footprint plus its second pass.
        assert!(used < 256 * 1024, "guard reread {used} bytes");
    }
    #[test]
    fn counts_and_expansions_in_unselected_tracks_fail_before_ffmpeg() {
        for (name, relative, value, expected) in [
            (b"stsz", 12, u32::MAX, "resource_limit"),
            (b"stss", 8, u32::MAX, "invalid_input"),
            (b"stts", 12, u32::MAX, "resource_limit"),
            (b"ctts", 12, u32::MAX, "resource_limit"),
            (b"stsc", 16, u32::MAX, "invalid_input"),
            (b"stco", 12, u32::MAX, "invalid_input"),
            (b"stsz", 16, 16 * 1024 * 1024 + 1, "resource_limit"),
        ] {
            let mut bytes = fixture();
            let at = tag(&bytes, name);
            set32(&mut bytes, at + relative, value);
            reject(&bytes, expected);
            assert_eq!(
                code(
                    super::validate_audio(
                        &file(&bytes),
                        None,
                        AudioDecodeLimits::default().into(),
                        control()
                    )
                    .unwrap_err()
                ),
                expected
            );
        }
    }
    #[test]
    fn nested_descriptor_lengths_cannot_escape_a_small_codec_box() {
        let mut bytes = fixture();
        let esds = tag(&bytes, b"esds");
        // Four continuation bytes never terminate, regardless of later bytes.
        bytes[esds + 9..esds + 13].fill(0xff);
        reject(&bytes, "invalid_input");
        let mut bytes = fixture();
        let esds = tag(&bytes, b"esds");
        // Top descriptor fits, but its nested DecoderSpecificInfo declares 2^28-1.
        let specific = bytes[esds..]
            .windows(5)
            .position(|part| part == [5, 0x80, 0x80, 0x80, 5])
            .unwrap()
            + esds;
        bytes[specific + 1..specific + 5].copy_from_slice(&[0xff, 0xff, 0xff, 0x7f]);
        reject(&bytes, "invalid_input");
    }
    #[test]
    fn malformed_extents_and_unsafe_metadata_never_reach_ffmpeg() {
        let bytes = fixture();
        reject(&bytes[..bytes.len() - 1], "invalid_input");
        for value in [b"cmov", b"moof", b"uuid", b"senc"] {
            let mut bytes = fixture();
            let at = tag(&bytes, b"moov");
            bytes[at..at + 4].copy_from_slice(value);
            reject(&bytes, "invalid_input");
        }
        let mut bytes = fixture();
        let at = tag(&bytes, b"esds");
        bytes[at..at + 4].copy_from_slice(b"wave");
        reject(&bytes, "invalid_input");
        let mut bytes = fixture();
        let at = tag(&bytes, b"ctts");
        set32(&mut bytes, at + 12, 1);
        set32(&mut bytes, at + 20, 2);
        reject(&bytes, "invalid_input");
        let mut bytes = fixture();
        let at = tag(&bytes, b"stco");
        let first = u32::from_be_bytes(bytes[at + 12..at + 16].try_into().unwrap());
        set32(&mut bytes, at + 16, first);
        reject(&bytes, "invalid_input");
        let mut bytes = fixture();
        let at = tag(&bytes, b"url ");
        set32(&mut bytes, at + 4, 0);
        reject(&bytes, "invalid_input");
    }
    #[test]
    fn sparse_huge_headers_and_compact_huge_sample_counts_fail_without_large_buffers() {
        let mut file = tempfile::tempfile().unwrap();
        let ftyp = atom(b"ftyp", [b"isom".as_slice(), &[0; 4], b"mp41"].concat());
        file.write_all(&ftyp).unwrap();
        file.write_all(&(32_u32 * 1024 * 1024).to_be_bytes())
            .unwrap();
        file.write_all(b"moov").unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        assert_eq!(
            code(validate(&file, 0, AudioDecodeLimits::default(), control()).unwrap_err()),
            "resource_limit"
        );
        let mut bytes = small_mp4(4, false, true);
        let at = tag(&bytes, b"stsz");
        set32(&mut bytes, at + 12, 1_000_001);
        assert_eq!(
            code(
                validate(
                    &super::tests::file(&bytes),
                    0,
                    AudioDecodeLimits::default(),
                    control()
                )
                .unwrap_err()
            ),
            "resource_limit"
        );
    }
    #[test]
    fn wav_lengths_block_alignment_and_metadata_are_checked_before_demux() {
        let bytes = std::fs::read(path("audio-fixtures", "pcm-stereo-48000.wav")).unwrap();
        for at in [4, 28, 32, 40] {
            let mut changed = bytes.clone();
            changed[at..at + 2].copy_from_slice(&0xffff_u16.to_le_bytes());
            assert!(validate(&file(&changed), 0, AudioDecodeLimits::default(), control()).is_err());
        }
        let mut changed = bytes.clone();
        changed[12..16].copy_from_slice(b"LIST");
        assert_eq!(
            code(
                validate(&file(&changed), 0, AudioDecodeLimits::default(), control()).unwrap_err()
            ),
            "invalid_input"
        );
        let mut truncated = bytes;
        truncated.pop();
        assert_eq!(
            code(
                validate(
                    &file(&truncated),
                    0,
                    AudioDecodeLimits::default(),
                    control()
                )
                .unwrap_err()
            ),
            "invalid_input"
        );
    }
    #[test]
    fn cached_constant_size_work_still_observes_cancellation_and_limits_charge_reads() {
        let bytes = small_mp4(4, false, true);
        let source = file(&bytes);
        let limits = AudioDecodeLimits {
            max_io_bytes_per_call: 100,
            ..AudioDecodeLimits::default()
        };
        assert_eq!(
            code(validate(&source, 0, limits, control()).unwrap_err()),
            "resource_limit"
        );
        let cancelled = AtomicBool::new(false);
        let mut reader = Reader {
            file: &source,
            control: DecodeControl {
                cancelled: &cancelled,
                ..control()
            },
            started: Instant::now(),
            length: bytes.len() as u64,
            cache: std::array::from_fn(|_| Page::default()),
            cache_clock: 0,
            read_bytes: 0,
            header_bytes: 0,
            atoms: 0,
            rows: 0,
            samples: 0,
            limits: AudioDecodeLimits::default().into(),
        };
        reader.bytes::<4>(0).unwrap();
        let sizes = Sizes {
            count: 1_000_000,
            constant: 6,
            bits: 32,
            data: Span { start: 0, end: 0 },
        };
        assert_eq!(reader.sample_size(sizes, 0).unwrap(), 6);
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(code(reader.sample_size(sizes, 1).unwrap_err()), "cancelled");
    }
}
