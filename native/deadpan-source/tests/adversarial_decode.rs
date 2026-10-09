//! Gate G out-of-process decoder fuzzing. Each case runs in a fresh child of
//! this test binary, so an FFmpeg or adapter abort, signal, sanitizer report,
//! hang or excessive resident memory is observed as a failure instead of
//! killing the runner. Mutation keeps the container intact and damages only
//! media payloads, so cases pass header admission and reach native decoding.
//! Run the same test with the ASan/UBSan setup through
//! `cargo xtask chaos --sanitize`; see docs/ADVERSARIAL.md.

use deadpan_chaos::{ChildRunner, Outcome, Target, Verdict, child_input, finish_child, fuzz};
use deadpan_source::{
    DecodeControl, DecodeLimits, SourceDecodeError, SourceDecoder,
    audio::{AudioDecodeLimits, AudioDecoder},
};
use std::{fs::File, io::Write as _, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}

/// A host fixture and the half-open payload region mutated in place.
struct Host {
    bytes: Vec<u8>,
    start: usize,
    end: usize,
}

fn hosts() -> Vec<Host> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut paths: Vec<PathBuf> = ["fixtures", "audio-fixtures"]
        .into_iter()
        .flat_map(|name| std::fs::read_dir(directory.join(name)).expect("fixtures"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("mp4" | "mkv" | "webm")
            )
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let bytes = std::fs::read(&path).ok()?;
            if bytes.len() > 128 * 1024 {
                return None;
            }
            let (start, end) = if bytes.get(4..8) == Some(b"ftyp") {
                let at = bytes.windows(4).position(|window| window == b"mdat")?;
                let size = u32::from_be_bytes(bytes[at - 4..at].try_into().ok()?) as usize;
                (at + 4, (at - 4 + size).min(bytes.len()))
            } else {
                // Matroska: everything from the first Cluster.
                let at = bytes
                    .windows(4)
                    .position(|window| window == [0x1f, 0x43, 0xb6, 0x75])?;
                (at, bytes.len())
            };
            (end > start).then_some(Host { bytes, start, end })
        })
        .collect()
}

/// Decodes up to eight pictures and every audio packet's metadata.
fn decode(bytes: &[u8]) -> Outcome {
    fn snapshot(bytes: &[u8]) -> File {
        let mut file = tempfile::tempfile().expect("snapshot");
        file.write_all(bytes).expect("write snapshot");
        file
    }
    fn class(error: SourceDecodeError) -> Outcome {
        match error {
            SourceDecodeError::Native { code, message }
                if code.is_empty() || message.is_empty() =>
            {
                Err(format!("untyped native failure {code:?} {message:?}"))
            }
            SourceDecodeError::Native { code, .. } => Ok(Verdict::Rejected(code)),
            other => Ok(Verdict::Rejected(other.to_string())),
        }
    }
    let limits = DecodeLimits {
        max_frames: 64,
        max_packets: 4096,
        ..DecodeLimits::default()
    };
    let mut verdict = match SourceDecoder::open(snapshot(bytes), limits, control()) {
        Ok(mut decoder) => {
            let mut verdict = Verdict::Accepted;
            for _ in 0..8 {
                match decoder.next_rgba(control()) {
                    Ok(Some(frame)) => {
                        let row = u64::from(frame.width) * 4;
                        let expected = frame.row_stride_bytes as u64 * u64::from(frame.height);
                        if (frame.row_stride_bytes as u64) < row
                            || frame.rgba.len() as u64 != expected
                        {
                            return Err("decoded RGBA size disagrees with its geometry".into());
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        verdict = class(error)?;
                        break;
                    }
                }
            }
            verdict
        }
        Err(error) => class(error)?,
    };
    if bytes.get(4..8) == Some(b"ftyp") || bytes.get(..4) == Some(&[0x1a, 0x45, 0xdf, 0xa3]) {
        match AudioDecoder::open_first(snapshot(bytes), AudioDecodeLimits::default(), control()) {
            Ok(mut decoder) => {
                for _ in 0..256 {
                    match decoder.next_metadata(control()) {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(error) => {
                            if verdict == Verdict::Accepted {
                                verdict = class(error)?;
                            }
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                if verdict == Verdict::Accepted {
                    verdict = class(error)?;
                }
            }
        }
    }
    Ok(verdict)
}

fn assemble(hosts: &[Host], input: &[u8]) -> Option<Vec<u8>> {
    let (selector, payload) = input.split_first()?;
    let host = &hosts[usize::from(*selector) % hosts.len()];
    let mut bytes = host.bytes.clone();
    let mut payload = payload.to_vec();
    payload.resize(host.end - host.start, 0);
    bytes[host.start..host.end].copy_from_slice(&payload);
    Some(bytes)
}

#[test]
fn adversarial_native_decoders_survive_payload_mutation() {
    let hosts = hosts();
    if let Some(input) = child_input() {
        let outcome = match assemble(&hosts, &input) {
            Some(bytes) => decode(&bytes),
            None => Ok(Verdict::Rejected("empty".into())),
        };
        finish_child(outcome);
    }
    assert!(hosts.len() >= 10, "decoder fixture hosts missing");
    let seeds = hosts
        .iter()
        .enumerate()
        .map(|(index, host)| {
            let mut seed = vec![u8::try_from(index).expect("few hosts")];
            seed.extend_from_slice(&host.bytes[host.start..host.end]);
            seed
        })
        .collect();
    let runner = ChildRunner::new("adversarial_native_decoders_survive_payload_mutation");
    let report = fuzz(
        Target::bytes("source-native-decode")
            .iterations(40)
            .max_input_bytes(128 * 1024)
            .max_case_time(Duration::from_secs(30))
            .minimize_budget(48),
        seeds,
        |input| runner.run(input),
    );
    report.assert_clean();
}
