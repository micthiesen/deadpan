//! Closed container grammar checked before FFmpeg can allocate from header counts.
//! This is allocation admission for immutable snapshots, not media qualification.

use crate::audio::AudioDecodeLimits;
use crate::{ContentLight, DecodeControl, DecodeLimits, MasteringDisplay, SourceDecodeError};
use std::{fs::File, os::unix::fs::FileExt, sync::atomic::Ordering, time::Instant};

mod inspection;
use inspection::{MovieHeader, TrackHeader};
pub use inspection::{
    Mp4AvcConfiguration, Mp4ColorDescription, Mp4Edit, Mp4H264Packet, Mp4HevcConfiguration,
    Mp4HevcPacket, Mp4Inspection, Mp4PacketObservation, Mp4PacketReader, Mp4PresentationTime,
    Mp4TrackInspection, Mp4TrackKind, inspect_mp4,
};

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
    /// sdtp entry count (one byte per sample), checked against stsz.
    dependencies: Option<u64>,
    roll_description: bool,
    roll_samples: Option<u32>,
    header: Option<TrackHeader>,
    media_timescale: u32,
    media_duration: Option<u64>,
    edit_entries: Vec<Mp4Edit>,
    timing: Option<Table>,
    timing_duration: u64,
    composition: Option<(Table, u8)>,
    avc: Option<Mp4AvcConfiguration>,
    hevc: Option<Mp4HevcConfiguration>,
    mastering: Option<MasteringDisplay>,
    content_light: Option<ContentLight>,
    dimensions: Option<[u32; 2]>,
    color: Option<Mp4ColorDescription>,
    pixel_aspect_ratio: Option<[u32; 2]>,
    audio_channels: Option<u32>,
    audio_sample_rate: Option<u32>,
}

struct Mp4Layout {
    movie: MovieHeader,
    moov_before_mdat: bool,
    tracks: Vec<Track>,
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
                    align.is_none() && !data && matches!(length, 16 | 40),
                    "WAVE requires one PCM fmt16 or extensible fmt40 before data",
                )?;
                let bytes = r.bytes::<16>(start)?;
                let format = u16::from_le_bytes([bytes[0], bytes[1]]);
                if !matches!((format, length), (1, 16) | (0xfffe, 40)) {
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
                if format == 0xfffe {
                    wave_extensible_pcm16(r, start + 16, channels)?;
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

fn wave_extensible_pcm16(r: &mut Reader<'_>, start: u64, channels: u32) -> Result<()> {
    // A closed extension, not arbitrary WAVEFORMATEX extradata. The containing
    // fmt40 has already been bounded before any of these fixed-size reads.
    let bytes = r.bytes::<24>(start)?;
    require(
        u16::from_le_bytes([bytes[0], bytes[1]]) == 22
            && u16::from_le_bytes([bytes[2], bytes[3]]) == 16,
        "WAVE extensible PCM requires cbSize22 and sixteen valid bits",
    )?;
    let mask = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
    require(
        mask != 0 && mask & !0x3ffff == 0 && mask.count_ones() == channels,
        "WAVE extensible speaker mask is unspecified, reserved or inconsistent",
    )?;
    // KSDATAFORMAT_SUBTYPE_PCM in RIFF GUID byte order. Other codecs, float
    // data and vendor-defined subtype namespaces remain unqualified.
    require(
        bytes[8..]
            == [
                1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
            ],
        "WAVE extensible subtype is not signed16 PCM",
    )
}

fn mp4(r: &mut Reader<'_>, policy: Selection) -> Result<Option<u32>> {
    let layout = mp4_layout(r)?;
    let tracks = layout.tracks;
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

fn mp4_layout(r: &mut Reader<'_>) -> Result<Mp4Layout> {
    let mut cursor = 0;
    let mut brands = false;
    let mut movie = None;
    let mut moov_before_mdat = false;
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
                    brands && movie.is_none(),
                    "MP4 requires one movie after its file type",
                )?;
                moov_before_mdat = media.is_none();
                movie = Some(movie_box(r, atom.body, &mut tracks)?);
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
        brands && movie.is_some() && !tracks.is_empty(),
        "MP4 lacks its bounded movie header",
    )?;
    let media = media.ok_or_else(|| invalid("MP4 has no media-data box"))?;
    for track in &tracks {
        validate_track(r, track, media)?;
    }
    Ok(Mp4Layout {
        movie: movie.ok_or_else(|| invalid("MP4 has no movie header"))?,
        moov_before_mdat,
        tracks,
    })
}

fn movie_box(r: &mut Reader<'_>, span: Span, tracks: &mut Vec<Track>) -> Result<MovieHeader> {
    let mut cursor = span.start;
    let mut header = None;
    let mut metadata = false;
    while cursor < span.end {
        let atom = r.atom(&mut cursor, span.end, 1)?;
        match &atom.tag {
            b"mvhd" => {
                require(header.is_none(), "duplicate movie header")?;
                let version = r.full(atom.body, &[0, 1], 0)?;
                r.fixed(atom.body, if version == 0 { 100 } else { 112 })?;
                let timescale = r.u32(atom.body.start + if version == 0 { 12 } else { 20 })?;
                require(
                    timescale > 0 && timescale <= i32::MAX as u32,
                    "invalid movie time scale",
                )?;
                header = Some(MovieHeader {
                    timescale,
                    duration: inspection::duration(r, atom.body, version, 16, 24)?,
                    matrix: inspection::matrix(
                        r,
                        atom.body.start + if version == 0 { 36 } else { 48 },
                    )?,
                });
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
    header.ok_or_else(|| invalid("MP4 lacks movie header"))
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
                track.header = Some(inspection::track_header(r, atom.body, version, full)?);
            }
            b"edts" => {
                require(!track.edits, "duplicate track edits")?;
                track.edits = true;
                track.edit_entries = edit_box(r, atom.body)?;
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

fn edit_box(r: &mut Reader<'_>, span: Span) -> Result<Vec<Mp4Edit>> {
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
    let mut entries = Vec::with_capacity(2);
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
        entries.push(Mp4Edit {
            segment_duration: duration,
            media_time: time,
            media_rate_integer: (rate >> 16) as i16,
            media_rate_fraction: rate as i16,
        });
    }
    Ok(entries)
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
                track.media_timescale = timescale;
                track.media_duration = inspection::duration(r, atom.body, version, 16, 24)?;
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
                track.codec = Some(sample_description(r, atom.body, track)?);
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
                    track.composition = Some((table, version));
                } else {
                    track.timing_count = Some(samples);
                    track.timing = Some(table);
                    track.timing_duration = duration;
                }
            }
            // Sample dependency flags (one byte per sample); FFmpeg derives
            // only disposable-packet flags from them.
            b"sdtp" => {
                require(
                    track.dependencies.is_none(),
                    "duplicate sample dependency table",
                )?;
                r.full(atom.body, &[0], 0)?;
                let count = atom.body.len() - 4;
                r.charge_rows(count)?;
                track.dependencies = Some(count);
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

fn sample_description(r: &mut Reader<'_>, span: Span, track: &mut Track) -> Result<Kind> {
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
        b"avc1" | b"hvc1" => (Kind::Video, 78),
        b"mp4a" => (Kind::Audio, 28),
        // hev1 permits parameter sets that exist only in-band and may change
        // between pictures; only hvc1's complete hvcC arrays are admitted.
        b"hev1" => {
            return Err(SourceDecodeError::Native {
                code: "unsupported_codec".into(),
                message: "hev1 HEVC sample entries with in-band parameter sets are not admitted"
                    .into(),
            });
        }
        _ => {
            return Err(SourceDecodeError::Native {
                code: "unsupported_codec".into(),
                message: "only avc1, hvc1 and mp4a MP4 sample descriptions are admitted".into(),
            });
        }
    };
    let hevc = entry.tag == *b"hvc1";
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
        track.audio_channels = Some(channels);
        track.audio_sample_rate = Some(rate >> 16);
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
        track.dimensions = Some([width, height]);
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
    let mut field = false;
    while cursor < children.end {
        let atom = r.atom(&mut cursor, children.end, 7)?;
        match &atom.tag {
            b"hvcC" if hevc => {
                require(!config, "duplicate video configuration")?;
                config = true;
                track.hevc = Some(hvcc(r, atom.body)?);
            }
            b"mdcv" if kind == Kind::Video => {
                require(track.mastering.is_none(), "duplicate mastering display")?;
                track.mastering = Some(mdcv(r, atom.body)?);
            }
            b"clli" if kind == Kind::Video => {
                require(
                    track.content_light.is_none(),
                    "duplicate content light level",
                )?;
                r.fixed(atom.body, 4)?;
                let bytes = r.bytes::<4>(atom.body.start)?;
                track.content_light = Some(ContentLight {
                    max_cll: u16::from_be_bytes([bytes[0], bytes[1]]),
                    max_fall: u16::from_be_bytes([bytes[2], bytes[3]]),
                });
            }
            // Progressive only; the decoder independently rejects interlace.
            b"fiel" if kind == Kind::Video => {
                require(!field, "duplicate field description")?;
                field = true;
                r.fixed(atom.body, 2)?;
                require(
                    r.bytes::<2>(atom.body.start)? == [1, 0],
                    "only progressive field descriptions are admitted",
                )?;
            }
            b"avcC" if kind == Kind::Video && !hevc => {
                require(!config, "duplicate video configuration")?;
                config = true;
                avcc(r, atom.body)?;
                let prefix = r.bytes::<5>(atom.body.start)?;
                track.avc = Some(Mp4AvcConfiguration {
                    profile: prefix[1],
                    compatibility: prefix[2],
                    level: prefix[3],
                    nal_length_bytes: (prefix[4] & 3) + 1,
                });
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
                let range_byte = r.bytes::<1>(atom.body.start + 10)?[0];
                track.color = Some(Mp4ColorDescription {
                    primaries: u16::from_be_bytes(r.bytes(atom.body.start + 4)?),
                    transfer: u16::from_be_bytes(r.bytes(atom.body.start + 6)?),
                    matrix: u16::from_be_bytes(r.bytes(atom.body.start + 8)?),
                    full_range: range_byte & 128 != 0,
                    range_byte,
                });
            }
            b"pasp" if kind == Kind::Video => {
                require(!aspect, "duplicate pixel aspect")?;
                aspect = true;
                r.fixed(atom.body, 8)?;
                require(
                    r.u32(atom.body.start)? > 0 && r.u32(atom.body.start + 4)? > 0,
                    "invalid pixel aspect",
                )?;
                track.pixel_aspect_ratio =
                    Some([r.u32(atom.body.start)?, r.u32(atom.body.start + 4)?]);
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
/// ISO/IEC 14496-15 8.3.3.1 HEVCDecoderConfigurationRecord, closed to one
/// Main10 4:2:0 ten-bit single-layer stream: complete VPS/SPS/PPS arrays plus
/// optional prefix/suffix SEI, at most 8 arrays and 64 units in 64 KiB.
fn hvcc(r: &mut Reader<'_>, span: Span) -> Result<Mp4HevcConfiguration> {
    require(
        (23..=EXTRA_BYTES).contains(&span.len()),
        "invalid HEVC configuration size",
    )?;
    let head = r.bytes::<23>(span.start)?;
    require(
        head[0] == 1
            && head[13] & 0xf0 == 0xf0
            && head[15] & 0xfc == 0xfc
            && head[16] & 0xfc == 0xfc
            && head[17] & 0xf8 == 0xf8
            && head[18] & 0xf8 == 0xf8,
        "unsupported HEVC configuration prefix",
    )?;
    let configuration = Mp4HevcConfiguration {
        profile_space: head[1] >> 6,
        tier: (head[1] >> 5) & 1,
        profile_idc: head[1] & 31,
        level_idc: head[12],
        chroma_format_idc: head[16] & 3,
        bit_depth_luma: (head[17] & 7) + 8,
        bit_depth_chroma: (head[18] & 7) + 8,
        nal_length_bytes: (head[21] & 3) + 1,
        vps_count: 0,
        sps_count: 0,
        pps_count: 0,
        sei_count: 0,
        sps_coded_size: [0, 0],
        sps_cropped_size: [0, 0],
    };
    if configuration.profile_space != 0
        || configuration.profile_idc != 2
        || configuration.chroma_format_idc != 1
        || configuration.bit_depth_luma != 10
        || configuration.bit_depth_chroma != 10
        || configuration.nal_length_bytes == 3
    {
        return Err(SourceDecodeError::Native {
            code: "unsupported_codec".into(),
            message: "only HEVC Main10 4:2:0 ten-bit with 1, 2 or 4 byte NAL lengths is admitted"
                .into(),
        });
    }
    let mut configuration = configuration;
    let arrays = head[22];
    require(
        arrays <= 8,
        "HEVC configuration declares too many NAL arrays",
    )?;
    let mut cursor = span.start + 23;
    let mut seen = 0_u64;
    let mut units = 0_u32;
    for _ in 0..arrays {
        require(span.end - cursor >= 3, "truncated HEVC configuration array")?;
        let header = r.bytes::<3>(cursor)?;
        cursor += 3;
        let kind = header[0] & 63;
        let count = u16::from_be_bytes([header[1], header[2]]);
        require(
            header[0] & 64 == 0 && seen & (1 << kind) == 0,
            "invalid or duplicate HEVC configuration array",
        )?;
        seen |= 1 << kind;
        let parameter_set = (32..=34).contains(&kind);
        require(
            parameter_set || matches!(kind, 39 | 40),
            "HEVC configuration carries an unqualified NAL array",
        )?;
        require(
            !parameter_set || (header[0] & 128 != 0 && count > 0),
            "HEVC parameter-set arrays must be complete and nonempty",
        )?;
        units += u32::from(count);
        require(units <= 64, "HEVC configuration has too many NAL units")?;
        for _ in 0..count {
            require(span.end - cursor >= 2, "truncated HEVC unit length")?;
            let size = u64::from(u16::from_be_bytes(r.bytes(cursor)?));
            cursor += 2;
            require(
                size >= 2 && size <= span.end - cursor,
                "HEVC unit exceeds configuration bounds",
            )?;
            let nal = r.bytes::<2>(cursor)?;
            require(
                (nal[0] >> 1) & 63 == kind
                    && nal[0] & 0x81 == 0
                    && nal[1] >> 3 == 0
                    && nal[1] & 7 != 0,
                "HEVC configuration unit header disagrees with its array",
            )?;
            if kind == 33 {
                let sps = hevc_sps_prefix(r, cursor, size)?;
                let geometry = hevc_sps_geometry(&sps)?;
                let [width, height] = geometry.coded;
                if width > r.limits.max_dimension
                    || height > r.limits.max_dimension
                    || u64::from(width) * u64::from(height) > r.limits.max_pixels
                {
                    return Err(limit(
                        "HEVC SPS picture size exceeds configured dimension or pixel bounds",
                    ));
                }
                if configuration.sps_coded_size == [0, 0] {
                    configuration.sps_coded_size = geometry.coded;
                    configuration.sps_cropped_size = geometry.cropped;
                }
            }
            cursor += size;
        }
        let count = u8::try_from(count).map_err(|_| limit("HEVC unit count overflow"))?;
        match kind {
            32 => configuration.vps_count = count,
            33 => configuration.sps_count = count,
            34 => configuration.pps_count = count,
            _ => configuration.sei_count += count,
        }
    }
    require(
        cursor == span.end,
        "HEVC configuration length disagrees with its arrays",
    )?;
    require(
        seen & (7 << 32) == 7 << 32,
        "HEVC configuration lacks VPS, SPS or PPS",
    )?;
    Ok(configuration)
}
/// Bytes read from each SPS: enough for the largest profile_tier_level plus
/// every Exp-Golomb field through the conformance window, with emulation
/// prevention bytes.
const HEVC_SPS_PREFIX: u64 = 512;

fn hevc_sps_prefix(r: &mut Reader<'_>, start: u64, size: u64) -> Result<Vec<u8>> {
    let length = size.min(HEVC_SPS_PREFIX);
    let mut bytes = Vec::with_capacity(length as usize);
    let mut at = start;
    while at < start + length {
        if start + length - at >= 64 {
            bytes.extend_from_slice(&r.bytes::<64>(at)?);
            at += 64;
        } else {
            bytes.push(r.bytes::<1>(at)?[0]);
            at += 1;
        }
    }
    Ok(bytes)
}

/// SPS picture geometry in luma samples: the coded size the decoder
/// allocates and the conformance-window cropped size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HevcSpsGeometry {
    coded: [u32; 2],
    cropped: [u32; 2],
}

/// Big-endian RBSP bit reader over a NAL unit that removes emulation
/// prevention bytes (00 00 03).
struct Rbsp<'a> {
    bytes: &'a [u8],
    index: usize,
    zeros: u8,
    current: u8,
    remaining: u8,
}
impl Rbsp<'_> {
    fn bit(&mut self) -> Result<u32> {
        if self.remaining == 0 {
            loop {
                let byte = *self
                    .bytes
                    .get(self.index)
                    .ok_or_else(|| invalid("truncated HEVC SPS"))?;
                self.index += 1;
                if self.zeros >= 2 && byte == 3 {
                    self.zeros = 0;
                    continue;
                }
                self.zeros = if byte == 0 {
                    self.zeros.saturating_add(1)
                } else {
                    0
                };
                self.current = byte;
                self.remaining = 8;
                break;
            }
        }
        self.remaining -= 1;
        Ok(u32::from((self.current >> self.remaining) & 1))
    }
    fn bits(&mut self, count: u32) -> Result<u32> {
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | self.bit()?;
        }
        Ok(value)
    }
    fn skip(&mut self, count: u32) -> Result<()> {
        for _ in 0..count {
            self.bit()?;
        }
        Ok(())
    }
    /// Unsigned Exp-Golomb, limited to 32-bit values.
    fn ue(&mut self) -> Result<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            require(zeros <= 31, "HEVC SPS Exp-Golomb value exceeds 32 bits")?;
        }
        let value = (1_u64 << zeros) - 1 + u64::from(self.bits(zeros)?);
        u32::try_from(value).map_err(|_| invalid("HEVC SPS Exp-Golomb value exceeds 32 bits"))
    }
}

/// ITU-T H.265 7.3.2.2.1 through the conformance window: the SPS picture size
/// governs the decoder's allocation, independently of the sample entry.
fn hevc_sps_geometry(nal: &[u8]) -> Result<HevcSpsGeometry> {
    require(nal.len() > 2, "truncated HEVC SPS")?;
    let mut rbsp = Rbsp {
        bytes: &nal[2..],
        index: 0,
        zeros: 0,
        current: 0,
        remaining: 0,
    };
    rbsp.skip(4)?; // sps_video_parameter_set_id
    let sub_layers = rbsp.bits(3)?; // sps_max_sub_layers_minus1
    require(sub_layers <= 6, "HEVC SPS declares too many sub-layers")?;
    rbsp.skip(1)?; // sps_temporal_id_nesting_flag
    // profile_tier_level(1, sps_max_sub_layers_minus1)
    rbsp.skip(88 + 8)?;
    let mut present = [(false, false); 7];
    for flags in present.iter_mut().take(sub_layers as usize) {
        *flags = (rbsp.bit()? == 1, rbsp.bit()? == 1);
    }
    if sub_layers > 0 {
        rbsp.skip(2 * (8 - sub_layers))?;
    }
    for (profile, level) in present.iter().take(sub_layers as usize) {
        if *profile {
            rbsp.skip(88)?;
        }
        if *level {
            rbsp.skip(8)?;
        }
    }
    require(rbsp.ue()? <= 15, "invalid HEVC SPS identifier")?;
    if rbsp.ue()? != 1 {
        return Err(SourceDecodeError::Native {
            code: "unsupported_codec".into(),
            message: "HEVC SPS chroma format disagrees with admitted 4:2:0".into(),
        });
    }
    let coded = [rbsp.ue()?, rbsp.ue()?];
    require(
        coded[0] > 0 && coded[1] > 0,
        "HEVC SPS picture size must be positive",
    )?;
    let mut cropped = coded;
    if rbsp.bit()? == 1 {
        // 4:2:0: SubWidthC = SubHeightC = 2.
        let window = [rbsp.ue()?, rbsp.ue()?, rbsp.ue()?, rbsp.ue()?];
        let horizontal = 2 * (u64::from(window[0]) + u64::from(window[1]));
        let vertical = 2 * (u64::from(window[2]) + u64::from(window[3]));
        require(
            horizontal < u64::from(coded[0]) && vertical < u64::from(coded[1]),
            "HEVC SPS conformance window removes the whole picture",
        )?;
        let remaining = |size: u32, removed: u64| {
            u32::try_from(u64::from(size) - removed)
                .map_err(|_| invalid("invalid HEVC SPS conformance window"))
        };
        cropped = [
            remaining(coded[0], horizontal)?,
            remaining(coded[1], vertical)?,
        ];
    }
    Ok(HevcSpsGeometry { coded, cropped })
}
/// SMPTE ST 2086 `mdcv` body: display primaries in G, B, R order (as in the
/// HEVC SEI), white point, then max and min luminance. Returned in R, G, B.
/// Only the box size is grammar: semantically invalid values are ignored with
/// a recorded note after decoding (the shared `deadpan_core` rule set).
fn mdcv(r: &mut Reader<'_>, span: Span) -> Result<MasteringDisplay> {
    r.fixed(span, 24)?;
    let bytes = r.bytes::<24>(span.start)?;
    let word = |at: usize| u16::from_be_bytes([bytes[at], bytes[at + 1]]);
    let long = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
    Ok(MasteringDisplay {
        primaries: [[word(8), word(10)], [word(0), word(2)], [word(4), word(6)]],
        white_point: [word(12), word(14)],
        max_luminance: long(16),
        min_luminance: long(20),
    })
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
                // FFmpeg's bitexact muxer writes an empty list without an
                // encoder name; that is equally inert.
                if atom.body.len() == 0 {
                    continue;
                }
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
        track
            .dependencies
            .is_none_or(|count| count == u64::from(sizes.count)),
        "sample dependency table does not cover exactly the declared samples",
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
            path("fixtures", "hevc-pq.mp4"),
            path("fixtures", "hevc-hlg.mp4"),
            path("fixtures", "h264-high10-pq.mp4"),
            path("fixtures", "hevc-ten-bit-sdr.mp4"),
            path("fixtures", "hevc-pq-bt709.mp4"),
            path("fixtures", "hevc-pq-mastering-change.mp4"),
            path("fixtures", "hdr-pq-av.mp4"),
            path("fixtures", "hdr-hlg-av.mp4"),
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
    fn extensible_pcm16_requires_exact_fields_and_declared_speaker_positions() {
        let source = std::fs::read(path("audio-fixtures", "pcm-mono-44100.wav")).unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&u32::try_from(source.len() + 16).unwrap().to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&40_u32.to_le_bytes());
        bytes.extend_from_slice(&0xfffe_u16.to_le_bytes());
        bytes.extend_from_slice(&source[22..36]);
        bytes.extend_from_slice(&22_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&[
            1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
        ]);
        bytes.extend_from_slice(&source[36..]);
        validate(&file(&bytes), 0, AudioDecodeLimits::default(), control()).unwrap();
        // Fixed format/tag pairing, extension size, valid width and every GUID
        // byte are admitted explicitly, before FFmpeg sees the container.
        for (offset, value) in [
            (16, 39),
            (16, 41),
            (20, 1),
            (36, 21),
            (36, 23),
            (38, 15),
            (38, 17),
        ] {
            let mut bad = bytes.clone();
            bad[offset] = value;
            assert!(
                validate(&file(&bad), 0, AudioDecodeLimits::default(), control()).is_err(),
                "offset {offset} value {value}"
            );
        }
        for offset in 44..60 {
            let mut bad = bytes.clone();
            bad[offset] ^= 1;
            assert!(
                validate(&file(&bad), 0, AudioDecodeLimits::default(), control()).is_err(),
                "GUID byte {offset}"
            );
        }
        for mask in [0_u32, 3, 1 << 18, 1 << 31] {
            let mut bad = bytes.clone();
            bad[40..44].copy_from_slice(&mask.to_le_bytes());
            assert!(
                validate(&file(&bad), 0, AudioDecodeLimits::default(), control()).is_err(),
                "mask {mask}"
            );
        }
        // Every canonical WAVE speaker bit is bounded and admitted as declared;
        // the downstream stereo matrix still rejects unsupported interpretations.
        for speaker in 0..18 {
            let mut declared = bytes.clone();
            declared[40..44].copy_from_slice(&(1_u32 << speaker).to_le_bytes());
            validate(&file(&declared), 0, AudioDecodeLimits::default(), control()).unwrap();
        }
        assert_eq!(
            code(
                validate(
                    &file(&bytes),
                    0,
                    AudioDecodeLimits {
                        max_sample_rate: 44_099,
                        ..AudioDecodeLimits::default()
                    },
                    control()
                )
                .unwrap_err()
            ),
            "resource_limit"
        );
        assert_eq!(
            code(
                validate(
                    &file(&bytes),
                    0,
                    AudioDecodeLimits {
                        max_decoded_samples: 44_116,
                        ..AudioDecodeLimits::default()
                    },
                    control()
                )
                .unwrap_err()
            ),
            "resource_limit"
        );
        assert!(validate(&file(&bytes), 1, AudioDecodeLimits::default(), control()).is_err());
        let mut truncated = bytes;
        truncated.truncate(59);
        let declared = u32::try_from(truncated.len() - 8).unwrap();
        truncated[4..8].copy_from_slice(&declared.to_le_bytes());
        assert!(
            validate(
                &file(&truncated),
                0,
                AudioDecodeLimits::default(),
                control()
            )
            .is_err()
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
    fn with_reader<T>(
        bytes: &[u8],
        parse: impl FnOnce(&mut Reader<'_>, Span) -> Result<T>,
    ) -> Result<T> {
        let source = file(bytes);
        let mut reader = Reader {
            file: &source,
            control: control(),
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
        parse(
            &mut reader,
            Span {
                start: 0,
                end: bytes.len() as u64,
            },
        )
    }
    fn hevc_configuration() -> Vec<u8> {
        let bytes = std::fs::read(path("fixtures", "hevc-pq.mp4")).unwrap();
        let at = tag(&bytes, b"hvcC");
        let size = u32::from_be_bytes(bytes[at - 4..at].try_into().unwrap()) as usize;
        bytes[at + 4..at - 4 + size].to_vec()
    }

    #[test]
    fn hvcc_admits_only_bounded_complete_main10_configurations() {
        let original = hevc_configuration();
        let parsed = with_reader(&original, hvcc).unwrap();
        assert_eq!(
            (
                parsed.profile_idc,
                parsed.bit_depth_luma,
                parsed.nal_length_bytes
            ),
            (2, 10, 4)
        );
        assert_eq!(
            (
                parsed.vps_count,
                parsed.sps_count,
                parsed.pps_count,
                parsed.sei_count
            ),
            (1, 1, 1, 2)
        );
        let reject = |edit: &dyn Fn(&mut Vec<u8>), expected: &str| {
            let mut bytes = original.clone();
            edit(&mut bytes);
            assert_eq!(code(with_reader(&bytes, hvcc).unwrap_err()), expected);
        };
        reject(&|b| b[0] = 2, "invalid_input"); // configuration version
        reject(&|b| b[1] = 1, "unsupported_codec"); // Main, not Main10
        reject(&|b| b[16] = 0xfe, "unsupported_codec"); // 4:2:2
        reject(&|b| b[17] = 0xf8, "unsupported_codec"); // eight-bit luma
        reject(&|b| b[21] = (b[21] & !3) | 2, "unsupported_codec"); // 3-byte NAL lengths
        reject(&|b| b[13] = 0, "invalid_input"); // reserved bits
        reject(&|b| b[22] = 9, "invalid_input"); // too many arrays
        reject(&|b| b.truncate(b.len() - 1), "invalid_input"); // unit escapes
        reject(&|b| b.push(0), "invalid_input"); // trailing bytes
        reject(&|b| b.truncate(22), "invalid_input"); // truncated header
        // VPS array marked incomplete; then relabeled as an unqualified type.
        reject(&|b| b[23] &= 0x7f, "invalid_input");
        reject(&|b| b[23] = 0x80 | 35, "invalid_input");
        // Unit header type disagrees with its array, or is multilayer.
        reject(&|b| b[28] = 0x42, "invalid_input");
        reject(&|b| b[29] = 0x09, "invalid_input");
        // Dropping the PPS array leaves an incomplete configuration.
        let pps = original.windows(3).position(|w| w == [0xa2, 0, 1]).unwrap();
        let length = usize::from(u16::from_be_bytes([original[pps + 3], original[pps + 4]]));
        let mut without = original.clone();
        without.drain(pps..pps + 5 + length);
        without[22] -= 1;
        assert_eq!(
            code(with_reader(&without, hvcc).unwrap_err()),
            "invalid_input"
        );
        // Size bound: 64 KiB.
        let mut large = original.clone();
        large.resize(64 * 1024 + 1, 0);
        assert_eq!(
            code(with_reader(&large, hvcc).unwrap_err()),
            "invalid_input"
        );
    }

    /// A Main10 4:2:0 SPS NAL with the given luma size and optional 4:2:0
    /// conformance window (left, right, top, bottom), emulation-prevented.
    fn hevc_sps(width: u32, height: u32, window: Option<[u32; 4]>, sub_layers: u32) -> Vec<u8> {
        hevc_sps_chroma(1, width, height, window, sub_layers)
    }
    fn hevc_sps_chroma(
        chroma: u32,
        width: u32,
        height: u32,
        window: Option<[u32; 4]>,
        sub_layers: u32,
    ) -> Vec<u8> {
        let mut bits = Vec::<bool>::new();
        let put = |bits: &mut Vec<bool>, value: u64, count: u32| {
            for index in (0..count).rev() {
                bits.push(index < 64 && (value >> index) & 1 == 1);
            }
        };
        let ue = |bits: &mut Vec<bool>, value: u32| {
            let coded = u64::from(value) + 1;
            let length = 64 - coded.leading_zeros();
            put(bits, 0, length - 1);
            put(bits, coded, length);
        };
        put(&mut bits, 0, 4);
        put(&mut bits, u64::from(sub_layers), 3);
        put(&mut bits, 1, 1);
        // general: space 0, tier 0, Main10; compatibility; progressive and
        // frame-only; 43 + 1 reserved zero bits; level 3.1.
        put(&mut bits, 2, 8);
        put(&mut bits, 0x2000_0000, 32);
        put(&mut bits, 0b1001, 4);
        put(&mut bits, 0, 44);
        put(&mut bits, 93, 8);
        for _ in 0..sub_layers {
            put(&mut bits, 0b11, 2); // both sub-layer profile and level present
        }
        if sub_layers > 0 {
            put(&mut bits, 0, 2 * (8 - sub_layers));
        }
        for _ in 0..sub_layers {
            put(&mut bits, 0, 88);
            put(&mut bits, 90, 8);
        }
        ue(&mut bits, 0);
        ue(&mut bits, chroma);
        ue(&mut bits, width);
        ue(&mut bits, height);
        put(&mut bits, u64::from(window.is_some()), 1);
        for value in window.into_iter().flatten() {
            ue(&mut bits, value);
        }
        bits.push(true); // rbsp_stop_one_bit
        while !bits.len().is_multiple_of(8) {
            bits.push(false);
        }
        let mut nal = vec![0x42, 0x01];
        let mut zeros = 0;
        for chunk in bits.chunks(8) {
            let byte = chunk
                .iter()
                .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit));
            if zeros >= 2 && byte <= 3 {
                nal.push(3);
                zeros = 0;
            }
            zeros = if byte == 0 { zeros + 1 } else { 0 };
            nal.push(byte);
        }
        nal
    }

    /// Replace the hvcC configuration's single SPS unit.
    fn with_sps(configuration: &[u8], sps: &[u8]) -> Vec<u8> {
        let at = configuration
            .windows(3)
            .position(|w| w == [0xa1, 0, 1])
            .unwrap();
        let length = usize::from(u16::from_be_bytes([
            configuration[at + 3],
            configuration[at + 4],
        ]));
        let mut out = configuration[..at + 3].to_vec();
        out.extend(u16::try_from(sps.len()).unwrap().to_be_bytes());
        out.extend(sps);
        out.extend(&configuration[at + 5 + length..]);
        out
    }

    #[test]
    fn hevc_sps_picture_size_is_checked_before_decoder_allocation() {
        let original = hevc_configuration();
        let parsed = with_reader(&original, hvcc).unwrap();
        // The encoder codes 64x40 and crops 4 rows: the decoder allocates 64x40.
        assert_eq!(
            (parsed.sps_coded_size, parsed.sps_cropped_size),
            ([64, 40], [64, 36])
        );
        for (width, height, window, layers, coded, cropped) in [
            (64, 40, Some([0, 0, 0, 2]), 0, [64, 40], [64, 36]),
            (
                1920,
                1088,
                Some([0, 0, 0, 4]),
                2,
                [1920, 1088],
                [1920, 1080],
            ),
            (8192, 8192, None, 6, [8192, 8192], [8192, 8192]),
            // 2^16 has many zero bits, exercising emulation prevention.
            (
                4096,
                2048,
                Some([1, 1, 0, 0]),
                0,
                [4096, 2048],
                [4092, 2048],
            ),
        ] {
            let sps = hevc_sps(width, height, window, layers);
            assert_eq!(
                hevc_sps_geometry(&sps).unwrap(),
                HevcSpsGeometry { coded, cropped }
            );
            let parsed = with_reader(&with_sps(&original, &sps), hvcc).unwrap();
            assert_eq!(
                (parsed.sps_coded_size, parsed.sps_cropped_size),
                (coded, cropped)
            );
        }
        assert!(
            hevc_sps(4096, 2048, None, 0)
                .windows(3)
                .any(|w| w == [0, 0, 3])
        );
        // Oversized SPS pictures fail as resource limits before FFmpeg sees
        // them (default preflight limits: 8192 per side, 8192^2 pixels).
        for (width, height) in [(8200, 64), (64, 8200), (8192, 8200)] {
            let bytes = with_sps(&original, &hevc_sps(width, height, None, 0));
            assert_eq!(
                code(with_reader(&bytes, hvcc).unwrap_err()),
                "resource_limit"
            );
        }
        for (sps, expected) in [
            (hevc_sps(0, 64, None, 0), "invalid_input"),
            (hevc_sps(64, 40, Some([16, 16, 0, 0]), 0), "invalid_input"),
            (hevc_sps(64, 40, None, 7), "invalid_input"),
            (hevc_sps(64, 40, None, 0)[..10].to_vec(), "invalid_input"),
        ] {
            assert_eq!(
                code(with_reader(&with_sps(&original, &sps), hvcc).unwrap_err()),
                expected
            );
        }
        let wrong_chroma = hevc_sps_chroma(2, 64, 40, None, 0);
        assert_eq!(
            code(with_reader(&with_sps(&original, &wrong_chroma), hvcc).unwrap_err()),
            "unsupported_codec"
        );
    }

    #[test]
    fn hevc_sps_size_governs_container_preflight_independently_of_the_sample_entry() {
        let original = std::fs::read(path("fixtures", "hevc-pq.mp4")).unwrap();
        // The sample entry says 64x36 (2304 pixels); the SPS allocates 64x40.
        let tight = InputLimits {
            max_pixels: 64 * 36,
            ..video_limits()
        };
        assert_eq!(
            code(video_admission(&file(&original), tight).unwrap_err()),
            "resource_limit"
        );
        video_admission(
            &file(&original),
            InputLimits {
                max_pixels: 64 * 40,
                ..video_limits()
            },
        )
        .unwrap();
        // A same-length SPS declaring 8320x64 behind a 64x36 sample entry.
        let configuration = hevc_configuration();
        let at = configuration
            .windows(3)
            .position(|w| w == [0xa1, 0, 1])
            .unwrap();
        let length = usize::from(u16::from_be_bytes([
            configuration[at + 3],
            configuration[at + 4],
        ]));
        let mut sps = hevc_sps(8320, 64, None, 0);
        assert!(sps.len() <= length);
        sps.resize(length, 0x80);
        let unit = at + 5 + tag(&original, b"hvcC") + 4;
        let mut forged = original.clone();
        forged[unit..unit + length].copy_from_slice(&sps);
        assert_eq!(
            code(video_admission(&file(&forged), video_limits()).unwrap_err()),
            "resource_limit"
        );
    }

    #[test]
    fn mdcv_converts_stored_green_blue_red_order_and_leaves_values_to_the_shared_rule() {
        let mut body = Vec::new();
        for value in [
            13_250_u16, 34_500, 7_500, 3_000, 34_000, 16_000, 15_635, 16_450,
        ] {
            body.extend(value.to_be_bytes());
        }
        body.extend(10_000_000_u32.to_be_bytes());
        body.extend(1_u32.to_be_bytes());
        let display = with_reader(&body, mdcv).unwrap();
        assert_eq!(
            display.primaries,
            [[34_000, 16_000], [13_250, 34_500], [7_500, 3_000]]
        );
        assert_eq!(display.white_point, [15_635, 16_450]);
        assert_eq!(
            (display.max_luminance, display.min_luminance),
            (10_000_000, 1)
        );
        assert!(display.is_valid());
        // Out-of-range values are not container grammar: they parse, fail the
        // shared rule and are later reported as ignored, not refused.
        let mut bad = body.clone();
        bad[20..24].copy_from_slice(&10_000_000_u32.to_be_bytes());
        let display = with_reader(&bad, mdcv).unwrap();
        assert_eq!(display.min_luminance, display.max_luminance);
        assert!(!display.is_valid());
        assert_eq!(
            code(with_reader(&body[..23], mdcv).unwrap_err()),
            "invalid_input"
        );
    }

    #[test]
    fn hevc_sample_entries_reject_hev1_and_unqualified_children() {
        let original = std::fs::read(path("fixtures", "hevc-pq.mp4")).unwrap();
        video_admission(&file(&original), video_limits()).unwrap();
        for (from, to, expected) in [
            (b"hvc1", b"hev1", "unsupported_codec"),
            (b"hvcC", b"avcC", "invalid_input"),
            (b"fiel", b"fie2", "invalid_input"),
            (b"mdcv", b"dvcC", "invalid_input"),
        ] {
            let mut changed = original.clone();
            let at = tag(&changed, from);
            changed[at..at + 4].copy_from_slice(to);
            assert_eq!(
                code(video_admission(&file(&changed), video_limits()).unwrap_err()),
                expected,
                "{}",
                String::from_utf8_lossy(to)
            );
        }
        let mut interlaced = original.clone();
        let at = tag(&interlaced, b"fiel");
        interlaced[at + 4] = 2;
        assert_eq!(
            code(video_admission(&file(&interlaced), video_limits()).unwrap_err()),
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
