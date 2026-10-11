//! Small, optional YouTube confirmation pictures. Fetch and decode on the
//! import worker, before its confirmation callback; never on the UI thread.
//! Neither metadata cookies nor media stream URLs enter this client.

use std::fmt;
use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

mod network;
mod url;
mod webp;

use url::{supported_host, supported_url};

const MAX_BYTES: usize = 4 << 20;
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 4 << 20;
const MAX_RASTER_BYTES: u64 = 32 << 20;
const PREVIEW_WIDTH: u32 = 480;
const PREVIEW_HEIGHT: u32 = 270;
const TIMEOUT: Duration = Duration::from_secs(4);

/// Owned, bounded straight RGBA8. This picture is confirmation UI only; it
/// never becomes an asset, qualification receipt, proxy or render input.
#[derive(Clone, PartialEq, Eq)]
pub struct Thumbnail {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thumbnail")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

impl Thumbnail {
    /// Decode prepared thumbnail bytes on a worker. Header dimensions and
    /// decoder allocation limits apply before decoding the full raster.
    pub fn decode(bytes: &[u8]) -> Result<Self, Unavailable> {
        if bytes.is_empty() {
            return Err(Unavailable::InvalidImage);
        }
        if bytes.len() > MAX_BYTES {
            return Err(Unavailable::TooLarge);
        }
        let format = image::guess_format(bytes).map_err(|_| Unavailable::UnsupportedImage)?;
        if !matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
        ) {
            return Err(Unavailable::UnsupportedImage);
        }
        if format == ImageFormat::WebP {
            webp::validate(bytes)?;
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        // Decoder scratch limits are best effort in image. Independently
        // bound the actual output raster and WebP's encoded chunk ranges.
        limits.max_alloc = Some(MAX_RASTER_BYTES);
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(image_error)?;
        let (width, height) = decoder.dimensions();
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > MAX_PIXELS
            || decoder.total_bytes() > MAX_RASTER_BYTES
        {
            return Err(Unavailable::TooLarge);
        }
        let orientation = decoder.orientation().map_err(image_error)?;
        let mut image = DynamicImage::from_decoder(decoder).map_err(image_error)?;
        image.apply_orientation(orientation);
        let image = if image.width() > PREVIEW_WIDTH || image.height() > PREVIEW_HEIGHT {
            image.thumbnail(PREVIEW_WIDTH, PREVIEW_HEIGHT)
        } else {
            image
        };
        let image = image.to_rgba8();
        Ok(Self {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }

    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

fn image_error(error: image::ImageError) -> Unavailable {
    match error {
        image::ImageError::Limits(_) => Unavailable::TooLarge,
        _ => Unavailable::InvalidImage,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThumbnailState {
    Missing,
    Ready(Arc<Thumbnail>),
    Unavailable(Unavailable),
}

/// Only bounded, non-secret explanations cross into the confirmation UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Address,
    Network,
    Timeout,
    TooLarge,
    UnsupportedImage,
    InvalidImage,
}

impl Unavailable {
    pub fn description(self) -> &'static str {
        match self {
            Self::Address => "The thumbnail address is not a supported public YouTube image.",
            Self::Network => {
                "The thumbnail could not be downloaded. You can still import the video."
            }
            Self::Timeout => "The thumbnail request timed out. You can still import the video.",
            Self::TooLarge => "The thumbnail exceeds the image size limit.",
            Self::UnsupportedImage => {
                "The thumbnail uses an unsupported image format or animation."
            }
            Self::InvalidImage => "The thumbnail could not be decoded.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

#[derive(Debug)]
enum FetchError {
    Cancelled,
    Unavailable(Unavailable),
}

trait Fetch {
    fn fetch(&self, url: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, FetchError>;
}

/// Missing/unavailable thumbnails do not refuse an otherwise valid import.
/// Cancellation remains a cancellation and never reaches confirmation.
pub fn load(
    video_id: &str,
    url: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<ThumbnailState, Cancelled> {
    load_with(video_id, url, cancelled, &Https)
}

fn load_with(
    video_id: &str,
    url: Option<&str>,
    cancelled: &AtomicBool,
    transport: &dyn Fetch,
) -> Result<ThumbnailState, Cancelled> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }
    let Some(url) = url else {
        return Ok(ThumbnailState::Missing);
    };
    if !supported_url(video_id, url) {
        return Ok(ThumbnailState::Unavailable(Unavailable::Address));
    }
    let bytes = match transport.fetch(url, cancelled) {
        Ok(bytes) => bytes,
        Err(FetchError::Cancelled) => return Err(Cancelled),
        Err(FetchError::Unavailable(reason)) => {
            return if cancelled.load(Ordering::Relaxed) {
                Err(Cancelled)
            } else {
                Ok(ThumbnailState::Unavailable(reason))
            };
        }
    };
    if cancelled.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }
    let image = Thumbnail::decode(&bytes);
    if cancelled.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }
    Ok(match image {
        Ok(image) => ThumbnailState::Ready(Arc::new(image)),
        Err(reason) => ThumbnailState::Unavailable(reason),
    })
}

struct Https;

impl Fetch for Https {
    fn fetch(&self, url: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, FetchError> {
        use ureq::unversioned::transport::DefaultConnector;
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(super::helpers::USER_AGENT)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_global(Some(TIMEOUT))
            .timeout_connect(Some(TIMEOUT))
            .timeout_recv_response(Some(TIMEOUT))
            .timeout_recv_body(Some(TIMEOUT))
            .build();
        let agent =
            ureq::Agent::with_parts(config, DefaultConnector::default(), network::PublicResolver);
        let mut response = agent.get(url).call().map_err(network_error)?;
        if response.status().as_u16() != 200 {
            return Err(FetchError::Unavailable(Unavailable::Network));
        }
        if response
            .headers()
            .get("content-length")
            .is_some_and(|value| {
                value
                    .to_str()
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .is_none_or(|size| size > MAX_BYTES as u64)
            })
        {
            return Err(FetchError::Unavailable(Unavailable::TooLarge));
        }
        let reader = response
            .body_mut()
            .with_config()
            .limit(MAX_BYTES as u64 + 1)
            .reader();
        read_body(reader, cancelled)
    }
}

fn network_error(error: ureq::Error) -> FetchError {
    FetchError::Unavailable(if matches!(error, ureq::Error::Timeout(_)) {
        Unavailable::Timeout
    } else {
        Unavailable::Network
    })
}

fn read_body(mut reader: impl Read, cancelled: &AtomicBool) -> Result<Vec<u8>, FetchError> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(FetchError::Cancelled);
        }
        let remaining = (MAX_BYTES + 1)
            .saturating_sub(bytes.len())
            .min(buffer.len());
        if remaining == 0 {
            return Err(FetchError::Unavailable(Unavailable::TooLarge));
        }
        let read = reader
            .read(&mut buffer[..remaining])
            .map_err(|error| network_error(error.into()))?;
        if read == 0 {
            return Ok(bytes);
        }
        if read > MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(FetchError::Unavailable(Unavailable::TooLarge));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests;
