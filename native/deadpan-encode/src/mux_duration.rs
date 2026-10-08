//! Correct one observed FFmpeg 8.0.3 `mdhd` bookkeeping defect on the owned
//! completed file. Packet data, timestamps, sample tables and edits are never
//! changed. The independent finished-file verifier remains mandatory.

use std::fs::{File, Metadata};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};

use deadpan_source::{DecodeControl, DecodeLimits, Mp4PacketReader, Mp4TrackKind};
use serde::{Deserialize, Serialize};

use crate::{Control, EncodeContract, EncodeError, MAX_OUTPUT_BYTES, MAX_VIDEO_FRAMES};

/// A correction made only after complete packet-table inspection proved the
/// authored presentation interval and the pinned muxer's precise overestimate.
/// These numbers alone do not prove output bytes or authorize publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoMediaDurationCorrection {
    pub previous_ticks: u64,
    pub corrected_ticks: u64,
    pub first_cts: u64,
    pub minimum_cts: u64,
}

impl VideoMediaDurationCorrection {
    /// Check arithmetic and the captured clock only. File evidence must come
    /// from the private finalizer and independent output verification.
    pub fn validate(&self, contract: &EncodeContract) -> Result<(), EncodeError> {
        let den = u64::from(contract.frame_rate()[1]);
        let expected = duration(contract)?;
        require(
            contract.policy().b_frames > 0
                && self.corrected_ticks == expected
                && self.previous_ticks > self.corrected_ticks
                && self.first_cts > self.minimum_cts
                && self.first_cts <= den * 64
                && self.first_cts <= den * u64::from(contract.policy().b_frames)
                && self.first_cts.is_multiple_of(den)
                && self.minimum_cts.is_multiple_of(den)
                && self.previous_ticks - self.corrected_ticks == self.first_cts - self.minimum_cts,
            "mux duration correction differs from its exact clock or policy",
        )
    }
}

fn require(condition: bool, reason: &'static str) -> Result<(), EncodeError> {
    if condition {
        Ok(())
    } else {
        Err(EncodeError::Evidence(reason))
    }
}

fn duration(contract: &EncodeContract) -> Result<u64, EncodeError> {
    contract
        .video_frames()
        .checked_mul(u64::from(contract.frame_rate()[1]))
        .ok_or(EncodeError::Evidence("video duration overflow"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Identity {
    fn read(file: &File) -> Result<Self, EncodeError> {
        let metadata = file.metadata()?;
        require(
            metadata.is_file(),
            "mux finalization needs the owned regular file",
        )?;
        require(
            (1..=MAX_OUTPUT_BYTES).contains(&metadata.len()),
            "mux output exceeds its file bound",
        )?;
        Ok(Self::from_metadata(&metadata))
    }

    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }

    fn same_file(self, other: Self) -> bool {
        self.device == other.device && self.inode == other.inode && self.length == other.length
    }
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
}

#[derive(Clone, Copy)]
struct Atom {
    kind: [u8; 4],
    body: Span,
}

#[derive(Clone, Copy)]
struct Header {
    track_id: u32,
    timescale: u32,
    duration: u64,
    offset: u64,
    wide: bool,
}

/// Only locates fixed-width header fields. Complete table grammar, sample
/// extents and packet clocks are checked by the existing source inspector.
struct Headers<'a> {
    file: &'a mut File,
    control: &'a Control<'a>,
    atoms: u32,
    bytes: u32,
}

impl Headers<'_> {
    fn read<const N: usize>(&mut self, at: u64, parent: Span) -> Result<[u8; N], EncodeError> {
        self.control.check()?;
        require(
            at >= parent.start
                && at
                    .checked_add(N as u64)
                    .is_some_and(|end| end <= parent.end),
            "mux header read exceeds its containing box",
        )?;
        self.bytes = self
            .bytes
            .checked_add(N as u32)
            .ok_or(EncodeError::Evidence("mux header read overflow"))?;
        require(
            self.bytes <= 64 * 1024,
            "mux header inspection exceeds its byte bound",
        )?;
        self.file.seek(SeekFrom::Start(at))?;
        let mut value = [0; N];
        bounded_read(self.file, &mut value, self.control)?;
        Ok(value)
    }

    fn atom(&mut self, cursor: &mut u64, parent: Span) -> Result<Atom, EncodeError> {
        self.control.check()?;
        self.atoms += 1;
        require(
            self.atoms <= 1024,
            "mux header inspection exceeds its box bound",
        )?;
        let bytes = self.read::<8>(*cursor, parent)?;
        let narrow = u32::from_be_bytes(bytes[..4].try_into().expect("four bytes"));
        let (size, header) = if narrow == 1 {
            (u64::from_be_bytes(self.read::<8>(*cursor + 8, parent)?), 16)
        } else {
            (u64::from(narrow), 8)
        };
        let end = cursor
            .checked_add(size)
            .ok_or(EncodeError::Evidence("mux box extent overflow"))?;
        require(
            size >= header && end <= parent.end,
            "mux box exceeds its parent or has unsupported open extent",
        )?;
        let atom = Atom {
            kind: bytes[4..].try_into().expect("four bytes"),
            body: Span {
                start: *cursor + header,
                end,
            },
        };
        *cursor = end;
        Ok(atom)
    }

    fn child(&mut self, parent: Span, kind: [u8; 4]) -> Result<Atom, EncodeError> {
        let mut cursor = parent.start;
        let mut found = None;
        while cursor < parent.end {
            let atom = self.atom(&mut cursor, parent)?;
            if atom.kind == kind {
                require(found.is_none(), "duplicate mux header box")?;
                found = Some(atom);
            }
        }
        found.ok_or(EncodeError::Evidence("required mux header box is missing"))
    }

    fn track(&mut self, track: Span) -> Result<(Header, [u8; 4]), EncodeError> {
        let tkhd = self.child(track, *b"tkhd")?.body;
        let version = self.read::<4>(tkhd.start, tkhd)?;
        let id_offset = match version[0] {
            0 if tkhd.len() == 84 => 12,
            1 if tkhd.len() == 96 => 20,
            _ => {
                return Err(EncodeError::Evidence(
                    "unsupported mux track header version or extent",
                ));
            }
        };
        require(
            version[1..] == [0, 0, 3],
            "mux track must be enabled in the movie",
        )?;
        let track_id = u32::from_be_bytes(self.read::<4>(tkhd.start + id_offset, tkhd)?);
        require(track_id != 0, "mux track ID must be positive")?;
        let media = self.child(track, *b"mdia")?.body;
        let handler = self.child(media, *b"hdlr")?.body;
        require(
            self.read::<4>(handler.start, handler)? == [0; 4],
            "unsupported mux handler version",
        )?;
        let kind = self.read::<4>(handler.start + 8, handler)?;
        let mdhd = self.child(media, *b"mdhd")?.body;
        let full = self.read::<4>(mdhd.start, mdhd)?;
        require(full[1..] == [0, 0, 0], "unsupported mux media header flags")?;
        let (scale_offset, duration_offset, wide) = match full[0] {
            0 if mdhd.len() == 24 => (12, 16, false),
            1 if mdhd.len() == 36 => (20, 24, true),
            _ => {
                return Err(EncodeError::Evidence(
                    "unsupported mux media header version or extent",
                ));
            }
        };
        let timescale = u32::from_be_bytes(self.read::<4>(mdhd.start + scale_offset, mdhd)?);
        let offset = mdhd.start + duration_offset;
        let duration = if wide {
            u64::from_be_bytes(self.read::<8>(offset, mdhd)?)
        } else {
            u64::from(u32::from_be_bytes(self.read::<4>(offset, mdhd)?))
        };
        require(
            duration != if wide { u64::MAX } else { u64::from(u32::MAX) },
            "unknown mux media duration",
        )?;
        Ok((
            Header {
                track_id,
                timescale,
                duration,
                offset,
                wide,
            },
            kind,
        ))
    }

    fn video(&mut self, length: u64) -> Result<Header, EncodeError> {
        let whole = Span {
            start: 0,
            end: length,
        };
        let movie = self.child(whole, *b"moov")?.body;
        let mut cursor = movie.start;
        let mut video = None;
        let mut ids = [0_u32; 2];
        let mut count = 0;
        let mut audio = false;
        while cursor < movie.end {
            let atom = self.atom(&mut cursor, movie)?;
            if atom.kind != *b"trak" {
                continue;
            }
            require(count < 2, "mux output has more than two tracks")?;
            let (header, kind) = self.track(atom.body)?;
            require(
                !ids[..count].contains(&header.track_id),
                "mux track IDs are duplicated",
            )?;
            ids[count] = header.track_id;
            count += 1;
            match &kind {
                b"vide" => {
                    require(video.is_none(), "duplicate mux video track")?;
                    video = Some(header);
                }
                b"soun" => {
                    require(!audio, "duplicate mux audio track")?;
                    audio = true;
                }
                _ => return Err(EncodeError::Evidence("unsupported mux track type")),
            }
        }
        require(
            count == 2 && audio,
            "mux output requires one video and one audio track",
        )?;
        video.ok_or(EncodeError::Evidence("mux video track is missing"))
    }
}

fn bounded_read(
    file: &mut File,
    bytes: &mut [u8],
    control: &Control<'_>,
) -> Result<(), EncodeError> {
    let mut consumed = 0;
    let mut interrupted = 0;
    while consumed < bytes.len() {
        control.check()?;
        match file.read(&mut bytes[consumed..]) {
            Ok(0) => return Err(EncodeError::Evidence("mux header is truncated")),
            Ok(count) => consumed += count,
            Err(error) if error.kind() == ErrorKind::Interrupted && interrupted < 8 => {
                interrupted += 1
            }
            Err(error) => return Err(error.into()),
        }
    }
    control.check()
}

fn inspection_control<'a>(control: &'a Control<'a>) -> Result<DecodeControl<'a>, EncodeError> {
    control.check()?;
    Ok(DecodeControl {
        cancelled: control.cancelled,
        timeout: control
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
    })
}

fn inspect<T>(
    result: Result<T, deadpan_source::SourceDecodeError>,
    control: &Control<'_>,
) -> Result<T, EncodeError> {
    control.check()?;
    result.map_err(|error| EncodeError::Native {
        code: "mux_duration_evidence".into(),
        message: error.to_string(),
    })
}

fn prove(
    file: &File,
    header: Header,
    contract: &EncodeContract,
    control: &Control<'_>,
) -> Result<VideoMediaDurationCorrection, EncodeError> {
    let expected_packets = contract
        .video_frames()
        .checked_add(
            contract
                .audio_samples()
                .div_ceil(u64::from(crate::AUDIO_FRAME_SAMPLES)),
        )
        .and_then(|count| count.checked_add(1))
        .ok_or(EncodeError::Evidence("mux packet count overflow"))?;
    let mut packets = inspect(
        Mp4PacketReader::open(
            file.try_clone()?,
            DecodeLimits {
                max_pixels: crate::MAX_PIXELS,
                max_input_bytes: file.metadata()?.len(),
                max_frames: contract.video_frames(),
                max_packets: expected_packets,
                // The independent source/finished-file parser has a 16 MiB
                // packet ceiling even though encoder allocation can be larger.
                max_packet_bytes: crate::MAX_PACKET_BYTES.min(16 * 1024 * 1024),
                ..DecodeLimits::default()
            },
            inspection_control(control)?,
        ),
        control,
    )?;
    let movie = packets.inspection();
    require(
        movie.moov_before_mdat
            && movie.tracks.len() == 2
            && movie.movie_timescale == contract.policy().movie_timescale,
        "mux duration proof requires the captured fast-start movie clock",
    )?;
    let video = movie
        .tracks
        .iter()
        .find(|track| track.kind == Mp4TrackKind::Video)
        .ok_or(EncodeError::Evidence(
            "mux duration proof has no video track",
        ))?;
    let codec_matches = match contract.video_format() {
        crate::VideoFormat::H264Rec709I420 => video.avc.is_some() && video.hevc.is_none(),
        crate::VideoFormat::HevcMain10Rec2100Pq | crate::VideoFormat::HevcMain10Rec2100Hlg => {
            video.hevc.is_some() && video.avc.is_none()
        }
    };
    let expected = duration(contract)?;
    let den = u64::from(contract.frame_rate()[1]);
    let movie_numerator = u128::from(expected) * u128::from(movie.movie_timescale);
    require(
        movie_numerator.is_multiple_of(u128::from(header.timescale)),
        "mux movie duration is not exactly representable",
    )?;
    let movie_duration = movie_numerator / u128::from(header.timescale);
    require(
        codec_matches
            && video.id == header.track_id
            && video.media_timescale == contract.frame_rate()[0]
            && video.media_duration == Some(header.duration)
            && video.sample_dimensions == Some(contract.raster())
            && u64::from(video.sample_count) == contract.video_frames()
            && video.timing_duration == expected
            && video.edits.len() == 1
            && u128::from(video.edits[0].segment_duration) == movie_duration
            && video.edits[0].media_rate_integer == 1
            && video.edits[0].media_rate_fraction == 0
            && video.edits[0].media_time >= 0,
        "mux duration proof differs from the video contract or normal edit",
    )?;
    let track_id = video.id;
    let track_index = video.index;
    let edit = u64::try_from(video.edits[0].media_time)
        .map_err(|_| EncodeError::Evidence("negative mux edit"))?;
    require(
        edit.is_multiple_of(den) && edit <= u64::from(contract.policy().b_frames) * den,
        "mux edit exceeds the captured reordering policy",
    )?;
    let count = usize::try_from(contract.video_frames())
        .map_err(|_| EncodeError::Evidence("mux frame count overflows"))?;
    require(
        contract.video_frames() <= MAX_VIDEO_FRAMES,
        "mux proof exceeds its sample bound",
    )?;
    let mut seen = Vec::new();
    seen.try_reserve_exact(count)
        .map_err(|_| EncodeError::Evidence("allocate bounded mux coverage"))?;
    seen.resize(count, false);
    let mut ordinal = 0_u64;
    let mut first_cts = None;
    let mut minimum_cts = u64::MAX;
    while let Some(packet) = inspect(packets.next_packet(inspection_control(control)?), control)? {
        if packet.track_id != track_id {
            continue;
        }
        let timing = packet
            .presentation
            .ok_or(EncodeError::Evidence("ambiguous mux presentation edit"))?;
        require(
            packet.track_index == track_index
                && u64::from(packet.sample_index) == ordinal
                && ordinal < contract.video_frames()
                && u64::from(packet.duration) == den
                && i128::from(packet.dts) == i128::from(ordinal) * i128::from(den)
                && i128::from(timing.dts) == i128::from(packet.dts) - i128::from(edit)
                && i128::from(timing.pts) == i128::from(packet.pts) - i128::from(edit)
                && timing.pts >= 0
                && timing.pts >= timing.dts
                && u64::try_from(timing.pts).is_ok_and(|pts| pts.is_multiple_of(den)),
            "mux sample clock or duration differs from authored CFR",
        )?;
        let presentation =
            usize::try_from(u64::try_from(timing.pts).expect("checked nonnegative") / den)
                .map_err(|_| EncodeError::Evidence("mux presentation ordinal overflows"))?;
        require(
            presentation < count && !seen[presentation],
            "mux presentation coverage is duplicated or out of range",
        )?;
        seen[presentation] = true;
        let cts = packet
            .pts
            .checked_sub(packet.dts)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(EncodeError::Evidence(
                "negative or overflowing mux composition offset",
            ))?;
        if ordinal == 0 {
            require(
                timing.pts == 0 && cts == edit,
                "mux first picture is not the authored origin",
            )?;
            first_cts = Some(cts);
        }
        // Exactly the predicate in pinned movenc.c:7036, in its original
        // pre-edit packet clock. The sample tables start their DTS at zero.
        if timing.dts <= 0 {
            minimum_cts = minimum_cts.min(cts);
        }
        ordinal += 1;
    }
    require(
        ordinal == contract.video_frames() && seen.into_iter().all(|present| present),
        "mux presentation coverage is incomplete",
    )?;
    let correction = VideoMediaDurationCorrection {
        previous_ticks: header.duration,
        corrected_ticks: expected,
        first_cts: first_cts.ok_or(EncodeError::Evidence("mux proof has no pictures"))?,
        minimum_cts,
    };
    correction.validate(contract)?;
    Ok(correction)
}

/// Called after the native writer closes and before the private descriptor is
/// returned. Successful no-op and corrected paths both restore the cursor.
pub(super) fn finalize(
    file: &mut File,
    contract: &EncodeContract,
    control: &Control<'_>,
) -> Result<Option<VideoMediaDurationCorrection>, EncodeError> {
    control.check()?;
    let before = Identity::read(file)?;
    let header = Headers {
        file,
        control,
        atoms: 0,
        bytes: 0,
    }
    .video(before.length)?;
    require(
        header.timescale == contract.frame_rate()[0],
        "mux video timescale differs from its authored clock",
    )?;
    let expected = duration(contract)?;
    if header.duration == expected {
        require(
            before == Identity::read(file)?,
            "mux output changed during header inspection",
        )?;
        file.seek(SeekFrom::Start(0))?;
        control.check()?;
        return Ok(None);
    }
    let correction = prove(file, header, contract, control)?;
    require(
        before == Identity::read(file)?,
        "mux output changed during packet proof",
    )?;
    control.check()?;
    let wide = correction.corrected_ticks.to_be_bytes();
    let narrow = u32::try_from(correction.corrected_ticks).map(u32::to_be_bytes);
    let bytes = if header.wide {
        &wide[..]
    } else {
        narrow
            .as_ref()
            .map(|value| &value[..])
            .map_err(|_| EncodeError::Evidence("corrected duration exceeds its original field"))?
    };
    file.seek(SeekFrom::Start(header.offset))?;
    let mut written = 0;
    let mut interrupted = 0;
    while written < bytes.len() {
        control.check()?;
        match file.write(&bytes[written..]) {
            Ok(0) => return Err(EncodeError::Evidence("mux duration write made no progress")),
            Ok(count) => written += count,
            Err(error) if error.kind() == ErrorKind::Interrupted && interrupted < 8 => {
                interrupted += 1
            }
            Err(error) => return Err(error.into()),
        }
    }
    control.check()?;
    file.sync_all()?;
    control.check()?;
    require(
        before.same_file(Identity::read(file)?),
        "mux duration write changed the owned file identity or size",
    )?;
    let after = Headers {
        file,
        control,
        atoms: 0,
        bytes: 0,
    }
    .video(before.length)?;
    require(
        after.offset == header.offset
            && after.duration == expected
            && after.track_id == header.track_id,
        "mux duration correction did not retain its exact header",
    )?;
    file.seek(SeekFrom::Start(0))?;
    control.check()?;
    Ok(Some(correction))
}

#[cfg(test)]
#[path = "mux_duration_tests.rs"]
mod tests;
