//! Closed thumbnail URL admission, independent of transport and decoding.

pub(crate) fn supported_url(video_id: &str, url: &str) -> bool {
    if video_id.len() != 11
        || !video_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        || url.len() > 2048
        || !url.is_ascii()
        || url
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    if !supported_host(host) || path.contains('#') {
        return false;
    }
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let mut segments = path.split('/');
    matches!(segments.next(), Some("vi" | "vi_webp"))
        && segments.next() == Some(video_id)
        && segments.next().is_some_and(|name| {
            name.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                && [".jpg", ".jpeg", ".png", ".webp"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
        })
        && segments.next().is_none()
}

pub(crate) fn supported_host(host: &str) -> bool {
    matches!(
        host,
        "i.ytimg.com" | "i1.ytimg.com" | "i2.ytimg.com" | "i3.ytimg.com" | "i4.ytimg.com"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "Z4C82eyhwgU";

    #[test]
    fn admits_only_supported_images_of_the_expected_video() {
        for host in [
            "i.ytimg.com",
            "i1.ytimg.com",
            "i2.ytimg.com",
            "i3.ytimg.com",
            "i4.ytimg.com",
        ] {
            for path in [
                "maxresdefault.jpg",
                "hq720.jpg",
                "default.jpeg",
                "0.png",
                "sddefault.webp",
            ] {
                for family in ["vi", "vi_webp"] {
                    let url = format!("https://{host}/{family}/{ID}/{path}?sqp=Q%3D%3D&rs=sample");
                    assert!(supported_url(ID, &url), "{url}");
                    assert!(!supported_url("aaaaaaaaaaa", &url));
                }
            }
        }
    }

    #[test]
    fn refuses_local_authorities_credentials_ports_path_traversal_and_header_injection() {
        for url in [
            "http://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg",
            "file:///private/etc/passwd",
            "https://127.0.0.1/vi/Z4C82eyhwgU/default.jpg",
            "https://[::1]/vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com.evil.invalid/vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com@127.0.0.1/vi/Z4C82eyhwgU/default.jpg",
            "https://user:secret@i.ytimg.com/vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com:443/vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com:80/vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com./vi/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/../default.jpg",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/%2e%2e%2fdefault.jpg",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg/extra",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg#fragment",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg?x=#fragment",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg\r\nCookie: secret",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.jpg?x=two words",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.avif",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/default.svg",
            "https://i.ytimg.com/storyboard/Z4C82eyhwgU/default.jpg",
            "https://i.ytimg.com/vi/Z4C82eyhwgU/",
            "https://i.ytimg.com\\@127.0.0.1/vi/Z4C82eyhwgU/default.jpg",
        ] {
            assert!(!supported_url(ID, url), "{url}");
        }
        assert!(!supported_url(
            ID,
            &format!(
                "https://i.ytimg.com/vi/{ID}/default.jpg?x={}",
                "x".repeat(2048)
            )
        ));
        assert!(!supported_url(
            "bad/id",
            "https://i.ytimg.com/vi/bad/id/default.jpg"
        ));
    }
}
