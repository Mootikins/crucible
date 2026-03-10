use std::net::Ipv4Addr;

use proptest::prelude::*;

pub fn arb_url_scheme() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("http".to_string()),
        Just("https".to_string()),
        Just("ftp".to_string()),
        Just("file".to_string()),
        Just("javascript".to_string()),
        Just("data".to_string()),
        Just("gopher".to_string()),
    ]
}

pub fn arb_ipv4_private() -> impl Strategy<Value = String> {
    prop_oneof![
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("10.{b}.{c}.{d}")),
        (16u8..=31, any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("172.{b}.{c}.{d}")),
        (any::<u8>(), any::<u8>()).prop_map(|(c, d)| format!("192.168.{c}.{d}")),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("127.{b}.{c}.{d}")),
    ]
}

pub fn arb_ipv4_public() -> impl Strategy<Value = String> {
    any::<[u8; 4]>()
        .prop_map(Ipv4Addr::from)
        .prop_filter("must be routable/public-ish", |ip| {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
        })
        .prop_map(|ip| ip.to_string())
}

pub fn arb_ipv6_loopback() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("::1".to_string()),
        Just("0:0:0:0:0:0:0:1".to_string()),
        Just("[::1]".to_string()),
        Just("[0:0:0:0:0:0:0:1]".to_string()),
    ]
}

pub fn arb_hostname() -> impl Strategy<Value = String> {
    let valid = "[a-zA-Z0-9-]{1,20}(\\.[a-zA-Z0-9-]{1,20}){0,3}";
    prop_oneof![
        7 => valid.prop_map(|s| s.to_ascii_lowercase()),
        1 => Just("".to_string()),
        1 => "[ .]{1,8}".prop_map(|s| s.to_string()),
        1 => "[a-zA-Z0-9]{1,12}_+[a-zA-Z0-9]{1,12}".prop_map(|s| s.to_string()),
    ]
}

pub fn arb_endpoint_url() -> impl Strategy<Value = String> {
    (
        arb_url_scheme(),
        prop_oneof![
            arb_ipv4_private(),
            arb_ipv4_public(),
            arb_hostname(),
            arb_ipv6_loopback(),
        ],
        prop::option::of(1u16..=65535),
        "(/[a-zA-Z0-9._~!$&'()*+,;=:@%-]{0,24}){0,4}",
    )
        .prop_map(|(scheme, host, port, path)| {
            port.map_or_else(
                || format!("{scheme}://{host}{path}"),
                |port| format!("{scheme}://{host}:{port}{path}"),
            )
        })
}

pub fn arb_traversal_path() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9/_-]{0,24}\\.\\.[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        "[a-zA-Z0-9/_-]{0,24}\\x00[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        Just("../etc/passwd".to_string()),
        Just("safe/..\0/evil".to_string()),
    ]
}

pub fn arb_safe_path() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9/_-]{1,64}".prop_filter("no traversal or null byte", |s| {
        !s.contains("..") && !s.contains('\0')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn smoke_arb_url_scheme(scheme in arb_url_scheme()) {
            prop_assert!(!scheme.is_empty());
        }

        #[test]
        fn smoke_arb_ipv4_private(ip in arb_ipv4_private()) {
            let parsed: Ipv4Addr = ip.parse().expect("strategy must produce valid IPv4");
            prop_assert!(parsed.is_private() || parsed.is_loopback());
        }

        #[test]
        fn smoke_arb_ipv4_public(ip in arb_ipv4_public()) {
            let parsed: Ipv4Addr = ip.parse().expect("strategy must produce valid IPv4");
            prop_assert!(!parsed.is_private());
            prop_assert!(!parsed.is_loopback());
            prop_assert!(!parsed.is_link_local());
            prop_assert!(!parsed.is_broadcast());
            prop_assert!(!parsed.is_unspecified());
        }

        #[test]
        fn smoke_arb_ipv6_loopback(ip in arb_ipv6_loopback()) {
            let host = ip.trim_matches(['[', ']']);
            let parsed: std::net::Ipv6Addr = host.parse().expect("strategy must produce valid IPv6");
            prop_assert!(parsed.is_loopback());
        }

        #[test]
        fn smoke_arb_hostname(host in arb_hostname()) {
            prop_assert!(host.len() <= 84);
        }

        #[test]
        fn smoke_arb_endpoint_url(url in arb_endpoint_url()) {
            prop_assert!(url.contains("://"));
        }

        #[test]
        fn smoke_arb_traversal_path(path in arb_traversal_path()) {
            prop_assert!(path.contains("..") || path.contains('\0'));
        }

        #[test]
        fn smoke_arb_safe_path(path in arb_safe_path()) {
            prop_assert!(!path.contains(".."));
            prop_assert!(!path.contains('\0'));
        }
    }
}
