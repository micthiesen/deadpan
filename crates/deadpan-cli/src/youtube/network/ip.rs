use std::net::IpAddr;

/// Conservative public unicast admission for the named YouTube service/CDN hosts.
/// Translated/tunnel IPv6 and special-purpose address space are unnecessary
/// for those CDN hosts and are refused rather than resolving through them.
pub(crate) fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, _] = address.octets();
            a != 0
                && a != 10
                && a != 127
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 169 && b == 254)
                && !(a == 172 && (16..=31).contains(&b))
                && !(a == 192
                    && (b == 168 || (b == 0 && matches!(c, 0 | 2)) || (b == 88 && c == 99)))
                && !(a == 198 && (matches!(b, 18 | 19) || (b == 51 && c == 100)))
                && !(a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            segments[0] & 0xe000 == 0x2000
                && !(segments[0] == 0x2001 && (segments[1] < 0x0200 || segments[1] == 0x0db8))
                && segments[0] != 0x2002
                && !(segments[0] == 0x3fff && segments[1] & 0xf000 == 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_local_shared_documentation_multicast_and_embedded_private_addresses() {
        for address in [
            "0.0.0.0",
            "0.1.2.3",
            "10.2.3.4",
            "100.64.0.1",
            "100.127.255.254",
            "127.1.2.3",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.254",
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.254",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:8.8.8.8",
            "64:ff9b::a00:1",
            "100::1",
            "2001::1",
            "2001:2::1",
            "2001:db8::1",
            "2002:7f00:1::1",
            "3fff:fff::1",
            "fc00::1",
            "fd00::1",
            "fe80::1",
            "ff02::1",
        ] {
            assert!(!public_address(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn admits_public_cdn_addresses_and_checks_private_range_edges() {
        for address in [
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "142.250.1.1",
            "172.217.0.1",
            "216.58.1.1",
            "2001:4860::1",
            "2607:f8b0::1",
            "2a00:1450::1",
        ] {
            assert!(public_address(address.parse().unwrap()), "{address}");
        }
    }
}
