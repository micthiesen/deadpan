//! Developer-only qualification of the actual rsmpeg FFmpeg 8 boundary.
//! This bounded fixture probe is not the application's persistent media index.

use rsmpeg::{
    avcodec::AVCodecContext,
    avformat::AVFormatContextInput,
    avutil::{AVFrame, AVMD5},
    error::RsmpegError,
    ffi,
};
use serde_json::{Value, json};
use std::{error::Error, ffi::CString, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const FRAME_COUNT: usize = 120;
const WIDTH: usize = 320;
const HEIGHT: usize = 180;
const DIGITS: [[u8; 7]; 10] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
];

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

struct Decoder {
    input: AVFormatContextInput,
    codec: AVCodecContext,
    stream_index: usize,
    time_base: ffi::AVRational,
    draining: bool,
}

impl Decoder {
    fn open(path: &str) -> Result<Self> {
        let input = AVFormatContextInput::open(&CString::new(path)?)?;
        let (stream_index, codec) = input
            .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
            .ok_or("video stream missing")?;
        let stream = &input.streams()[stream_index];
        let time_base = stream.time_base;
        require(time_base.num > 0 && time_base.den > 0, "positive time base")?;
        let mut codec = AVCodecContext::new(&codec);
        codec.apply_codecpar(&stream.codecpar())?;
        codec.set_pkt_timebase(time_base);
        codec.open(None)?;
        Ok(Self {
            input,
            codec,
            stream_index,
            time_base,
            draining: false,
        })
    }

    fn next(&mut self) -> Result<Option<AVFrame>> {
        // This cap also catches a broken drain state or unexpected unbounded input.
        let mut packets = 0;
        for _ in 0..10_000 {
            match self.codec.receive_frame() {
                Ok(frame) => return Ok(Some(frame)),
                Err(RsmpegError::DecoderFlushedError) => return Ok(None),
                Err(RsmpegError::DecoderDrainError) => {
                    require(!self.draining, "decoder requested input after EOF drain")?;
                }
                Err(error) => return Err(error.into()),
            }
            loop {
                packets += 1;
                require(packets <= 10_000, "bounded packets between video frames")?;
                match self.input.read_packet()? {
                    Some(packet) if usize::try_from(packet.stream_index)? == self.stream_index => {
                        // receive_frame returned EAGAIN, so send must accept this packet.
                        // Any DecoderFullError fails instead of dropping the packet.
                        self.codec.send_packet(Some(&packet))?;
                        break;
                    }
                    Some(_) => continue,
                    None => {
                        self.codec.send_packet(None)?;
                        self.draining = true;
                        break;
                    }
                }
            }
        }
        Err("bounded decoder iteration count exceeded".into())
    }

    fn seek(&mut self, pts: i64) -> Result<()> {
        self.input.seek(
            i32::try_from(self.stream_index)?,
            pts,
            ffi::AVSEEK_FLAG_BACKWARD as i32,
        )?;
        self.codec.flush_buffers();
        self.draining = false;
        Ok(())
    }
}

fn pixels(frame: &AVFrame) -> Result<Vec<u8>> {
    require(
        frame.width == WIDTH as i32
            && frame.height == HEIGHT as i32
            && frame.format == ffi::AV_PIX_FMT_YUV420P,
        "fixture geometry and pixel format",
    )?;
    require(frame.decode_error_flags == 0, "no concealed decode errors")?;
    let size = frame.image_get_buffer_size(1)?;
    require(
        size == WIDTH * HEIGHT * 3 / 2,
        "bounded packed YUV420 frame size",
    )?;
    let mut data = vec![0; size];
    require(
        frame.image_copy_to_buffer(&mut data, 1)? == size,
        "complete visible plane copy",
    )?;
    Ok(data)
}

fn identity(data: &[u8]) -> Result<usize> {
    require(data.len() >= WIDTH * HEIGHT, "luma plane bounds")?;
    let mut number = 0;
    for place in 0..3 {
        let mut rows = [0_u8; 7];
        for (row, bits) in rows.iter_mut().enumerate() {
            for col in 0..5 {
                let x = 96 + place * 42 + col * 6 + 3;
                let y = 16 + row * 6 + 3;
                if data[y * WIDTH + x] > 162 {
                    *bits |= 1 << (4 - col);
                }
            }
        }
        number = number * 10
            + DIGITS
                .iter()
                .position(|digit| *digit == rows)
                .ok_or("decoded authored frame number unreadable")?;
    }
    Ok(number)
}

fn md5(data: &[u8]) -> String {
    AVMD5::sum(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Record {
    pts: i64,
    duration: i64,
    md5: String,
    identity: usize,
    keyframe: bool,
}

fn qualify(path: &str, cadence: &str) -> Result<Value> {
    require(
        matches!(cadence, "cfr" | "vfr" | "offset"),
        "recognized fixture cadence",
    )?;
    let mut decoder = Decoder::open(path)?;
    let tb = decoder.time_base;
    let streams = decoder.input.streams().len();
    let mut records: Vec<Record> = Vec::with_capacity(FRAME_COUNT);
    let mut retained = None;
    let mut expected_ticks = if cadence == "offset" { 60_060_i128 } else { 0 };
    let started = Instant::now();
    while let Some(frame) = decoder.next()? {
        let index = records.len();
        require(index < FRAME_COUNT, "bounded authored frame count")?;
        let data = pixels(&frame)?;
        let authored = identity(&data)?;
        require(
            authored == index,
            format!("authored identity mismatch: expected={index} actual={authored}"),
        )?;
        let pts = frame.best_effort_timestamp;
        require(
            pts != ffi::AV_NOPTS_VALUE,
            "decoded presentation timestamp exists",
        )?;
        require(
            i128::from(pts) * i128::from(tb.num) * 30_000 == expected_ticks * i128::from(tb.den),
            format!("exact authored presentation timestamp at frame {index}"),
        )?;
        expected_ticks += 1001
            * if cadence == "vfr" {
                1 + (index % 3) as i128
            } else {
                1
            };
        if index == 0 {
            retained = Some(frame.clone());
        }
        records.push(Record {
            pts,
            duration: frame.duration,
            md5: md5(&data),
            identity: authored,
            keyframe: frame.flags & ffi::AV_FRAME_FLAG_KEY as i32 != 0,
        });
    }
    require(records.len() == FRAME_COUNT, "exact authored frame count")?;
    let linear_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut duration_mismatches = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let duration = records
            .get(index + 1)
            .map_or(record.duration, |next| next.pts - record.pts);
        let expected = 1001
            * if cadence == "vfr" {
                1 + (index % 3) as i128
            } else {
                1
            };
        let actual_numerator = i128::from(duration) * i128::from(tb.num) * 30_000;
        require(
            actual_numerator % i128::from(tb.den) == 0,
            "integral fixture duration ticks",
        )?;
        let actual_ticks = actual_numerator / i128::from(tb.den);
        if actual_ticks != expected {
            duration_mismatches.push(
                json!({"frame": index, "actual_ticks": actual_ticks, "expected_ticks": expected}),
            );
        }
    }
    let mut seeks = Vec::new();
    let requests = [0, 119, 1, 60, 14, 16, 29, 30, 61, 5, 118, 0];
    for target in requests.into_iter().chain((0..FRAME_COUNT).rev()) {
        let started = Instant::now();
        let expected = &records[target];
        decoder.seek(expected.pts)?;
        let mut found = false;
        let mut decoded = 0;
        while let Some(frame) = decoder.next()? {
            decoded += 1;
            require(decoded <= FRAME_COUNT, "bounded seek preroll")?;
            let pts = frame.best_effort_timestamp;
            if pts < expected.pts {
                continue;
            }
            require(pts == expected.pts, format!("seek skipped target {target}"))?;
            let data = pixels(&frame)?;
            require(
                identity(&data)? == target,
                format!("seek authored identity {target}"),
            )?;
            require(
                md5(&data) == expected.md5,
                format!("seek hash versus linear frame {target}"),
            )?;
            found = true;
            break;
        }
        require(found, format!("seek reaches exact target {target}"))?;
        seeks.push(
            json!({"frame": target, "pts": expected.pts, "decoded_frames": decoded,
                          "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0}),
        );
    }
    let retained = retained.ok_or("retained frame missing")?;
    require(
        md5(&pixels(&retained)?) == records[0].md5,
        "retained clone survives reuse and flush",
    )?;
    drop(decoder);
    require(
        md5(&pixels(&retained)?) == records[0].md5 && identity(&pixels(&retained)?)? == 0,
        "retained clone survives decoder and demuxer destruction",
    )?;
    let frames: Vec<Value> = records.iter().map(|record| json!({
        "pts": record.pts, "duration": record.duration, "authored_identity": record.identity,
        "md5": record.md5, "keyframe": record.keyframe,
    })).collect();
    let keyframes: Vec<usize> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.keyframe.then_some(i))
        .collect();
    Ok(
        json!({"path": path, "cadence": cadence, "stream_count": streams, "time_base": [tb.num, tb.den],
              "frame_count": records.len(), "frames": frames, "keyframe_index": keyframes,
              "linear_decode_ms": linear_ms, "seeks": seeks, "duration_mismatches": duration_mismatches,
              "retained_frame_survived_flush": true,
              "retained_frame_survived_decoder_and_demuxer_destruction": true}),
    )
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    require(args.len() == 3, "usage: rsmpeg-probe INPUT cfr|vfr|offset")?;
    let report = qualify(&args[1], &args[2])?;
    println!("{report}");
    // Emit all decode/seek evidence, but independently fail qualification if
    // container metadata loses an authored duration. Never guess that duration.
    require(
        report["duration_mismatches"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "authored indexed duration mismatch",
    )
}
