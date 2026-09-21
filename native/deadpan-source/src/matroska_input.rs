//! Finite, closed Matroska allocation admission before any FFmpeg parsing.
//! Payloads are skipped, but every element and every deferred seek target is checked.

use crate::{DecodeControl, SourceDecodeError, input::InputLimits};
use std::{
    collections::BTreeSet, fs::File, os::unix::fs::FileExt, sync::atomic::Ordering, time::Instant,
};

const HEADER_BYTES: u64 = 16 * 1024 * 1024;
const METADATA_ITEMS: u64 = 1_000_000;
const PACKETS: u64 = 40_000_000;
const STRING_BYTES: u64 = 4096;
const CODEC_BYTES: u64 = 64 * 1024;
const EBML: u32 = 0x1a45dfa3;
const SEGMENT: u32 = 0x18538067;
const SEEK_HEAD: u32 = 0x114d9b74;
const INFO: u32 = 0x1549a966;
const TRACKS: u32 = 0x1654ae6b;
const TAGS: u32 = 0x1254c367;
const CLUSTER: u32 = 0x1f43b675;
const CUES: u32 = 0x1c53bb6b;
const VOID: u32 = 0xec;
const CRC: u32 = 0xbf;
const SIMPLE_BLOCK: u32 = 0xa3;
const BLOCK_GROUP: u32 = 0xa0;
const BLOCK: u32 = 0xa1;

type Result<T> = std::result::Result<T, SourceDecodeError>;

fn error(code: &str, message: &'static str) -> SourceDecodeError {
    SourceDecodeError::Native {
        code: code.into(),
        message: message.into(),
    }
}
fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error("invalid_input", message))
    }
}
fn limit(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error("resource_limit", message))
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
struct Element {
    id: u32,
    start: u64,
    body: Span,
}

struct Reader<'a> {
    file: &'a File,
    control: DecodeControl<'a>,
    started: Instant,
    limits: InputLimits,
    length: u64,
    // Small unaligned windows matter: a full page at each sparse Block would
    // exhaust 16 MiB after only 4096 frames. Cached reads still check cancellation.
    window: [u8; 64],
    window_start: u64,
    window_len: usize,
    read_bytes: u64,
    header_bytes: u64,
    metadata_items: u64,
    packets: u64,
}
impl Reader<'_> {
    fn check(&self) -> Result<()> {
        if self.control.cancelled.load(Ordering::Relaxed) {
            return Err(error("cancelled", "Matroska preflight cancelled"));
        }
        if self.started.elapsed() >= self.control.timeout {
            return Err(error(
                "deadline_exceeded",
                "Matroska preflight exceeded its cooperative deadline",
            ));
        }
        Ok(())
    }
    fn byte(&mut self, at: u64) -> Result<u8> {
        self.check()?;
        require(at < self.length, "truncated Matroska field")?;
        if at < self.window_start || at - self.window_start >= self.window_len as u64 {
            let remaining = HEADER_BYTES
                .min(self.limits.max_io_bytes_per_call)
                .saturating_sub(self.read_bytes);
            limit(remaining > 0, "Matroska admission read budget exceeded")?;
            let amount = (self.length - at).min(64).min(remaining) as usize;
            self.window_start = at;
            self.window_len = 0;
            while self.window_len < amount {
                self.check()?;
                let read = match self.file.read_at(
                    &mut self.window[self.window_len..amount],
                    at + self.window_len as u64,
                ) {
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                require(read != 0, "truncated Matroska input")?;
                self.read_bytes += read as u64;
                self.window_len += read;
            }
        }
        Ok(self.window[(at - self.window_start) as usize])
    }
    fn vint(&mut self, at: u64, end: u64, id: bool) -> Result<(u64, u64)> {
        require(at < end, "truncated EBML integer")?;
        let first = self.byte(at)?;
        let length = u64::from(first.leading_zeros()) + 1;
        require(
            length <= if id { 4 } else { 8 } && length <= end - at,
            "invalid EBML integer length",
        )?;
        let mut value = u64::from(first);
        if !id {
            value &= (1 << (8 - length)) - 1;
        }
        for offset in 1..length {
            value = (value << 8) | u64::from(self.byte(at + offset)?);
        }
        if !id {
            require(
                value != (1_u64 << (7 * length)) - 1,
                "unknown-size EBML elements are not admitted",
            )?;
        }
        Ok((value, length))
    }
    fn charge(&mut self, bytes: u64) -> Result<()> {
        self.header_bytes = self
            .header_bytes
            .checked_add(bytes)
            .ok_or_else(|| error("resource_limit", "Matroska header size overflow"))?;
        limit(
            self.header_bytes <= HEADER_BYTES,
            "Matroska metadata exceeds 16 MiB",
        )
    }
    fn element(&mut self, cursor: &mut u64, end: u64, depth: u32, charge: bool) -> Result<Element> {
        self.check()?;
        limit(depth <= 16, "Matroska nesting exceeds 16 levels")?;
        require(*cursor <= end, "invalid Matroska element position")?;
        let start = *cursor;
        let (id, id_len) = self.vint(start, end, true)?;
        let (size, size_len) = self.vint(start + id_len, end, false)?;
        let body_start = start + id_len + size_len;
        require(
            size <= end - body_start,
            "Matroska element exceeds its parent",
        )?;
        let id = u32::try_from(id).expect("at most four EBML ID bytes");
        if charge {
            self.charge(id_len + size_len)?;
            if !matches!(id, SIMPLE_BLOCK | BLOCK | BLOCK_GROUP) {
                self.metadata_items += 1;
                limit(
                    self.metadata_items <= METADATA_ITEMS,
                    "Matroska metadata exceeds one million elements",
                )?;
            }
        }
        *cursor = body_start + size;
        Ok(Element {
            id,
            start,
            body: Span {
                start: body_start,
                end: *cursor,
            },
        })
    }
    fn uint(&mut self, span: Span) -> Result<u64> {
        require(
            (1..=8).contains(&span.len()),
            "invalid Matroska scalar size",
        )?;
        self.charge(span.len())?;
        let mut value = 0;
        for at in span.start..span.end {
            value = (value << 8) | u64::from(self.byte(at)?);
        }
        Ok(value)
    }
    fn float(&mut self, span: Span) -> Result<()> {
        require(matches!(span.len(), 4 | 8), "invalid Matroska float size")?;
        let bits = self.uint(span)?;
        let finite = if span.len() == 4 {
            f32::from_bits(u32::try_from(bits).expect("four-byte scalar")).is_finite()
        } else {
            f64::from_bits(bits).is_finite()
        };
        require(finite, "nonfinite Matroska float")
    }
    fn fixed(&mut self, span: Span, size: u64) -> Result<()> {
        require(span.len() == size, "invalid fixed Matroska field size")?;
        self.charge(size)
    }
    fn blob(&mut self, span: Span, maximum: u64) -> Result<Vec<u8>> {
        limit(
            span.len() <= maximum,
            "Matroska field exceeds admission size",
        )?;
        self.charge(span.len())?;
        let mut bytes = Vec::with_capacity(usize::try_from(span.len()).expect("bounded field"));
        for at in span.start..span.end {
            bytes.push(self.byte(at)?);
        }
        Ok(bytes)
    }
    fn string(&mut self, span: Span) -> Result<()> {
        self.blob(span, STRING_BYTES).map(|_| ())
    }
    fn padding(&mut self, element: Element, seen: &mut BTreeSet<u32>) -> Result<bool> {
        match element.id {
            VOID => self.charge(element.body.len())?,
            CRC => {
                singleton(seen, CRC)?;
                self.fixed(element.body, 4)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
fn singleton(seen: &mut BTreeSet<u32>, id: u32) -> Result<()> {
    require(seen.insert(id), "duplicate singleton Matroska field")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cue {
    cluster: u64,
    relative: Option<u64>,
    block: Option<u64>,
}
struct Admission<'a> {
    reader: Reader<'a>,
    track: Option<u64>,
    simple_tags: u32,
    seeks: BTreeSet<(u64, u32)>,
    cues: BTreeSet<Cue>,
}
impl Admission<'_> {
    fn ebml(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 1, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                0x4286 | 0x42f7 => {
                    require(self.reader.uint(e.body)? == 1, "unsupported EBML version")?
                }
                0x42f2 => require(
                    (1..=4).contains(&self.reader.uint(e.body)?),
                    "unsupported EBML ID length",
                )?,
                0x42f3 => require(
                    (1..=8).contains(&self.reader.uint(e.body)?),
                    "unsupported EBML size length",
                )?,
                0x4282 => require(
                    self.reader.blob(e.body, STRING_BYTES)? == b"matroska",
                    "only Matroska DocType is admitted",
                )?,
                0x4287 => require(
                    (1..=4).contains(&self.reader.uint(e.body)?),
                    "unsupported Matroska document version",
                )?,
                0x4285 => require(
                    (1..=2).contains(&self.reader.uint(e.body)?),
                    "unsupported Matroska read version",
                )?,
                _ => return Err(error("invalid_input", "unqualified EBML header field")),
            }
        }
        require(seen.contains(&0x4282), "Matroska DocType is required")
    }
    fn info(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 2, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                0x2ad7b1 => require(
                    self.reader.uint(e.body)? > 0,
                    "zero Matroska timestamp scale",
                )?,
                0x4489 => self.reader.float(e.body)?,
                0x7ba9 | 0x4d80 | 0x5741 => self.reader.string(e.body)?,
                0x73a4 => self.reader.fixed(e.body, 16)?,
                0x4461 => self.reader.fixed(e.body, 8)?,
                _ => return Err(error("invalid_input", "unqualified Matroska Info field")),
            }
        }
        Ok(())
    }
    fn video(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        let (mut width, mut height) = (None, None);
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 4, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                0xb0 => width = Some(self.reader.uint(e.body)?),
                0xba => height = Some(self.reader.uint(e.body)?),
                0x55b0 => self.colour(e.body, false)?,
                0x2383e3 => self.reader.float(e.body)?,
                0x2eb524 => self.reader.fixed(e.body, 4)?,
                0x9a | 0x9d | 0x54b0 | 0x54ba | 0x54b2 | 0x54b3 | 0x53b8 | 0x53c0 | 0x54aa
                | 0x54bb | 0x54cc | 0x54dd => {
                    self.reader.uint(e.body)?;
                }
                _ => return Err(error("invalid_input", "unqualified Matroska Video field")),
            }
        }
        let (Some(width), Some(height)) = (width, height) else {
            return Err(error(
                "invalid_input",
                "Matroska video dimensions are required",
            ));
        };
        limit(
            width > 0
                && height > 0
                && width <= u64::from(self.reader.limits.max_dimension)
                && height <= u64::from(self.reader.limits.max_dimension)
                && width
                    .checked_mul(height)
                    .is_some_and(|n| n <= self.reader.limits.max_pixels),
            "Matroska video dimensions exceed configured limits",
        )
    }
    fn colour(&mut self, span: Span, mastering: bool) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e =
                self.reader
                    .element(&mut cursor, span.end, if mastering { 6 } else { 5 }, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                0x55d1..=0x55da if mastering => self.reader.float(e.body)?,
                0x55b1..=0x55bd if !mastering => {
                    self.reader.uint(e.body)?;
                }
                0x55d0 if !mastering => self.colour(e.body, true)?,
                _ => return Err(error("invalid_input", "unqualified Matroska Colour field")),
            }
        }
        Ok(())
    }
    fn tracks(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        let mut tracks = 0;
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 2, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            require(e.id == 0xae, "unqualified Matroska Tracks field")?;
            tracks += 1;
            limit(tracks <= 33, "Matroska track count exceeds hard limit")?;
            require(tracks == 1, "only one Matroska video track is admitted")?;
            self.track(e.body)?;
        }
        require(tracks == 1, "one Matroska video track is required")
    }
    fn track(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 3, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                0xd7 => {
                    let number = self.reader.uint(e.body)?;
                    require(
                        number > 0 && number < (1_u64 << 56) - 1,
                        "invalid Matroska TrackNumber",
                    )?;
                    self.track = Some(number);
                }
                0x83 => require(
                    self.reader.uint(e.body)? == 1,
                    "only Matroska video tracks are admitted",
                )?,
                0x86 => require(
                    self.reader.blob(e.body, STRING_BYTES)? == b"V_FFV1",
                    "only FFV1 Matroska video is admitted",
                )?,
                0x63a2 => {
                    let bytes = self.reader.blob(e.body, CODEC_BYTES)?;
                    crate::video_codec::validate_ffv1(
                        &bytes,
                        self.reader.limits.max_pixels,
                        self.reader.limits.max_dimension,
                    )?;
                    self.reader.check()?;
                }
                0xe0 => self.video(e.body)?,
                0x9c | 0x55ee => require(
                    self.reader.uint(e.body)? == 0,
                    "Matroska lacing and additional payloads are not admitted",
                )?,
                0x536e | 0x22b59c | 0x258688 => self.reader.string(e.body)?,
                0x73c5 | 0x23e383 | 0xb9 | 0x88 | 0x55aa | 0x55ab | 0x55ac | 0x55ad | 0x55ae
                | 0x55af | 0x56aa | 0x56bb => {
                    self.reader.uint(e.body)?;
                }
                _ => {
                    return Err(error(
                        "invalid_input",
                        "unqualified Matroska TrackEntry field",
                    ));
                }
            }
        }
        require(
            [0xd7, 0x83, 0x86, 0x63a2, 0xe0]
                .iter()
                .all(|id| seen.contains(id)),
            "Matroska video track lacks required fields",
        )
    }
    fn tags(&mut self, span: Span, kind: u32, depth: u32) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, depth, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            let repeated = matches!((kind, e.id), (TAGS, 0x7373) | (0x7373, 0x67c8));
            if !repeated {
                singleton(&mut seen, e.id)?;
            }
            match (kind, e.id) {
                (TAGS, 0x7373) | (0x7373, 0x63c0 | 0x67c8) => {
                    if e.id == 0x67c8 {
                        // FFmpeg recursively visits a default language tag's
                        // children twice. Admit only flat tags and bound their
                        // aggregate dictionary work, not just physical bytes.
                        self.simple_tags += 1;
                        limit(
                            self.simple_tags <= 1024,
                            "Matroska metadata exceeds 1024 SimpleTags",
                        )?;
                    }
                    self.tags(e.body, e.id, depth + 1)?
                }
                (0x63c0, 0x63ca) | (0x67c8, 0x45a3 | 0x4487 | 0x447a) => {
                    self.reader.string(e.body)?
                }
                (0x63c0, 0x68ca | 0x63c5) | (0x67c8, 0x4484) => {
                    self.reader.uint(e.body)?;
                }
                _ => return Err(error("invalid_input", "unqualified Matroska tag field")),
            }
        }
        if kind == 0x67c8 {
            require(seen.contains(&0x45a3), "Matroska SimpleTag requires a name")?;
        }
        Ok(())
    }
    fn seek_head(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 2, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            require(e.id == 0x4dbb, "unqualified Matroska SeekHead field")?;
            let mut at = e.body.start;
            let mut fields = BTreeSet::new();
            let (mut id, mut position) = (None, None);
            while at < e.body.end {
                let field = self.reader.element(&mut at, e.body.end, 3, true)?;
                if self.reader.padding(field, &mut fields)? {
                    continue;
                }
                singleton(&mut fields, field.id)?;
                match field.id {
                    0x53ab => {
                        require(field.body.len() <= 4, "invalid Matroska SeekID size")?;
                        let value = self.reader.uint(field.body)?;
                        let value = u32::try_from(value).expect("four-byte SeekID");
                        require(
                            matches!(value, INFO | TRACKS | TAGS | CUES | CLUSTER),
                            "unqualified or recursive Matroska Seek target",
                        )?;
                        id = Some(value);
                    }
                    0x53ac => position = Some(self.reader.uint(field.body)?),
                    _ => return Err(error("invalid_input", "unqualified Matroska Seek field")),
                }
            }
            let (Some(id), Some(position)) = (id, position) else {
                return Err(error("invalid_input", "incomplete Matroska Seek entry"));
            };
            require(
                self.seeks.insert((position, id)),
                "duplicate Matroska Seek entry",
            )?;
        }
        Ok(())
    }
    fn cues(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 2, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            require(e.id == 0xbb, "unqualified Matroska Cues field")?;
            let mut at = e.body.start;
            let mut fields = BTreeSet::new();
            while at < e.body.end {
                let field = self.reader.element(&mut at, e.body.end, 3, true)?;
                if self.reader.padding(field, &mut fields)? {
                    continue;
                }
                // Exactly one admitted track means only one position per point.
                singleton(&mut fields, field.id)?;
                match field.id {
                    0xb3 => {
                        self.reader.uint(field.body)?;
                    }
                    0xb7 => self.cue_position(field.body)?,
                    _ => {
                        return Err(error(
                            "invalid_input",
                            "unqualified Matroska CuePoint field",
                        ));
                    }
                }
            }
            require(
                fields.contains(&0xb3) && fields.contains(&0xb7),
                "incomplete Matroska CuePoint",
            )?;
        }
        Ok(())
    }
    fn cue_position(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        let (mut track, mut cluster, mut relative, mut block) = (None, None, None, None);
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 4, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            let value = match e.id {
                0xf7 | 0xf1 | 0xf0 | 0xb2 | 0x5378 => self.reader.uint(e.body)?,
                _ => {
                    return Err(error(
                        "invalid_input",
                        "unqualified Matroska CueTrackPositions field",
                    ));
                }
            };
            match e.id {
                0xf7 => track = Some(value),
                0xf1 => cluster = Some(value),
                0xf0 => relative = Some(value),
                0x5378 => {
                    require(value > 0, "zero Matroska CueBlockNumber")?;
                    block = Some(value);
                }
                _ => {}
            }
        }
        require(
            track.is_some() && track == self.track,
            "Matroska cue references an unqualified track",
        )?;
        let cluster =
            cluster.ok_or_else(|| error("invalid_input", "Matroska cue lacks cluster position"))?;
        self.cues.insert(Cue {
            cluster,
            relative,
            block,
        });
        Ok(())
    }
    fn block(&mut self, e: Element) -> Result<()> {
        self.reader.packets += 1;
        limit(
            self.reader.packets <= self.reader.limits.max_packets.min(PACKETS),
            "Matroska block count exceeds configured limit",
        )?;
        // FFmpeg allocates the entire EBML binary, including the block header.
        limit(
            e.body.len() <= self.reader.limits.max_packet_bytes,
            "Matroska Block exceeds configured packet bytes",
        )?;
        let (track, track_bytes) = self.reader.vint(e.body.start, e.body.end, false)?;
        require(
            Some(track) == self.track,
            "Matroska Block references an unqualified track",
        )?;
        require(
            e.body.len() > track_bytes + 3,
            "truncated or empty Matroska Block",
        )?;
        let flags = self.reader.byte(e.body.start + track_bytes + 2)?;
        require(flags & 0x06 == 0, "Matroska laced blocks are not admitted")?;
        let allowed = if e.id == SIMPLE_BLOCK { 0x89 } else { 0x08 };
        require(flags & !allowed == 0, "unsupported Matroska Block flags")?;
        self.reader.charge(track_bytes + 3)
    }
    fn block_group(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 3, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            singleton(&mut seen, e.id)?;
            match e.id {
                BLOCK => self.block(e)?,
                0x9b | 0xfb | 0x75a2 => {
                    self.reader.uint(e.body)?;
                }
                _ => {
                    return Err(error(
                        "invalid_input",
                        "unqualified Matroska BlockGroup field",
                    ));
                }
            }
        }
        require(
            seen.contains(&BLOCK),
            "Matroska BlockGroup requires one Block",
        )
    }
    fn cluster(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 2, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            match e.id {
                SIMPLE_BLOCK | BLOCK_GROUP => {
                    require(
                        seen.contains(&0xe7),
                        "Matroska cluster timestamp must precede blocks",
                    )?;
                    if e.id == SIMPLE_BLOCK {
                        self.block(e)?;
                    } else {
                        self.block_group(e.body)?;
                    }
                }
                0xe7 | 0xa7 | 0xab => {
                    singleton(&mut seen, e.id)?;
                    self.reader.uint(e.body)?;
                }
                _ => return Err(error("invalid_input", "unqualified Matroska Cluster field")),
            }
        }
        require(seen.contains(&0xe7), "Matroska Cluster requires Timestamp")
    }
    fn segment(&mut self, span: Span) -> Result<()> {
        let mut cursor = span.start;
        let mut seen = BTreeSet::new();
        while cursor < span.end {
            let e = self.reader.element(&mut cursor, span.end, 1, true)?;
            if self.reader.padding(e, &mut seen)? {
                continue;
            }
            if e.id != CLUSTER {
                singleton(&mut seen, e.id)?;
            }
            match e.id {
                SEEK_HEAD => self.seek_head(e.body)?,
                INFO => self.info(e.body)?,
                TRACKS => self.tracks(e.body)?,
                TAGS => self.tags(e.body, TAGS, 2)?,
                CUES => self.cues(e.body)?,
                CLUSTER => {
                    require(
                        seen.contains(&INFO) && seen.contains(&TRACKS),
                        "Matroska Info and Tracks must precede Clusters",
                    )?;
                    self.cluster(e.body)?;
                }
                _ => return Err(error("invalid_input", "unqualified Matroska Segment field")),
            }
        }
        require(
            seen.contains(&INFO) && seen.contains(&TRACKS) && self.reader.packets > 0,
            "Matroska requires Info, Tracks, and video Blocks",
        )?;
        self.targets(span)
    }
    fn targets(&mut self, span: Span) -> Result<()> {
        // Rewalk real structure after collecting late/deferred Cues. Never parse
        // target bytes independently: compressed payload can contain valid magic.
        let mut cursor = span.start;
        while cursor < span.end && (!self.seeks.is_empty() || !self.cues.is_empty()) {
            let e = self.reader.element(&mut cursor, span.end, 1, false)?;
            let offset = e.start - span.start;
            if self.seeks.first().is_some_and(|&(pos, _)| pos < offset)
                || self.cues.first().is_some_and(|cue| cue.cluster < offset)
            {
                return Err(error(
                    "invalid_input",
                    "Matroska target is not a structural boundary",
                ));
            }
            if self.seeks.first().is_some_and(|&(pos, _)| pos == offset) {
                require(
                    self.seeks.remove(&(offset, e.id)),
                    "Matroska SeekID disagrees with target boundary",
                )?;
                require(
                    !self.seeks.first().is_some_and(|&(pos, _)| pos == offset),
                    "conflicting Matroska Seek target IDs",
                )?;
            }
            let mut wanted = BTreeSet::new();
            while self.cues.first().is_some_and(|cue| cue.cluster == offset) {
                let cue = self.cues.pop_first().expect("checked cue");
                require(e.id == CLUSTER, "Matroska cue target is not a Cluster")?;
                wanted.insert((cue.relative, cue.block));
            }
            if !wanted.is_empty() {
                wanted.remove(&(None, None));
                let mut at = e.body.start;
                let mut block = 0;
                while at < e.body.end && !wanted.is_empty() {
                    let child = self.reader.element(&mut at, e.body.end, 2, false)?;
                    if matches!(child.id, SIMPLE_BLOCK | BLOCK_GROUP) {
                        block += 1;
                        let relative = child.start - e.body.start;
                        wanted.remove(&(Some(relative), None));
                        wanted.remove(&(None, Some(block)));
                        wanted.remove(&(Some(relative), Some(block)));
                    }
                }
                require(
                    wanted.is_empty(),
                    "Matroska cue position is not its declared block boundary",
                )?;
            }
        }
        require(
            self.seeks.is_empty() && self.cues.is_empty(),
            "Matroska target lies outside admitted structure",
        )
    }
}

pub(crate) fn validate(
    file: &File,
    limits: InputLimits,
    control: DecodeControl<'_>,
) -> Result<u64> {
    let started = Instant::now();
    let metadata = file.metadata()?;
    require(
        metadata.is_file(),
        "Matroska preflight requires a regular file",
    )?;
    let length = metadata.len();
    limit(
        length > 0 && length <= limits.max_input_bytes,
        "Matroska input size exceeds configured limit",
    )?;
    let mut admission = Admission {
        reader: Reader {
            file,
            control,
            started,
            limits,
            length,
            window: [0; 64],
            window_start: 0,
            window_len: 0,
            read_bytes: 0,
            header_bytes: 0,
            metadata_items: 0,
            packets: 0,
        },
        track: None,
        simple_tags: 0,
        seeks: BTreeSet::new(),
        cues: BTreeSet::new(),
    };
    let mut cursor = 0;
    let header = admission.reader.element(&mut cursor, length, 0, true)?;
    require(
        header.id == EBML,
        "Matroska requires one leading EBML header",
    )?;
    admission.ebml(header.body)?;
    let segment = admission.reader.element(&mut cursor, length, 0, true)?;
    require(
        segment.id == SEGMENT && cursor == length,
        "Matroska requires one finite Segment and no trailing data",
    )?;
    admission.segment(segment.body)?;
    admission.reader.check()?;
    Ok(admission.reader.read_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, sync::atomic::AtomicBool, time::Duration};

    static CANCELLED: AtomicBool = AtomicBool::new(false);

    fn limits() -> InputLimits {
        InputLimits {
            max_input_bytes: 1024 * 1024 * 1024,
            max_packets: 1_000_000,
            max_io_bytes_per_call: 64 * 1024 * 1024,
            max_packet_bytes: 16 * 1024 * 1024,
            max_decoded_samples: 1_000_000,
            max_channels: 32,
            max_sample_rate: 192_000,
            max_pixels: 16_777_216,
            max_dimension: 8192,
        }
    }
    fn control() -> DecodeControl<'static> {
        DecodeControl {
            timeout: Duration::from_secs(30),
            cancelled: &CANCELLED,
        }
    }
    fn size(value: u64) -> Vec<u8> {
        let length = (1..=8)
            .find(|length| value < (1_u64 << (7 * length)) - 1)
            .unwrap();
        let encoded = value | (1_u64 << (7 * length));
        encoded.to_be_bytes()[8 - length..].to_vec()
    }
    fn header(id: u32, length: u64) -> Vec<u8> {
        let bytes = id.to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap();
        [bytes[first..].to_vec(), size(length)].concat()
    }
    fn element(id: u32, body: &[u8]) -> Vec<u8> {
        [header(id, body.len() as u64), body.to_vec()].concat()
    }
    fn uint(id: u32, value: u64) -> Vec<u8> {
        let bytes = value.to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap_or(7);
        element(id, &bytes[first..])
    }
    fn ebml() -> Vec<u8> {
        element(
            EBML,
            &[
                uint(0x4286, 1),
                uint(0x42f7, 1),
                uint(0x42f2, 4),
                uint(0x42f3, 8),
                element(0x4282, b"matroska"),
                uint(0x4287, 4),
                uint(0x4285, 2),
            ]
            .concat(),
        )
    }
    fn private() -> &'static [u8] {
        // The committed FFmpeg FFV1-v3 fixture has a 42-byte CodecPrivate.
        &include_bytes!("../tests/fixtures/limited709.mkv")[410..452]
    }
    fn track_body() -> Vec<u8> {
        [
            uint(0xd7, 1),
            uint(0x83, 1),
            element(0x86, b"V_FFV1"),
            uint(0x9c, 0),
            element(0x63a2, private()),
            element(0xe0, &[uint(0xb0, 4), uint(0xba, 2)].concat()),
        ]
        .concat()
    }
    fn prefix_with_track(track: &[u8]) -> Vec<u8> {
        [
            element(INFO, &uint(0x2ad7b1, 1_000_000)),
            element(TRACKS, &element(0xae, track)),
        ]
        .concat()
    }
    fn prefix() -> Vec<u8> {
        prefix_with_track(&track_body())
    }
    fn block() -> Vec<u8> {
        element(SIMPLE_BLOCK, &[0x81, 0, 0, 0x80, 1])
    }
    fn cluster() -> Vec<u8> {
        element(CLUSTER, &[uint(0xe7, 0), block()].concat())
    }
    fn document(body: &[u8]) -> Vec<u8> {
        [ebml(), element(SEGMENT, body)].concat()
    }
    fn file(bytes: &[u8]) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file
    }
    fn check(bytes: &[u8]) -> Result<u64> {
        validate(&file(bytes), limits(), control())
    }
    fn rejection(bytes: &[u8], expected: &str) {
        let err = check(bytes).unwrap_err().to_string();
        assert!(err.contains(expected), "{err}, expected {expected}");
    }
    fn cue(cluster: u64, relative: Option<u64>, block: Option<u64>) -> Vec<u8> {
        let mut position = [uint(0xf7, 1), uint(0xf1, cluster)].concat();
        if let Some(relative) = relative {
            position.extend(uint(0xf0, relative));
        }
        if let Some(block) = block {
            position.extend(uint(0x5378, block));
        }
        element(
            CUES,
            &element(0xbb, &[uint(0xb3, 0), element(0xb7, &position)].concat()),
        )
    }
    fn seek(id: u32, position: u64) -> Vec<u8> {
        element(
            SEEK_HEAD,
            &element(
                0x4dbb,
                &[element(0x53ab, &id.to_be_bytes()), uint(0x53ac, position)].concat(),
            ),
        )
    }

    #[test]
    fn all_committed_matroska_fixtures_pass_allocation_admission() {
        for name in [
            "limited709.mkv",
            "full709.mkv",
            "anamorphic.mkv",
            "interlaced.mkv",
            "hdr-pq.mkv",
            "sdr-with-stream-hdr.mkv",
            "ten-bit.mkv",
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name);
            let file = File::open(path).unwrap();
            let charged =
                validate(&file, limits(), control()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(charged > 0 && charged < 4096, "{name}: {charged}");
        }
    }

    #[test]
    fn finite_complete_document_and_block_group_are_admitted() {
        check(&document(&[prefix(), cluster()].concat())).unwrap();
        let group = element(
            BLOCK_GROUP,
            &[
                element(BLOCK, &[0x81, 0, 0, 0, 1]),
                uint(0x9b, 1),
                uint(0xfb, 0),
                uint(0x75a2, 0),
            ]
            .concat(),
        );
        check(&document(
            &[prefix(), element(CLUSTER, &[uint(0xe7, 0), group].concat())].concat(),
        ))
        .unwrap();
    }

    #[test]
    fn deferred_cues_and_seek_entries_require_actual_boundaries() {
        let prefix = prefix();
        let offset = prefix.len() as u64;
        let relative = uint(0xe7, 0).len() as u64;
        check(&document(
            &[
                prefix.clone(),
                cluster(),
                cue(offset, Some(relative), Some(1)),
                seek(CLUSTER, offset),
            ]
            .concat(),
        ))
        .unwrap();
        rejection(
            &document(&[prefix.clone(), cluster(), cue(offset + 1, None, None)].concat()),
            "structural boundary",
        );
        rejection(
            &document(
                &[
                    prefix.clone(),
                    cluster(),
                    cue(offset, Some(relative + 1), None),
                ]
                .concat(),
            ),
            "block boundary",
        );
        rejection(
            &document(
                &[
                    prefix.clone(),
                    cluster(),
                    cue(offset, Some(relative), Some(2)),
                ]
                .concat(),
            ),
            "block boundary",
        );
        rejection(
            &document(&[prefix.clone(), cluster(), cue(u64::MAX, None, None)].concat()),
            "outside admitted structure",
        );
        rejection(
            &document(&[prefix.clone(), cluster(), seek(INFO, offset)].concat()),
            "SeekID disagrees",
        );
        rejection(
            &document(&[prefix.clone(), cluster(), seek(SEEK_HEAD, 0)].concat()),
            "recursive",
        );

        // A fully parseable Cluster hidden in a Block must not satisfy a cue or seek.
        let payload = [vec![0x81, 0, 0, 0x80], cluster()].concat();
        let body = [uint(0xe7, 0), element(SIMPLE_BLOCK, &payload)].concat();
        let fake = offset
            + header(CLUSTER, body.len() as u64).len() as u64
            + relative
            + header(SIMPLE_BLOCK, payload.len() as u64).len() as u64
            + 4;
        for target in [cue(fake, None, None), seek(CLUSTER, fake)] {
            rejection(
                &document(&[prefix.clone(), element(CLUSTER, &body), target].concat()),
                "structural boundary",
            );
        }
    }

    #[test]
    fn duplicate_unknown_and_unqualified_metadata_are_rejected_even_after_frames() {
        rejection(
            &document(&[prefix(), cluster(), element(INFO, &[])].concat()),
            "duplicate singleton",
        );
        rejection(
            &document(&[prefix(), cluster(), element(0x1941a469, &[])].concat()),
            "unqualified",
        );
        rejection(
            &document(&[prefix(), cluster(), element(0x1043a770, &[])].concat()),
            "unqualified",
        );
        rejection(
            &document(&[prefix(), cluster(), element(0x12345678, &[])].concat()),
            "unqualified",
        );
        for unsupported in [0x6d80, 0xe1, 0xe2, 0x41e4] {
            let track = [track_body(), element(unsupported, &[])].concat();
            rejection(
                &document(&[prefix_with_track(&track), cluster()].concat()),
                "unqualified",
            );
        }
        let track = [track_body(), uint(0xd7, 2)].concat();
        rejection(
            &document(&[prefix_with_track(&track), cluster()].concat()),
            "duplicate singleton",
        );
        let tracks = element(
            TRACKS,
            &[element(0xae, &track_body()), element(0xae, &track_body())].concat(),
        );
        rejection(
            &document(&[element(INFO, &[]), tracks, cluster()].concat()),
            "only one",
        );
    }

    #[test]
    fn unknown_sizes_truncation_and_overflowing_positions_are_rejected() {
        let bytes = document(&[prefix(), cluster()].concat());
        for end in [0, 1, 4, 6, bytes.len() - 1] {
            assert!(check(&bytes[..end]).is_err());
        }
        rejection(
            &[ebml(), SEGMENT.to_be_bytes().to_vec(), vec![0xff]].concat(),
            "unknown-size",
        );
        let cluster = [CLUSTER.to_be_bytes().to_vec(), vec![0xff]].concat();
        rejection(&document(&[prefix(), cluster].concat()), "unknown-size");
        rejection(
            &[ebml(), header(SEGMENT, (1_u64 << 56) - 2)].concat(),
            "exceeds its parent",
        );
        rejection(&[bytes, element(VOID, &[])].concat(), "trailing data");
    }

    #[test]
    fn large_fields_and_laced_or_extra_payloads_fail_before_payload_reads() {
        let mut track = track_body();
        // Appended fields are independently bounded, including ignored metadata.
        track.extend(element(0x536e, &vec![0; STRING_BYTES as usize + 1]));
        rejection(
            &document(&[prefix_with_track(&track), cluster()].concat()),
            "admission size",
        );
        let late_tag = element(
            TAGS,
            &element(
                0x7373,
                &element(
                    0x67c8,
                    &[
                        element(0x45a3, b"LATE"),
                        element(0x4487, &vec![0; STRING_BYTES as usize + 1]),
                    ]
                    .concat(),
                ),
            ),
        );
        rejection(
            &document(&[prefix(), cluster(), late_tag].concat()),
            "admission size",
        );
        let private = element(0x63a2, &vec![0; CODEC_BYTES as usize + 1]);
        let track = [
            uint(0xd7, 1),
            uint(0x83, 1),
            element(0x86, b"V_FFV1"),
            private,
        ]
        .concat();
        rejection(
            &document(&[prefix_with_track(&track), cluster()].concat()),
            "admission size",
        );
        for flags in [0x82, 0x84, 0x86] {
            let block = element(SIMPLE_BLOCK, &[0x81, 0, 0, flags, 1]);
            rejection(
                &document(&[prefix(), element(CLUSTER, &[uint(0xe7, 0), block].concat())].concat()),
                "laced",
            );
        }
        for payload in [element(0x75a1, &[]), element(0xa4, &[])] {
            let group = element(
                BLOCK_GROUP,
                &[element(BLOCK, &[0x81, 0, 0, 0, 1]), payload].concat(),
            );
            rejection(
                &document(&[prefix(), element(CLUSTER, &[uint(0xe7, 0), group].concat())].concat()),
                "unqualified",
            );
        }
    }

    #[test]
    fn metadata_conversion_cannot_expand_nested_language_tags_or_unbounded_flat_tags() {
        let fields = [
            element(0x45a3, b"name"),
            element(0x4487, b"value"),
            element(0x447a, b"eng"),
            uint(0x4484, 1),
        ]
        .concat();
        let flat = element(0x67c8, &fields);
        assert!(
            check(&document(
                &[prefix(), cluster(), element(TAGS, &element(0x7373, &flat))].concat()
            ))
            .is_ok()
        );
        let mut nested = flat.clone();
        for _ in 0..10 {
            nested = element(0x67c8, &[fields.clone(), nested].concat());
        }
        rejection(
            &document(
                &[
                    prefix(),
                    cluster(),
                    element(TAGS, &element(0x7373, &nested)),
                ]
                .concat(),
            ),
            "unqualified Matroska tag field",
        );
        rejection(
            &document(
                &[
                    prefix(),
                    cluster(),
                    element(TAGS, &element(0x7373, &flat.repeat(1025))),
                ]
                .concat(),
            ),
            "1024 SimpleTags",
        );
    }

    #[test]
    fn nested_tags_and_aggregate_metadata_have_hard_limits() {
        let mut tag = element(0x45a3, b"name");
        for _ in 0..20 {
            tag = element(0x67c8, &tag);
        }
        rejection(
            &document(&[prefix(), cluster(), element(TAGS, &element(0x7373, &tag))].concat()),
            "unqualified Matroska tag field",
        );
        let tags = element(TAGS, &element(0x7373, &[]).repeat(METADATA_ITEMS as usize));
        rejection(
            &document(&[prefix(), cluster(), tags].concat()),
            "one million",
        );
        let huge_void = [
            header(VOID, HEADER_BYTES + 1),
            vec![0; HEADER_BYTES as usize + 1],
        ]
        .concat();
        rejection(
            &document(&[prefix(), cluster(), huge_void].concat()),
            "metadata exceeds",
        );
    }

    #[test]
    fn configured_dimensions_packets_io_and_cancellation_are_enforced() {
        let bytes = document(&[prefix(), cluster()].concat());
        let file = file(&bytes);
        for changed in [
            InputLimits {
                max_pixels: 7,
                ..limits()
            },
            InputLimits {
                max_dimension: 3,
                ..limits()
            },
            InputLimits {
                max_packet_bytes: 4,
                ..limits()
            },
            InputLimits {
                max_packets: 0,
                ..limits()
            },
            InputLimits {
                max_io_bytes_per_call: 32,
                ..limits()
            },
            InputLimits {
                max_input_bytes: 32,
                ..limits()
            },
        ] {
            let err = validate(&file, changed, control()).unwrap_err().to_string();
            assert!(err.contains("resource_limit"), "{err}");
        }
        let cancelled = AtomicBool::new(true);
        assert!(
            validate(
                &file,
                limits(),
                DecodeControl {
                    cancelled: &cancelled,
                    ..control()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("cancelled")
        );
        assert!(
            validate(
                &file,
                limits(),
                DecodeControl {
                    timeout: Duration::ZERO,
                    ..control()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("deadline_exceeded")
        );
        // Cancellation is checked on cache hits, not merely on descriptor I/O.
        cancelled.store(false, Ordering::Relaxed);
        let mut reader = Reader {
            file: &file,
            control: DecodeControl {
                cancelled: &cancelled,
                ..control()
            },
            started: Instant::now(),
            limits: limits(),
            length: bytes.len() as u64,
            window: [0; 64],
            window_start: 0,
            window_len: 0,
            read_bytes: 0,
            header_bytes: 0,
            metadata_items: 0,
            packets: 0,
        };
        reader.byte(0).unwrap();
        cancelled.store(true, Ordering::Relaxed);
        assert!(
            reader
                .byte(1)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }

    fn sparse_blocks(count: u64, payload_size: u64) -> File {
        let file = tempfile::tempfile().unwrap();
        let prefix = prefix();
        let timestamp = uint(0xe7, 0);
        let block_header = [header(SIMPLE_BLOCK, payload_size), vec![0x81, 0, 0, 0x80]].concat();
        let block_size = header(SIMPLE_BLOCK, payload_size).len() as u64 + payload_size;
        let cluster_size = timestamp.len() as u64 + count * block_size;
        let cluster_header = header(CLUSTER, cluster_size);
        let segment_size = prefix.len() as u64 + cluster_header.len() as u64 + cluster_size;
        let leading = [
            ebml(),
            header(SEGMENT, segment_size),
            prefix,
            cluster_header,
            timestamp,
        ]
        .concat();
        let total = leading.len() as u64 + count * block_size;
        file.set_len(total).unwrap();
        file.write_all_at(&leading, 0).unwrap();
        for ordinal in 0..count {
            file.write_all_at(&block_header, leading.len() as u64 + ordinal * block_size)
                .unwrap();
        }
        file
    }

    #[test]
    fn sparse_oversized_block_is_rejected_without_reading_its_payload() {
        let file = sparse_blocks(1, 512 * 1024 * 1024);
        let err = validate(&file, limits(), control())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Block exceeds configured packet bytes"),
            "{err}"
        );
    }

    #[test]
    fn twelve_thousand_sparse_blocks_do_not_spend_one_page_per_frame() {
        let file = sparse_blocks(12_000, 8192);
        let charged = validate(&file, limits(), control()).unwrap();
        assert!(
            charged < 1024 * 1024,
            "{charged} bytes for 12000 sparse block headers"
        );
        let err = validate(
            &file,
            InputLimits {
                max_packets: 11_999,
                ..limits()
            },
            control(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("block count"), "{err}");
    }
}
