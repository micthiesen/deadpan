//! Pure YouTube URL normalization.
//!
//! Only HTTPS URLs on known YouTube hosts that name exactly one video are
//! accepted. The result is the 11-character video ID and a canonical watch
//! URL built from it; the user's own URL, its tracking parameters and any
//! playlist context are never passed to the downloader.

use std::fmt;

/// Longest accepted input. Real share URLs are far shorter.
pub const MAX_URL_BYTES: usize = 2048;
const ID_LENGTH: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoId(String);

impl VideoId {
    pub fn new(value: &str) -> Result<Self, UrlError> {
        if value.len() == ID_LENGTH
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(UrlError::MalformedId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The canonical URL handed to the downloader and stored as provenance.
    pub fn watch_url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.0)
    }
}

impl fmt::Display for VideoId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    #[error("the URL is empty, too long or contains spaces or non-ASCII characters")]
    Malformed,
    #[error("only HTTPS YouTube URLs are supported")]
    NotHttps,
    #[error(
        "only youtube.com, m.youtube.com, music.youtube.com, youtu.be and youtube-nocookie.com URLs are supported"
    )]
    UnsupportedHost,
    #[error("the URL has a user name, password or port, which YouTube URLs never need")]
    UnexpectedAuthority,
    #[error("this is a playlist; choose a specific video from it")]
    PlaylistWithoutVideo,
    #[error("this YouTube URL does not name a video")]
    NoVideo,
    #[error("the video ID must be 11 letters, digits, '-' or '_'")]
    MalformedId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Host {
    Main,
    Short,
    NoCookie,
}

/// Normalize a user-supplied YouTube URL to its single video.
pub fn normalize(input: &str) -> Result<VideoId, UrlError> {
    let input = input.trim();
    if input.is_empty()
        || input.len() > MAX_URL_BYTES
        || !input.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(UrlError::Malformed);
    }
    let (scheme, rest) = input.split_once("://").ok_or(UrlError::NotHttps)?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(UrlError::NotHttps);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, rest) = rest.split_at(end);
    if authority.contains(['@', ':']) {
        return Err(UrlError::UnexpectedAuthority);
    }
    let host = match authority.to_ascii_lowercase().as_str() {
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => Host::Main,
        "youtu.be" | "www.youtu.be" => Host::Short,
        "youtube-nocookie.com" | "www.youtube-nocookie.com" => Host::NoCookie,
        _ => return Err(UrlError::UnsupportedHost),
    };
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let parameter = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    let id = match (host, segments.as_slice()) {
        (Host::Short, [id]) => *id,
        (Host::Main, ["watch"]) => match parameter("v") {
            Some(id) => id,
            None if parameter("list").is_some() => return Err(UrlError::PlaylistWithoutVideo),
            None => return Err(UrlError::NoVideo),
        },
        (Host::Main, ["playlist"]) => match parameter("v") {
            Some(id) => id,
            None => return Err(UrlError::PlaylistWithoutVideo),
        },
        (Host::Main, ["shorts" | "embed" | "live" | "v", id]) | (Host::NoCookie, ["embed", id]) => {
            *id
        }
        _ if parameter("list").is_some() && parameter("v").is_none() => {
            return Err(UrlError::PlaylistWithoutVideo);
        }
        _ => return Err(UrlError::NoVideo),
    };
    VideoId::new(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "dQw4w9WgXcQ";

    #[test]
    fn accepts_every_supported_video_form() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ",
            "HTTPS://WWW.YOUTUBE.COM/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=abc",
            "https://www.youtube.com/watch?feature=youtu.be&v=dQw4w9WgXcQ#t=10",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?si=tracking&t=42",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=share",
            "https://www.youtube.com/embed/dQw4w9WgXcQ?start=3",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/live/dQw4w9WgXcQ",
            "https://www.youtube.com/v/dQw4w9WgXcQ",
            "  https://www.youtube.com/watch?v=dQw4w9WgXcQ  ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL123&index=4",
            "https://www.youtube.com/playlist?list=PL123&v=dQw4w9WgXcQ",
        ] {
            assert_eq!(normalize(url).map(|id| id.0), Ok(ID.to_owned()), "{url}");
        }
    }

    #[test]
    fn canonical_url_drops_tracking_and_playlist_context() {
        let id = normalize("https://youtu.be/dQw4w9WgXcQ?si=abc&list=PL1").unwrap();
        assert_eq!(
            id.watch_url(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn playlists_require_a_specific_video() {
        for url in [
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.youtube.com/watch?list=PL123",
            "https://m.youtube.com/playlist?list=PL123&index=2",
        ] {
            assert_eq!(normalize(url), Err(UrlError::PlaylistWithoutVideo), "{url}");
        }
    }

    #[test]
    fn rejects_other_schemes_hosts_and_authorities() {
        let cases = [
            (
                "http://www.youtube.com/watch?v=dQw4w9WgXcQ",
                UrlError::NotHttps,
            ),
            ("file:///etc/passwd", UrlError::NotHttps),
            ("www.youtube.com/watch?v=dQw4w9WgXcQ", UrlError::NotHttps),
            (
                "https://evil.example/watch?v=dQw4w9WgXcQ",
                UrlError::UnsupportedHost,
            ),
            (
                "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
                UrlError::UnsupportedHost,
            ),
            (
                "https://notyoutube.com/watch?v=dQw4w9WgXcQ",
                UrlError::UnsupportedHost,
            ),
            (
                "https://127.0.0.1/watch?v=dQw4w9WgXcQ",
                UrlError::UnsupportedHost,
            ),
            (
                "https://www.youtube.com./watch?v=dQw4w9WgXcQ",
                UrlError::UnsupportedHost,
            ),
            (
                "https://user@www.youtube.com/watch?v=dQw4w9WgXcQ",
                UrlError::UnexpectedAuthority,
            ),
            (
                "https://www.youtube.com:8443/watch?v=dQw4w9WgXcQ",
                UrlError::UnexpectedAuthority,
            ),
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXc",
                UrlError::MalformedId,
            ),
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQQ",
                UrlError::MalformedId,
            ),
            (
                "https://www.youtube.com/watch?v=dQw4w9Wg%2FQ",
                UrlError::MalformedId,
            ),
            ("https://youtu.be/../etc/passwd", UrlError::NoVideo),
            ("https://www.youtube.com/channel/UC123", UrlError::NoVideo),
            ("https://www.youtube.com/", UrlError::NoVideo),
            (
                "https://www.youtube.com/watch?v=dQw4w9 WgXcQ",
                UrlError::Malformed,
            ),
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ\u{e9}",
                UrlError::Malformed,
            ),
            ("", UrlError::Malformed),
        ];
        for (url, error) in cases {
            assert_eq!(normalize(url), Err(error), "{url}");
        }
        assert_eq!(
            normalize(&format!(
                "https://youtu.be/{ID}?{}",
                "a".repeat(MAX_URL_BYTES)
            )),
            Err(UrlError::Malformed)
        );
    }

    /// Every valid ID survives every supported form unchanged, and arbitrary
    /// text never panics or yields a non-canonical ID.
    #[test]
    fn normalization_is_total_and_preserves_ids() {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2_000 {
            let id: String = (0..ID_LENGTH)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize] as char)
                .collect();
            for url in [
                format!("https://www.youtube.com/watch?v={id}"),
                format!("https://youtu.be/{id}?si=x"),
                format!("https://m.youtube.com/shorts/{id}"),
                format!("https://www.youtube.com/watch?list=PL&v={id}&index=3"),
            ] {
                assert_eq!(normalize(&url).unwrap().as_str(), id);
            }
            let noise: String = (0..(next() % 80))
                .map(|_| (next() % 128) as u8 as char)
                .collect();
            for candidate in [noise.clone(), format!("https://youtu.be/{noise}")] {
                if let Ok(id) = normalize(&candidate) {
                    assert!(VideoId::new(id.as_str()).is_ok());
                }
            }
        }
    }
}
