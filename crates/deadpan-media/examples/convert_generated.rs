//! Developer qualification entrypoint, not candidate admission or app packaging.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    use deadpan_media::protocol::{ConversionRequest, MAX_REQUEST_BYTES};
    use deadpan_media::{InputIdentity, canonicalize};

    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 5 {
        return Err(
            "usage: convert_generated WORKER INPUT REQUEST_JSON INPUT_SHA256 OUTPUT".into(),
        );
    }
    let mut json = Vec::new();
    File::open(&arguments[2])?
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut json)?;
    if json.len() > MAX_REQUEST_BYTES {
        return Err("request exceeds byte budget".into());
    }
    let request: ConversionRequest = serde_json::from_slice(&json)?;
    let digest = arguments[3].to_str().ok_or("SHA-256 must be UTF-8")?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("SHA-256 must be 64 lowercase hexadecimal digits".into());
    }
    let mut sha256 = [0u8; 32];
    for (index, byte) in sha256.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest[2 * index..2 * index + 2], 16)?;
    }
    let started = std::time::Instant::now();
    let mut media = canonicalize(
        Path::new(&arguments[0]),
        &mut File::open(&arguments[1])?,
        InputIdentity { sha256 },
        &request,
        &AtomicBool::new(false),
    )?;
    let output = Path::new(&arguments[4]);
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let copied = std::io::copy(&mut media, &mut temporary)?;
    if copied != media.object().byte_length() {
        return Err("validated output length changed".into());
    }
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scope": "developer conversion qualification; not candidate acceptance",
            "request": request,
            "input_sha256": digest,
            "object": media.object(),
            "report": media.report(),
            "elapsed_seconds": started.elapsed().as_secs_f64(),
        }))?
    );
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("generated media qualification requires a supported Unix host");
    std::process::exit(1);
}
