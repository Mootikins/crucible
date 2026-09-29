//! Tests for the request endpoint check. The RPC crossing is tested in
//! `tests/rpc_session_create_agent_e2e.rs`.

use super::*;

use proptest::prelude::*;

/// Drive the async check from a sync test body (proptest included).
fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime")
        .block_on(fut)
}

/// A resolver that a test must never reach: the check judges a literal host
/// with no DNS, and refuses a bad scheme before any lookup.
async fn no_dns(host: String, _port: u16) -> ResolvedAddrs {
    panic!("unexpected DNS lookup for {host}");
}

/// Literal-host check with nothing configured.
fn check(endpoint: &str) -> Result<(), String> {
    block_on(check_request_endpoint_with(endpoint, &[], no_dns))
}

/// Check with `configured` as the operator's endpoints. A hostname resolves
/// to loopback, as `localhost` does.
fn check_configured(endpoint: &str, configured: &[&str]) -> Result<(), String> {
    let configured: Vec<String> = configured.iter().map(|e| (*e).to_string()).collect();
    block_on(check_request_endpoint_with(
        endpoint,
        &configured,
        |_, _| async { Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]) },
    ))
}

/// Hostname check against a fixed answer set, in place of DNS.
fn check_resolving_to(endpoint: &str, answers: &[&str]) -> Result<(), String> {
    let answers: Vec<IpAddr> = answers.iter().map(|a| a.parse().expect("addr")).collect();
    block_on(check_request_endpoint_with(endpoint, &[], |_, _| async {
        Ok(answers)
    }))
}

/// A globally routable unicast IPv4 address, the only kind the check may
/// accept. The ranges are written here from the octets, not by a call to
/// `is_internal_target`, so the property is not a tautology.
fn arb_ipv4_public() -> impl Strategy<Value = Ipv4Addr> {
    any::<[u8; 4]>()
        .prop_map(Ipv4Addr::from)
        .prop_filter("globally routable unicast", |ip| {
            let [a, b, c, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && a != 0
                && !(a == 100 && (64..128).contains(&b))
                && !(a == 192 && b == 0 && c == 0)
                && !(a == 198 && b & 0xfe == 18)
                && a < 240
        })
}

fn arb_ipv4_private() -> impl Strategy<Value = String> {
    prop_oneof![
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("10.{b}.{c}.{d}")),
        (16u8..=31, any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("172.{b}.{c}.{d}")),
        (any::<u8>(), any::<u8>()).prop_map(|(c, d)| format!("192.168.{c}.{d}")),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("127.{b}.{c}.{d}")),
        (any::<u8>(), any::<u8>()).prop_map(|(c, d)| format!("169.254.{c}.{d}")),
    ]
}

proptest! {
    #[test]
    fn private_ipv4_endpoints_are_refused(ip in arb_ipv4_private()) {
        let endpoint = format!("http://{ip}/");
        prop_assert!(check(&endpoint).is_err(), "{endpoint}");
    }

    #[test]
    fn public_ipv4_endpoints_are_accepted_with_http_or_https(
        ip in arb_ipv4_public(),
        scheme in prop_oneof![Just("http"), Just("https")],
    ) {
        let endpoint = format!("{scheme}://{ip}/");
        prop_assert!(check(&endpoint).is_ok(), "{endpoint}");
    }

    #[test]
    fn non_http_schemes_are_refused(
        scheme in prop_oneof![
            Just("ftp"), Just("file"), Just("javascript"), Just("data"), Just("gopher"),
        ],
    ) {
        let endpoint = format!("{scheme}://example.com");
        prop_assert!(check(&endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn the_default_ollama_endpoint_is_configured_with_no_llm_config() {
    let configured = configured_endpoints(None, None);
    let configured: Vec<&str> = configured.iter().map(String::as_str).collect();
    assert!(check_configured("http://localhost:11434", &configured).is_ok());
    // A path on the same server is the same server.
    assert!(check_configured("http://localhost:11434/v1", &configured).is_ok());
}

#[test]
fn a_configured_endpoint_on_a_private_network_is_accepted() {
    // The documented LAN Ollama in `docs/Help/Config/llm.md`.
    let llm = LlmConfig {
        providers: [(
            "lan".to_string(),
            crucible_core::config::LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: Some(("http://192.168.1.100:11434").into()),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let configured = configured_endpoints(Some(&llm), None);
    let configured: Vec<&str> = configured.iter().map(String::as_str).collect();
    assert!(check_configured("http://192.168.1.100:11434", &configured).is_ok());
    // Another port on the same host is another server, which nobody configured.
    assert!(check_configured("http://192.168.1.100:22", &configured).is_err());
}

#[test]
fn the_chat_endpoint_is_configured() {
    let configured = configured_endpoints(None, Some("http://10.1.2.3:8080".to_string()));
    let configured: Vec<&str> = configured.iter().map(String::as_str).collect();
    assert!(check_configured("http://10.1.2.3:8080", &configured).is_ok());
}

#[test]
fn a_configured_origin_does_not_admit_another_scheme_or_host() {
    let configured = ["http://localhost:11434"];
    for endpoint in [
        "https://localhost:11434",
        "http://127.0.0.1:11434",
        "http://localhost:11435",
    ] {
        assert!(
            check_configured(endpoint, &configured).is_err(),
            "{endpoint}"
        );
    }
}

#[test]
fn an_unconfigured_loopback_endpoint_is_refused() {
    for endpoint in ["http://127.0.0.1:8080", "http://[::1]:8080"] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
    assert!(check_resolving_to("http://localhost:8080", &["127.0.0.1", "::1"]).is_err());
}

#[test]
fn private_addresses_are_refused() {
    for endpoint in ["http://10.0.0.1", "http://192.168.1.1", "http://172.16.0.1"] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn malformed_and_hostless_urls_are_refused() {
    for endpoint in ["not-a-url", "http://", "ftp://example.com"] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn a_hostname_that_resolves_to_public_addresses_is_accepted() {
    assert!(check_resolving_to("http://example.com", &["93.184.216.34"]).is_ok());
}

#[test]
fn a_hostname_that_resolves_to_an_internal_address_is_refused() {
    assert!(check_resolving_to("http://internal.corp", &["10.0.0.5"]).is_err());
    assert!(check_resolving_to("http://metadata.example/latest/", &["169.254.169.254"]).is_err());
}

#[test]
fn one_internal_answer_refuses_the_hostname() {
    // One public record to pass a check that reads only the first answer.
    assert!(check_resolving_to(
        "http://mixed.example",
        &["93.184.216.34", "169.254.169.254"]
    )
    .is_err());
}

#[test]
fn a_hostname_that_does_not_resolve_is_refused() {
    let unresolvable = block_on(check_request_endpoint_with(
        "http://nx.example",
        &[],
        |_, _| async {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no such host",
            ))
        },
    ));
    assert!(
        unresolvable.is_err(),
        "an unresolvable host must be refused"
    );

    let empty = block_on(check_request_endpoint_with(
        "http://nx.example",
        &[],
        |_, _| async { Ok(Vec::new()) },
    ));
    assert!(empty.is_err(), "an empty answer set must be refused");
}

#[test]
fn shorthand_literal_encodings_of_loopback_are_refused() {
    for endpoint in [
        "http://2130706433/",
        "http://127.1",
        "http://0x7f.0.0.1",
        "http://0177.0.0.1",
    ] {
        // The message names 127.0.0.1: the URL parser normalized the host and
        // the address check refused it. A parse failure would not name it.
        let err = check(endpoint).expect_err(endpoint);
        assert!(err.contains("127.0.0.1"), "{endpoint}: {err}");
    }
}

#[test]
fn ipv6_encodings_of_internal_ipv4_addresses_are_refused() {
    for endpoint in [
        "http://[::ffff:169.254.169.254]",
        "http://[::ffff:a9fe:a9fe]",
        "http://[::ffff:127.0.0.1]",
        "http://[::ffff:10.0.0.1]",
        "http://[::127.0.0.1]",
        "http://[::ffff:0:169.254.169.254]",
        "http://[2002:a9fe:a9fe::]",
        "http://[64:ff9b::169.254.169.254]",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn ipv6_outside_global_unicast_is_refused() {
    for endpoint in [
        "http://[::1]",
        "http://[100::1]",
        "http://[ff02::1]",
        "http://[64:ff9b:1::a9fe]",
        "http://[2001:db8::1]",
        "http://[fc00::1]",
        "http://[fd12:3456::1]",
        "http://[fe80::1]",
        "http://[fec0::1]",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn global_unicast_ipv6_is_accepted() {
    assert!(check("http://[2606:4700::1111]").is_ok());
}

#[test]
fn reserved_and_shared_ipv4_ranges_are_refused() {
    for endpoint in [
        "http://0.1.2.3",
        "http://100.64.0.1",
        "http://192.0.0.1",
        "http://198.18.0.1",
        "http://224.0.0.1",
        "http://240.0.0.1",
    ] {
        assert!(check(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn an_internal_host_after_userinfo_is_refused() {
    assert!(check("http://example.com@169.254.169.254/").is_err());
}
