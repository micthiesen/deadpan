//! Fetch the existing CC-BY Caminandes fixture's confirmation thumbnail with
//! the production client, then retain its bounded, decoded raster and identity.
//! Usage: cargo run --locked -p deadpan-cli --example qualify_youtube_thumbnail -- NEW_DIRECTORY

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    use deadpan_cli::youtube::thumbnail::{self, ThumbnailState};
    use sha2::{Digest, Sha256};

    let mut arguments = std::env::args_os().skip(1);
    let directory =
        std::path::PathBuf::from(arguments.next().ok_or("provide a new output directory")?);
    if arguments.next().is_some() {
        return Err("expected only a new output directory".into());
    }
    std::fs::create_dir(&directory)?;
    let started = std::time::Instant::now();
    let url = "https://i.ytimg.com/vi/Z4C82eyhwgU/maxresdefault.jpg";
    let state = thumbnail::load("Z4C82eyhwgU", Some(url), &AtomicBool::new(false))
        .map_err(|_| "thumbnail was cancelled")?;
    let elapsed = started.elapsed().as_secs_f64();
    let ThumbnailState::Ready(thumbnail) = state else {
        return Err(format!("production thumbnail unavailable: {state:?}").into());
    };
    let [width, height] = thumbnail.dimensions();
    if [width, height] != [480, 270] {
        return Err(format!("fixture thumbnail changed dimensions to {width}×{height}").into());
    }
    let png = directory.join("thumbnail.png");
    image::save_buffer_with_format(
        &png,
        thumbnail.rgba(),
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    let binary = std::env::current_exe()?;
    let hash = |bytes: &[u8]| -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    let report = serde_json::json!({
        "video_id":"Z4C82eyhwgU", "thumbnail_url":url, "seconds":elapsed,
        "width":width, "height":height, "rgba_bytes":thumbnail.rgba().len(),
        "rgba_sha256":hash(thumbnail.rgba()),
        "png_sha256":hash(&std::fs::read(&png)?),
        "binary":binary, "binary_sha256":hash(&std::fs::read(&binary)?),
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    let mut file = std::fs::File::create_new(directory.join("report.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("thumbnail qualification: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("thumbnail qualification requires macOS or Linux");
    std::process::exit(1);
}
