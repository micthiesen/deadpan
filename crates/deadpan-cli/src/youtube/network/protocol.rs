//! The local gateway accepts authenticated HTTPS CONNECT requests only.

pub(super) const HEADER_LIMIT: usize = 8192;

pub(super) struct Credentials {
    pub password: String,
    pub(super) authorization: String,
}

impl Credentials {
    pub fn new() -> Self {
        let password = uuid::Uuid::new_v4().simple().to_string();
        let authorization = format!("Basic {}", base64(format!("deadpan:{password}").as_bytes()));
        Self {
            password,
            authorization,
        }
    }
}

// Encoding a local proxy credential does not introduce a new runtime/network
// dependency. The receiver compares the exact encoded value, never decodes it.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        result.push(char::from(TABLE[usize::from(a >> 2)]));
        result.push(char::from(TABLE[usize::from(((a & 3) << 4) | (b >> 4))]));
        result.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from(((b & 15) << 2) | (c >> 6))])
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(c & 63)])
        } else {
            '='
        });
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    Unauthenticated,
    Restricted,
}

/// Domain policy applies to every connection, including a redirect or a DASH
/// fragment. The helper still owns end-to-end TLS and certificate validation.
pub(super) fn supported_host(host: &str) -> bool {
    if host.len() > 253
        || host.is_empty()
        || !host.is_ascii()
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return false;
    }
    let host = host.to_ascii_lowercase();
    host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "googlevideo.com"
        || host.ends_with(".googlevideo.com")
        || host == "ytimg.com"
        || host.ends_with(".ytimg.com")
        || matches!(
            host.as_str(),
            "youtubei.googleapis.com"
                | "jnn-pa.googleapis.com"
                | "www.google.com"
                | "consent.google.com"
        )
}

pub(super) fn destination(header: &[u8], credentials: &Credentials) -> Result<String, Refusal> {
    if header.len() > HEADER_LIMIT || !header.ends_with(b"\r\n\r\n") || !header.is_ascii() {
        return Err(Refusal::Unauthenticated);
    }
    let text = std::str::from_utf8(header).map_err(|_| Refusal::Unauthenticated)?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let request = lines.next().ok_or(Refusal::Unauthenticated)?;
    let mut authorization = None;
    let mut host_header = None;
    let mut invalid = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            invalid = true;
            continue;
        };
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b.is_ascii_control() && b != b'\t')
        {
            invalid = true;
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("proxy-authorization") {
            if authorization.replace(value).is_some() {
                invalid = true;
            }
        } else if name.eq_ignore_ascii_case("host") {
            if host_header.replace(value).is_some() {
                invalid = true;
            }
        } else if name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            invalid = true;
        }
    }
    let authorized = authorization.is_some_and(|value| {
        let actual = value.as_bytes();
        let expected = credentials.authorization.as_bytes();
        actual.len() == expected.len()
            && actual
                .iter()
                .zip(expected)
                .fold(0u8, |difference, (a, b)| difference | (a ^ b))
                == 0
    });
    if !authorized {
        return Err(Refusal::Unauthenticated);
    }
    let parts: Vec<_> = request.split(' ').collect();
    if invalid
        || parts.len() != 3
        || parts[0] != "CONNECT"
        || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1")
    {
        return Err(Refusal::Restricted);
    }
    let target = parts[1];
    if host_header.is_some_and(|host| !host.eq_ignore_ascii_case(target)) {
        return Err(Refusal::Restricted);
    }
    let host = target.strip_suffix(":443").ok_or(Refusal::Restricted)?;
    if !supported_host(host) {
        return Err(Refusal::Restricted);
    }
    Ok(host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_encoding_uses_standard_padding() {
        for (plain, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), expected);
        }
    }

    #[test]
    fn every_tunnel_requires_the_owned_credential_and_a_supported_https_destination() {
        let credentials = Credentials::new();
        for host in [
            "www.youtube.com",
            "youtubei.googleapis.com",
            "rr3---sn-abc.googlevideo.com",
            "I.YTIMG.COM",
        ] {
            let request = format!(
                "CONNECT {host}:443 HTTP/1.1\r\nHost: {host}:443\r\nProxy-Authorization: {}\r\n\r\n",
                credentials.authorization
            );
            assert_eq!(
                destination(request.as_bytes(), &credentials).unwrap(),
                host.to_ascii_lowercase()
            );
            assert_eq!(
                destination(request.as_bytes(), &Credentials::new()),
                Err(Refusal::Unauthenticated)
            );
        }
        for target in [
            "127.0.0.1:443",
            "[::1]:443",
            "youtube.com.evil.test:443",
            "evilyoutube.com:443",
            "www.youtube.com:80",
            "www.youtube.com.:443",
            "user@www.youtube.com:443",
            "www.youtube.com:443/path",
            "file:///tmp/video",
            "accounts.google.com:443",
        ] {
            let request = format!(
                "CONNECT {target} HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
                credentials.authorization
            );
            assert_eq!(
                destination(request.as_bytes(), &credentials),
                Err(Refusal::Restricted),
                "{target}"
            );
        }
    }

    #[test]
    fn refuses_plain_http_smuggling_ambiguous_headers_and_oversized_requests() {
        let credentials = Credentials::new();
        for (request, extra) in [
            ("GET https://www.youtube.com/ HTTP/1.1", ""),
            (
                "CONNECT www.youtube.com:443 HTTP/1.1",
                "Content-Length: 5\r\n",
            ),
            (
                "CONNECT www.youtube.com:443 HTTP/1.1",
                "Host: localhost\r\n",
            ),
            (
                "CONNECT www.youtube.com:443 HTTP/1.1",
                "Host: www.youtube.com:443\r\nHost: www.youtube.com:443\r\n",
            ),
            ("CONNECT  www.youtube.com:443 HTTP/1.1", ""),
        ] {
            let header = format!(
                "{request}\r\nProxy-Authorization: {}\r\n{extra}\r\n",
                credentials.authorization
            );
            assert_eq!(
                destination(header.as_bytes(), &credentials),
                Err(Refusal::Restricted)
            );
        }
        let header = format!(
            "CONNECT www.youtube.com:443 HTTP/1.1\r\nProxy-Authorization: {}\r\nX: {}\r\n\r\n",
            credentials.authorization,
            "x".repeat(HEADER_LIMIT)
        );
        assert_eq!(
            destination(header.as_bytes(), &credentials),
            Err(Refusal::Unauthenticated)
        );
    }

    #[test]
    fn adversarial_youtube_connect() {
        use deadpan_chaos::{Target, Verdict, fuzz};

        // Captured from the pinned yt-dlp against an owned loopback proxy,
        // using an intentionally public test credential. No upstream request.
        let seed = include_bytes!("../../../tests/fixtures/youtube-connect.bin");
        let credentials = Credentials {
            password: "fixture".into(),
            authorization: "Basic ZGVhZHBhbjpmaXh0dXJl".into(),
        };
        let wrong = Credentials {
            password: "wrong".into(),
            authorization: "Basic ZGVhZHBhbjp3cm9uZw==".into(),
        };
        assert_eq!(destination(seed, &credentials).unwrap(), "www.youtube.com");
        let report = fuzz(
            Target::bytes("cli-youtube-connect")
                .iterations(2_000)
                .max_input_bytes(HEADER_LIMIT * 2)
                .max_case_alloc(256 * 1024),
            vec![seed.to_vec()],
            |input| match destination(input, &credentials) {
                Ok(host) => {
                    if !supported_host(&host)
                        || !host.is_ascii()
                        || host != host.to_ascii_lowercase()
                        || destination(input, &wrong) != Err(Refusal::Unauthenticated)
                    {
                        return Err("accepted an unsupported or unauthenticated tunnel".into());
                    }
                    Ok(Verdict::Accepted)
                }
                Err(reason) => Ok(Verdict::Rejected(format!("{reason:?}"))),
            },
        );
        report.assert_clean();
    }
}
